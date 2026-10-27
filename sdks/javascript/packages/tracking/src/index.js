// incisive-tracking: record events from browsers and Node.js.

import Client, { each, extend, deepExtend, mixinEmitter } from 'incisive-core';
import {
  DEFAULT_RETRY,
  TrackingError,
  canUseBeaconApi,
  canUseImage,
  sendBeaconApi,
  sendFetch,
  sendImage,
} from './transports.js';
import * as helpers from './helpers.js';
import { Cookie, listener, serializeForm, timer } from './browser-utils.js';
import { initAutoTracking, defaultAddons } from './auto-tracking.js';
import { VERSION } from './version.js';

export { VERSION, defaultAddons };

const QUEUE_DEFAULTS = { capacity: 5000, interval: 15 };
const UNIQUE_CACHE_DEFAULTS = { maxAge: 60 * 1000 };

function localStorageAvailable() {
  try {
    return typeof localStorage !== 'undefined' && localStorage !== null;
  } catch (e) {
    return false;
  }
}

// Applies a list of extensions (objects or functions returning objects) onto `result`.
function applyExtensions(result, list) {
  each(list || [], (modifier) => {
    const value = typeof modifier === 'function' ? modifier() : modifier;
    deepExtend(result, value);
  });
  return result;
}

class EventQueue {
  constructor(config, onFlush) {
    this.config = { ...QUEUE_DEFAULTS, ...(config || {}) };
    this.events = {};
    this.capacity = 0; // number of queued events
    this.onFlush = onFlush;
    this.timer = null;
  }

  add(collection, events) {
    this.events[collection] = (this.events[collection] || []).concat(events);
    this.capacity += events.length;
    this.check();
    if (this.capacity > 0 && !this.timer) this.start();
  }

  check() {
    if (this.capacity > 0 && this.config.capacity > 0 && this.capacity >= this.config.capacity) {
      this.flush();
    }
    if (this.config.interval === 0 || this.capacity === 0) this.pause();
  }

  flush() {
    if (this.capacity === 0) return;
    const events = this.events;
    this.events = {};
    this.capacity = 0;
    this.pause();
    this.onFlush(events);
  }

  start() {
    this.pause();
    if (!this.config.interval) return;
    this.timer = setInterval(() => this.flush(), this.config.interval * 1000);
    if (this.timer && typeof this.timer.unref === 'function') this.timer.unref();
  }

  pause() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
}

export class IncisiveTracking extends Client {
  constructor(config = {}) {
    super(config);
    this.config.retry = { ...DEFAULT_RETRY, ...(config.retry || {}) };
    this.config.cache = { ...UNIQUE_CACHE_DEFAULTS, ...(config.cache || {}) };
    this.config.requestType = config.requestType || 'fetch';
    this._requestTypeExplicit = Boolean(config.requestType);
    this.extensions = { events: [], collections: {} };
    this.observers = {};
    this._uniqueHashes = [];
    this.queue = new EventQueue(config.queue, (events) => this._sendDeferred(events));

    if (typeof config.optOut !== 'undefined') {
      this.setOptOut(config.optOut);
      IncisiveTracking.optedOut = Boolean(config.optOut);
    }
    if (config.respectDoNotTrack && typeof navigator !== 'undefined' && navigator) {
      const dnt = navigator.doNotTrack || (typeof window !== 'undefined' && window.doNotTrack);
      IncisiveTracking.doNotTrack = dnt === '1' || dnt === 'yes';
    } else if (!config.respectDoNotTrack) {
      IncisiveTracking.doNotTrack = false;
    }
  }

  referrerPolicy(str) {
    if (!arguments.length) return this.config.referrerPolicy;
    this.config.referrerPolicy = str ? String(str) : null;
    return this;
  }

  // ----------------------------------------------------------------------
  // Recording
  // ----------------------------------------------------------------------

