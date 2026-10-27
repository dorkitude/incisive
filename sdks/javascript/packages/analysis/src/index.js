// incisive-analysis: run analyses, saved queries and dataset lookups, plus raw
// authenticated GET/POST/PUT/DELETE helpers for management endpoints.

import Client, { each, extend, serialize, mapKeysToUnderscore, mixinEmitter } from 'incisive-core';

export const VERSION = '0.1.0';

export class AnalysisError extends Error {
  constructor(message, props = {}) {
    super(message);
    this.name = 'AnalysisError';
    Object.assign(this, props);
  }
}

const NUMERIC_STRING = /^\s*-?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$/;

// Numbers stay numbers; numeric strings ("42", "1.5") become numbers; everything
// else (null, objects, words) is left alone.
function toNumber(obj, key) {
  if (!obj || typeof obj !== 'object') return;
  const value = obj[key];
  if (typeof value === 'string' && NUMERIC_STRING.test(value)) obj[key] = Number(value);
}

function coerceNumbers(json, params) {
  const result = json.result;
  if (Array.isArray(result)) {
    if (params.interval) {
      if (params.group_by) {
        if (params.analysis_type === 'extraction') return;
        result.forEach((row) => {
          if (row && Array.isArray(row.value)) row.value.forEach((group) => toNumber(group, 'result'));
        });
      } else {
        result.forEach((row) => toNumber(row, 'value'));
      }
    } else if (params.analysis_type !== 'extraction' && params.analysis_type !== 'select_unique') {
      result.forEach((row) => toNumber(row, 'result'));
    }
  } else {
    toNumber(json, 'result');
  }
}

function applyResultParsers(json, params, parsers) {
  const run = (value) => parsers.reduce((acc, fn) => fn(acc), value);
  const result = json.result;
  if (Array.isArray(result)) {
    if (params.interval) {
      if (params.group_by) {
        result.forEach((row) => (row.value || []).forEach((g) => { g.result = run(g.result); }));
      } else {
        result.forEach((row) => { row.value = run(row.value); });
      }
    } else {
      result.forEach((row) => { if (row && typeof row === 'object') row.result = run(row.result); });
    }
  } else if (result && typeof result === 'object') {
    Object.keys(result).forEach((k) => { result[k] = run(result[k]); });
  } else {
    json.result = run(result);
  }
}

async function readBody(response) {
  const text = await response.text();
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch (e) {
    return { message: text, error_code: response.ok ? 'InvalidResponse' : undefined };
  }
}

function appendQuery(url, query) {
  if (!query) return url;
  return url + (url.indexOf('?') === -1 ? '?' : '&') + query;
}

export class IncisiveAnalysis extends Client {
  constructor(config = {}) {
    super(config);
    this._cache = new Map();
  }

  // --------------------------------------------------------------------
  // Queries
  // --------------------------------------------------------------------

  // query('count', { event_collection, timeframe, ... }, options?)
  // query({ analysis_type: 'count', ... })               single config object
  // query({ saved_query_name: 'name' })                  saved / cached query result
  // query({ dataset_name: 'name', index_by, timeframe }) cached dataset result
  // query('saved', 'name')                               legacy saved query form
  query(a, b = undefined, options = {}) {
    this._validateReadCredentials();
    const opts = { ...options };
    let analysisType = mapKeysToUnderscore(a);
    let params = mapKeysToUnderscore(b);

    if (a && typeof a === 'object' && !b) {
      const { analysis_type: type, cache, ...rest } = analysisType;
      analysisType = type;
      params = rest;
      opts.cache = cache !== undefined ? cache : this.config.cache;
    }
    if (opts.cache === undefined && this.config.cache) opts.cache = { ...this.config.cache };
    opts.resultParsers = this.config.resultParsers;

    if (params && typeof params === 'object' && (this.config.includeMetadata || this.config.include_metadata)) {
      params.include_metadata = true;
    }

    const apiKey = this.config.readKey || this.config.masterKey;

    if (analysisType && typeof params === 'string') {
      const path = params.indexOf('/result') < 0 ? `${params}/result` : params;
      return this.get({ url: this.url('queries', analysisType, path), api_key: apiKey }, opts);
    }

    if (params && params.saved_query_name) {
      const name = params.saved_query_name;
      const path = name.indexOf('/result') > -1 ? name : `${name}/result`;
      return this.get({ url: this.url('queries', 'saved', path), api_key: apiKey }, opts);
    }

    if (analysisType === 'dataset' && params && typeof params === 'object') {
      return this.get({ url: this.url('datasets', params.name, 'results'), api_key: apiKey, params }, opts);
    }

    if (params && params.dataset_name) {
      return this.get({ url: this.url('datasets', params.dataset_name, 'results'), api_key: apiKey, params }, opts);
    }

    if (analysisType && params && typeof params === 'object') {
      const body = extend({ analysis_type: analysisType }, params);
      if (!body.timezone) body.timezone = new Date().getTimezoneOffset() * -60 || 0;
      return this.post({ url: this.url('queries', analysisType), api_key: apiKey, params: body }, opts);
    }

    return Promise.reject(new AnalysisError('.query() called with incorrect arguments', {
      error_code: 'SDKError',
    }));
  }

  // run(query | [queries], callback?) — runs Query objects, saved query names or promises.
  run(q, callback) {
    const list = Array.isArray(q) ? q : [q];
    const promises = list.map((item) => {
      if (typeof item === 'string') return this.query('saved', `${item}/result`);
      if (item instanceof Query) {
        return this.query(item.analysis, extend({ analysis_type: item.analysis }, item.params), item.options);
      }
      return item;
    });
    const output = promises.length > 1 ? Promise.all(promises) : promises[0];
    if (callback) {
      output.then((res) => callback(null, res), (err) => callback(err, null));
    }
    return output;
  }

