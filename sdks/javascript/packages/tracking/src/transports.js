// Delivery strategies for events: fetch (default), the browser Beacon API,
// and the legacy image-pixel GET request.

import { base64Encode } from 'incisive-core/utils';

export const DEFAULT_RETRY = {
  limit: 10,
  initialDelay: 200,
  retryOnResponseStatuses: [408, 500, 502, 503, 504],
};

export class TrackingError extends Error {
  constructor(message, props = {}) {
    super(message);
    this.name = 'TrackingError';
    Object.assign(this, props);
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function isAbortError(err) {
  return err && (err.name === 'AbortError' || err.code === 20);
}

// fetch with retry on network errors and configured status codes.
// Backoff: initialDelay * 2^(attempt-1).
export async function fetchWithRetry(url, options, retryConfig = {}, signal) {
  const retry = { ...DEFAULT_RETRY, ...retryConfig };
  if (retry.retryOnResponseStatuses && !Array.isArray(retry.retryOnResponseStatuses)) {
    throw new TypeError('retry.retryOnResponseStatuses must be an array');
  }
  const retryOn = retry.retryOnResponseStatuses || [];
  let attempt = 0;
  for (;;) {
    let response;
    try {
      response = await fetch(url, { ...options, signal });
    } catch (err) {
      if (isAbortError(err)) throw new TrackingError('Request aborted', { error_code: 'RequestAborted', aborted: true });
      if (attempt >= retry.limit) throw err;
      attempt++;
      await sleep(retry.initialDelay * 2 ** (attempt - 1));
      continue;
    }
    if (retryOn.indexOf(response.status) === -1 || attempt >= retry.limit) {
      return response;
    }
    attempt++;
    await sleep(retry.initialDelay * 2 ** (attempt - 1));
  }
}

async function readJson(response) {
  const text = await response.text();
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch (e) {
    return { message: text };
  }
}

// A single-event response must say {created: true}; a batch response must report
// success for every event of every collection.
export function eventsSavedSuccessfully(response) {
  if (!response || typeof response !== 'object') return false;
  if (typeof response.created !== 'undefined') return Boolean(response.created);
  return Object.keys(response).every((collection) => {
    const results = response[collection];
    return !Array.isArray(results) || results.every((r) => r && r.success);
  });
}

export async function sendFetch(client, url, data) {
  const options = {
    method: 'POST',
    body: JSON.stringify(data),
    headers: {
      Authorization: client.writeKey(),
      'Content-Type': 'application/json',
    },
  };
  if (typeof window !== 'undefined') {
    options.mode = 'cors';
    options.redirect = 'follow';
    options.referrerPolicy = client.referrerPolicy() || 'unsafe-url';
  }
  if (client.config.keepalive) options.keepalive = true;
  if (client.config.fetchOptions) Object.assign(options, client.config.fetchOptions);

  const controller = typeof AbortController !== 'undefined' ? new AbortController() : undefined;
  const promise = (async () => {
    const response = await fetchWithRetry(url, options, client.config.retry, controller && controller.signal);
    const body = await readJson(response);
    if (!response.ok || (body && body.error_code)) {
      throw new TrackingError(body.message || `HTTP ${response.status}`, {
        ok: false,
        status: response.status,
        statusText: response.statusText,
        error_code: body.error_code,
        body: body.message,
      });
    }
    if (!eventsSavedSuccessfully(body)) {
      throw new TrackingError('Some events were not recorded', { ...body, response: body, ok: true, status: response.status });
    }
    return body;
  })();
  promise.abort = () => controller && controller.abort();
  return promise;
}

export function canUseBeaconApi() {
  return typeof navigator !== 'undefined' && navigator && typeof navigator.sendBeacon === 'function';
}

export function canUseImage() {
  return typeof document !== 'undefined' && document && typeof document.createElement === 'function';
}

export function sendBeaconApi(client, url, data) {
  const target = `${url}?api_key=${encodeURIComponent(client.writeKey())}`;
  const queued = navigator.sendBeacon(target, JSON.stringify(data));
  return Promise.resolve({ queued: Boolean(queued) });
}

export function getUrlMaxLength() {
  if (typeof navigator !== 'undefined' && navigator && navigator.userAgent) {
    const ua = navigator.userAgent;
    if (ua.indexOf('MSIE') !== -1 || ua.indexOf('Trident/') > 0) return 1900;
  }
  return 16000;
}

export function buildImageUrl(client, collection, data) {
  return client.url('events', encodeURIComponent(collection), {
    api_key: encodeURIComponent(client.writeKey()),
    data: encodeURIComponent(base64Encode(JSON.stringify(data))),
    modified: Date.now(),
  });
}

export function sendImage(client, collection, data) {
  const url = buildImageUrl(client, collection, data);
  if (url.length >= getUrlMaxLength()) {
    return Promise.reject(new TrackingError('Image request URL exceeds the browser length limit', { error_code: 'UrlTooLong' }));
  }
  return new Promise((resolve, reject) => {
    const img = document.createElement('img');
    img.onload = function onload() {
      const w = 'naturalWidth' in this ? this.naturalWidth + this.naturalHeight : this.width + this.height;
      if (w === 0) {
        reject(new TrackingError('Image request returned an empty image', { error_code: 'ImageError' }));
        return;
      }
      resolve({ created: true });
    };
    img.onerror = () => reject(new TrackingError('Image request failed', { error_code: 'ImageError' }));
    img.src = `${url}&c=clv1`;
  });
}