  // recordEvent('collection', {..}, callback?)
  // recordEvent({ collection, event, callback?, unique?, requestType?, cache? })
  recordEvent(collectionOrOptions, eventBody, callback) {
    let collection = collectionOrOptions;
    let options = {};
    if (collectionOrOptions && typeof collectionOrOptions === 'object') {
      options = collectionOrOptions;
      collection = options.collection || options.event_collection;
      eventBody = options.event;
      callback = options.callback;
    }

    if (!this._checkCredentials(callback)) return undefined;
    if (!collection || typeof collection !== 'string') {
      this._validationError('Collection name must be a string.', callback);
      return undefined;
    }

    const event = {};
    applyExtensions(event, this.extensions.events);
    applyExtensions(event, this.extensions.collections[collection]);
    applyExtensions(event, [extend({}, eventBody)]);

    if (options.unique && !this._isUnique(event, options.cache)) {
      return this._settle(Promise.resolve({ created: false, message: '[NOT_UNIQUE] This event has already been recorded' }), callback);
    }

    this.emit('recordEvent', collection, event);

    const blocked = this._blocked(callback);
    if (blocked !== null) return blocked;

    const requestType = options.requestType || this.config.requestType;
    const url = this.url('events', encodeURIComponent(collection));
    let promise;
    if (requestType === 'beaconAPI' && canUseBeaconApi()) {
      promise = sendBeaconApi(this, url, event);
    } else if ((requestType === 'img' || requestType === 'beacon') && canUseImage()) {
      promise = sendImage(this, collection, event);
    } else {
      promise = sendFetch(this, url, event);
    }
    return this._settle(promise, callback);
  }

  // recordEvents({ collectionA: [ {..}, {..} ], collectionB: [ ... ] }, callback?)
  recordEvents(eventsHash, callback) {
    if (!this._checkCredentials(callback)) return undefined;
    if (!eventsHash || typeof eventsHash !== 'object' || Array.isArray(eventsHash)) {
      this._validationError('First argument must be an object', callback);
      return undefined;
    }
    if (arguments.length > 2) {
      this._validationError('Incorrect arguments provided to #recordEvents method', callback);
      return undefined;
    }

    const payload = {};
    each(eventsHash, (list, collection) => {
      payload[collection] = payload[collection] || [];
      each(list, (body) => {
        const event = {};
        applyExtensions(event, this.extensions.events);
        applyExtensions(event, this.extensions.collections[collection]);
        applyExtensions(event, [body]);
        payload[collection].push(event);
      });
    });

    this.emit('recordEvents', payload);

    const blocked = this._blocked(callback);
    if (blocked !== null) return blocked;

    const url = this.url('events');
    const promise = this.config.requestType === 'beaconAPI' && canUseBeaconApi()
      ? sendBeaconApi(this, url, payload)
      : sendFetch(this, url, payload);
    return this._settle(promise, callback);
  }

  // ----------------------------------------------------------------------
  // Extensions (properties merged into every event)
  // ----------------------------------------------------------------------

  extendEvent(collection, modifier) {
    if (arguments.length !== 2 || typeof collection !== 'string'
      || (typeof modifier !== 'object' && typeof modifier !== 'function') || modifier === null) {
      this.emit('error', 'Event(s) not extended: Incorrect arguments provided to #extendEvent method');
      return undefined;
    }
    (this.extensions.collections[collection] = this.extensions.collections[collection] || []).push(modifier);
    this.emit('extendEvent', collection, modifier);
    return this;
  }

  extendEvents(modifier) {
    if (arguments.length !== 1 || (typeof modifier !== 'object' && typeof modifier !== 'function') || modifier === null) {
      this.emit('error', 'Event(s) not extended: Incorrect arguments provided to #extendEvents method');
      return undefined;
    }
    this.extensions.events.push(modifier);
    this.emit('extendEvents', modifier);
    return this;
  }

  // ----------------------------------------------------------------------
  // Deferred (batched) delivery
  // ----------------------------------------------------------------------

  deferEvent(collection, eventBody) {
    if (arguments.length !== 2 || typeof collection !== 'string') {
      this.emit('error', 'Event(s) not deferred: Incorrect arguments provided to #deferEvent method');
      return undefined;
    }
    this.emit('deferEvent', collection, eventBody);
    this.queue.add(collection, [eventBody]);
    return this;
  }

  deferEvents(eventsHash) {
    if (arguments.length !== 1 || !eventsHash || typeof eventsHash !== 'object') {
      this.emit('error', 'Event(s) not deferred: Incorrect arguments provided to #deferEvents method');
      return undefined;
    }
    this.emit('deferEvents', eventsHash);
    each(eventsHash, (list, collection) => this.queue.add(collection, [].concat(list)));
    return this;
  }

  queueCapacity(num) {
    if (!arguments.length) return this.queue.config.capacity;
    this.queue.config.capacity = num ? Number(num) : 0;
    this.queue.check();
    return this;
  }