  // --------------------------------------------------------------------
  // Raw requests: { url, api_key, params } (+ filters/timeframe/timezone for del)
  // --------------------------------------------------------------------

  get(request, options) { return this._request('GET', request, options); }

  post(request, options) { return this._request('POST', request, options); }

  put(request, options) { return this._request('PUT', request, options); }

  del(request, options) { return this._request('DELETE', request, options); }

  _validateReadCredentials() {
    if (!this.config.projectId) throw new Error('Please provide valid project ID');
    if (!this.config.masterKey && !this.config.readKey) throw new Error('Please provide valid API key');
  }

  _request(method, request = {}, options = {}) {
    const apiKey = request.api_key || request.apiKey;
    if (!apiKey) throw new Error('Please provide a valid API key');
    let url = request.url;
    const params = request.params ? mapKeysToUnderscore(request.params) : {};

    if (method === 'DELETE') {
      // Delete-events filters travel in the query string as JSON.
      const parts = [];
      ['filters', 'timeframe', 'timezone'].forEach((key) => {
        if (request[key] === undefined) return;
        let value = request[key];
        if (typeof value === 'string') {
          try {
            value = JSON.parse(decodeURIComponent(value));
          } catch (e) {
            // keep plain strings such as "this_7_days"
          }
        }
        value = mapKeysToUnderscore(value);
        parts.push(`${key}=${encodeURIComponent(JSON.stringify(value))}`);
      });
      url = appendQuery(url, parts.join('&'));
    }

    // Echo the analysis type for generic requests to analysis endpoints.
    if (method !== 'DELETE' && params.analysis_type === undefined
      && url.indexOf('/queries/') > -1 && !/\/queries\/saved(\/|\?|$)/.test(url)) {
      params.analysis_type = url.split('/queries/').pop().split('?')[0];
    }

    const headers = {
      Authorization: apiKey,
      'Content-Type': 'application/json',
      ...(request.headers || {}),
    };
    const fetchOptions = { method, headers };
    if (method === 'GET' || method === 'DELETE') {
      const query = [`api_key=${encodeURIComponent(apiKey)}`];
      if (method === 'GET') {
        const serialized = serialize(params);
        if (serialized) query.push(serialized);
      }
      url = appendQuery(url, query.join('&'));
    } else {
      fetchOptions.body = JSON.stringify(params);
    }
    if (typeof window !== 'undefined') fetchOptions.mode = 'cors';

    const cacheConfig = options.cache && method !== 'DELETE' && method !== 'PUT' ? options.cache : null;
    const cacheKey = cacheConfig ? `${method} ${url} ${fetchOptions.body || ''}` : null;
    if (cacheKey) {
      const hit = this._cache.get(cacheKey);
      if (hit && (!hit.expires || hit.expires > Date.now())) {
        const cached = Promise.resolve(JSON.parse(hit.value));
        cached.abort = () => {};
        return cached;
      }
    }

    const controller = typeof AbortController !== 'undefined' ? new AbortController() : null;
    if (controller) fetchOptions.signal = controller.signal;
    const timeoutMs = options.timeout || this.config.timeout;
    let timer = null;
    if (controller && timeoutMs) {
      timer = setTimeout(() => controller.abort(), timeoutMs);
      if (typeof timer.unref === 'function') timer.unref();
    }

    const promise = (async () => {
      let response;
      try {
        response = await fetch(url, fetchOptions);
      } finally {
        if (timer) clearTimeout(timer);
      }
      if (response.ok && method === 'DELETE') return {};
      const json = await readBody(response);
      if (!response.ok || (json && json.error_code)) {
        throw new AnalysisError(json.message || `HTTP ${response.status}`, {
          ok: false,
          error_code: json.error_code,
          body: json.message,
          status: response.status,
          statusText: response.statusText,
        });
      }
      if (cacheKey) {
        this._cache.set(cacheKey, {
          value: JSON.stringify(json),
          expires: cacheConfig.maxAge ? Date.now() + cacheConfig.maxAge : undefined,
        });
      }
      if (json && Object.prototype.hasOwnProperty.call(json, 'result')) {
        coerceNumbers(json, params);
        if (Array.isArray(options.resultParsers) && options.resultParsers.length) {
          applyResultParsers(json, params, options.resultParsers);
        }
      }
      if (params.event_collection !== undefined && json.query === undefined) {
        return { query: params, ...json };
      }
      return json;
    })();
    promise.abort = () => controller && controller.abort();
    return promise;
  }
}

// Legacy query object, accepted by run().
export class Query {
  constructor(analysisType, params = {}, options = {}) {
    this.analysis = analysisType;
    this.params = {};
    this.set(params);
    this.options = { ...options };
  }

  set(attributes) {
    each(attributes, (value, key) => {
      this.params[key.replace(/([A-Z])/g, (c) => `_${c.toLowerCase()}`)] = mapKeysToUnderscore(value);
    });
    return this;
  }

  get(attribute) {
    const key = attribute.replace(/([A-Z])/g, (c) => `_${c.toLowerCase()}`);
    return this.params[key] !== undefined ? this.params[key] : null;
  }

  addFilter(property, operator, value) {
    this.params.filters = this.params.filters || [];
    this.params.filters.push({ property_name: property, operator, property_value: value });
    return this;
  }
}

mixinEmitter(IncisiveAnalysis);
IncisiveAnalysis.Query = Query;
IncisiveAnalysis.version = VERSION;

export default IncisiveAnalysis;
