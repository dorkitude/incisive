// Shared client foundation: configuration, credentials, URL resources and events.

import { mixinEmitter } from './emitter.js';
import { each, extend, serialize, deepExtend, mapKeysToUnderscore, base64Encode, base64Decode } from './utils.js';

export const DEFAULT_HOST = 'localhost:8080';
export const DEFAULT_PROTOCOL = 'http';
export const DEFAULT_VERSION = '3.0';

export const defaultResources = {
  base: '{protocol}://{host}',
  version: '{base}/{version}',
  projects: '{base}/{version}/projects',
  projectId: '{base}/{version}/projects/{projectId}',
  events: '{base}/{version}/projects/{projectId}/events',
  queries: '{base}/{version}/projects/{projectId}/queries',
  datasets: '{base}/{version}/projects/{projectId}/datasets',
  keys: '{base}/{version}/projects/{projectId}/keys',
};

function stripTrailingSlash(str) {
  return String(str).replace(/\/+$/, '');
}

export class Client {
  constructor(config = {}) {
    this.config = {
      projectId: undefined,
      writeKey: undefined,
      readKey: undefined,
      masterKey: undefined,
      host: DEFAULT_HOST,
      protocol: DEFAULT_PROTOCOL,
      version: DEFAULT_VERSION,
      resources: extend({}, this.constructor.resources || defaultResources),
    };
    this.configure(config);
    this.emit('ready');
    if (typeof this.constructor.emit === 'function') this.constructor.emit('client', this);
  }

  configure(obj = {}) {
    const config = extend({}, obj);

    // A full base URL wins over host/protocol.
    if (config.baseUrl) {
      const base = stripTrailingSlash(config.baseUrl);
      const match = /^([a-z][a-z0-9+.-]*):\/\/(.*)$/i.exec(base);
      if (match) {
        config.protocol = match[1].toLowerCase();
        config.host = match[2];
      }
      config.resources = extend({}, this.config.resources, config.resources || {}, { base });
    } else if (config.host) {
      // Accept "https://example.com" in host too, deriving the protocol from it.
      const match = /^([a-z][a-z0-9+.-]*):\/\/(.*)$/i.exec(config.host);
      if (match) {
        if (!config.protocol) config.protocol = match[1].toLowerCase();
        config.host = match[2];
      }
      config.host = stripTrailingSlash(config.host);
    }

    if (config.protocol === 'auto' && typeof location !== 'undefined') {
      config.protocol = String(location.protocol).replace(/:/g, '');
    }

    if (config.resources && !config.baseUrl) {
      config.resources = extend({}, this.config.resources, config.resources);
    }

    extend(this.config, config);
    return this;
  }

  projectId(str) {
    if (!arguments.length) return this.config.projectId;
    this.config.projectId = str ? String(str) : null;
    return this;
  }

  masterKey(str) {
    if (!arguments.length) return this.config.masterKey;
    this.config.masterKey = str ? String(str) : null;
    return this;
  }

  readKey(str) {
    if (!arguments.length) return this.config.readKey;
    this.config.readKey = str ? String(str) : null;
    return this;
  }

  writeKey(str) {
    if (!arguments.length) return this.config.writeKey;
    this.config.writeKey = str ? String(str) : null;
    return this;
  }

  resources(obj) {
    if (!arguments.length) return this.config.resources;
    if (obj && typeof obj === 'object') {
      each(obj, (value, key) => {
        this.config.resources[key] = value || null;
      });
    }
    return this;
  }

  // url(name, ...segments, {query})
  // `name` is a resource name ("events", "queries", ...) or a path appended to the base.
  // String segments are appended as "/segment" verbatim (encode them yourself);
  // object arguments become "?k=v&k2=v2" verbatim.
  url(name, ...args) {
    const resources = this.config.resources;
    const baseTemplate = resources.base || '{protocol}://{host}';
    let path;
    if (name && typeof name === 'string') {
      path = resources[name] ? resources[name] : `{base}${name}`;
    } else {
      path = '{base}';
    }
    path = path.split('{base}').join(baseTemplate);
    each(this.config, (value, key) => {
      if (value === null || value === undefined || typeof value !== 'object') {
        path = path.split(`{${key}}`).join(value === undefined ? 'undefined' : String(value));
      }
    });
    for (const arg of args) {
      if (typeof arg === 'string') {
        path += `/${arg}`;
      } else if (arg && typeof arg === 'object') {
        const pairs = Object.keys(arg).map((k) => `${k}=${arg[k]}`);
        if (pairs.length) path += `?${pairs.join('&')}`;
      }
    }
    return path;
  }
}

mixinEmitter(Client.prototype);
mixinEmitter(Client);
Client.resources = defaultResources;
Client.debug = false;
Client.log = function log(...args) {
  if (Client.debug && typeof console === 'object') console.log('[incisive]', ...args);
};

export { each, extend, serialize, deepExtend, mapKeysToUnderscore, base64Encode, base64Decode, mixinEmitter };
export default Client;