  queueInterval(num) {
    if (!arguments.length) return this.queue.config.interval;
    this.queue.config.interval = num ? Number(num) : 0;
    if (this.queue.timer) this.queue.start();
    this.queue.check();
    return this;
  }

  recordDeferredEvents() {
    this.queue.flush();
    return this;
  }

  _sendDeferred(events) {
    this.emit('recordDeferredEvents', events);
    const promise = this.recordEvents(events, (err) => {
      if (err) this.emit('recordDeferredEventsError', err, events);
    });
    this._lastDeferred = promise;
    return promise;
  }

  // ----------------------------------------------------------------------
  // Privacy controls
  // ----------------------------------------------------------------------

  setOptOut(optOut = true) {
    IncisiveTracking.optedOut = Boolean(optOut);
    if (localStorageAvailable()) {
      try {
        if (optOut) localStorage.setItem('optout', String(optOut));
        else localStorage.removeItem('optout');
      } catch (e) {
        // storage can be disabled; the in-memory flag still applies
      }
    }
    return this;
  }

  initAutoTracking(options) {
    return initAutoTracking(this, IncisiveTracking, options);
  }

  // ----------------------------------------------------------------------
  // Internals
  // ----------------------------------------------------------------------

  _checkCredentials(callback) {
    if (!this.projectId()) {
      this._validationError('Client is missing a projectId property.', callback);
      return false;
    }
    if (!this.writeKey()) {
      this._validationError('Client is missing a writeKey property.', callback);
      return false;
    }
    return true;
  }

  _validationError(message, callback) {
    const err = `Event(s) not recorded: ${message}`;
    this.emit('error', err);
    if (callback) callback.call(this, err, null);
  }

  // Returns a promise when tracking is disabled/opted out, or null to proceed.
  _blocked(callback) {
    if (!IncisiveTracking.enabled) {
      this._validationError('Tracking is disabled (IncisiveTracking.enabled = false).', callback);
      return false;
    }
    if (IncisiveTracking.optedOut) {
      return this._settle(Promise.resolve({ created: false, message: 'User opted out of tracking.' }), callback);
    }
    if (IncisiveTracking.doNotTrack) {
      return this._settle(Promise.resolve({ created: false, message: 'Do Not Track is enabled.' }), callback);
    }
    return null;
  }

  _isUnique(event, cacheOverride) {
    const cache = { ...this.config.cache, ...(cacheOverride || {}) };
    const hash = JSON.stringify(event);
    const now = Date.now();
    this._uniqueHashes = this._uniqueHashes.filter((h) => !h.expires || h.expires > now);
    if (this._uniqueHashes.some((h) => h.hash === hash)) return false;
    this._uniqueHashes.push({ hash, expires: cache.maxAge ? now + cache.maxAge : undefined });
    return true;
  }

  // Wires an optional node-style callback to the promise. When a callback is
  // supplied the returned promise is marked handled so callback-only callers do not
  // trigger unhandled-rejection warnings.
  _settle(promise, callback) {
    const abort = promise.abort;
    const result = promise.then(
      (res) => {
        if (callback) callback.call(this, null, res);
        return res;
      },
      (err) => {
        this.emit('error', err);
        if (callback) callback.call(this, err, null);
        throw err;
      },
    );
    if (callback) result.catch(() => {});
    if (abort) result.abort = abort;
    return result;
  }
}

mixinEmitter(IncisiveTracking);
IncisiveTracking.enabled = true;
IncisiveTracking.optedOut = localStorageAvailable() && (() => {
  try {
    return Boolean(localStorage.getItem('optout'));
  } catch (e) {
    return false;
  }
})();
IncisiveTracking.doNotTrack = false;
IncisiveTracking.version = VERSION;
IncisiveTracking.helpers = { ...helpers };
IncisiveTracking.utils = { each, extend, deepExtend, cookie: Cookie, listener, serializeForm, timer };

// listenTo({ 'click .nav a': fn, 'submit form#signup': fn })
IncisiveTracking.listenTo = function listenTo(hash) {
  each(hash, (callback, key) => {
    const parts = key.split(' ');
    listener(parts.slice(1).join(' ')).on(parts[0], callback);
  });
};

export { TrackingError, helpers, Cookie, listener, serializeForm, timer };
export default IncisiveTracking;
