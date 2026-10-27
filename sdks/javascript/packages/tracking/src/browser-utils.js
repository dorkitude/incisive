// Browser utilities: a JSON cookie jar, a delegated DOM listener, a form serializer,
// and a simple stopwatch.

export class Cookie {
  constructor(name) {
    if (!(this instanceof Cookie)) return new Cookie(name);
    this.config = { key: name, options: { expires: 365 } };
    this.data = this.get();
  }

  get(key) {
    let all = {};
    if (typeof document !== 'undefined' && document && typeof document.cookie === 'string') {
      const prefix = `${encodeURIComponent(this.config.key)}=`;
      const found = document.cookie.split('; ').find((c) => c.indexOf(prefix) === 0);
      if (found) {
        try {
          all = JSON.parse(decodeURIComponent(found.slice(prefix.length))) || {};
        } catch (e) {
          all = {};
        }
      }
    }
    if (!arguments.length) return all;
    return typeof all[key] !== 'undefined' ? all[key] : null;
  }

  set(key, value, options = {}) {
    const data = this.get();
    if (key && typeof key === 'object') {
      Object.assign(data, key);
    } else if (typeof key === 'string') {
      data[key] = value;
    }
    this.data = data;
    this._write(data, { ...this.config.options, ...options });
    return this;
  }

  expire() {
    this._write({}, { expires: -1 });
    this.data = {};
    return this;
  }

  options(obj) {
    if (!arguments.length) return this.config.options;
    this.config.options = typeof obj === 'object' ? obj : {};
    return this;
  }

  enabled() {
    return typeof navigator !== 'undefined' && navigator ? Boolean(navigator.cookieEnabled) : false;
  }

  _write(data, options) {
    if (typeof document === 'undefined' || !document) return;
    const parts = [`${encodeURIComponent(this.config.key)}=${encodeURIComponent(JSON.stringify(data))}`];
    if (typeof options.expires === 'number') {
      const d = new Date(Date.now() + options.expires * 864e5);
      parts.push(`expires=${d.toUTCString()}`);
    }
    parts.push(`path=${options.path || '/'}`);
    if (options.domain) parts.push(`domain=${options.domain}`);
    if (options.secure) parts.push('secure');
    if (options.sameSite) parts.push(`samesite=${options.sameSite}`);
    document.cookie = parts.join('; ');
  }
}

// listener('a.cta').on('click', fn) — delegated listener registered on document
// (or window for the 'window' selector).
export function listener(selector) {
  const handlers = [];
  const target = () => (selector === 'window' ? window : document);
  return {
    on(eventType, fn) {
      if (typeof window === 'undefined') return this;
      const handler = (e) => {
        if (selector === 'window' || selector === 'document') return fn.call(e.target, e);
        let el = e.target;
        while (el && el !== document) {
          if (typeof el.matches === 'function' && el.matches(selector)) {
            return fn.call(el, e);
          }
          el = el.parentNode;
        }
        return undefined;
      };
      handlers.push({ eventType, fn, handler });
      target().addEventListener(eventType, handler, false);
      return this;
    },
    off(eventType, fn) {
      for (let i = handlers.length - 1; i >= 0; i--) {
        const h = handlers[i];
        if ((!eventType || h.eventType === eventType) && (!fn || h.fn === fn)) {
          target().removeEventListener(h.eventType, h.handler, false);
          handlers.splice(i, 1);
        }
      }
      return this;
    },
  };
}

// Serializes a form element to a nested object. Password fields are skipped by
// default (`ignoreTypes`), disabled fields can be skipped with `disabled: true`.
export function serializeForm(form, options = {}) {
  const ignoreTypes = options.ignoreTypes || ['password'];
  const result = {};
  const elements = (form && form.elements) || [];
  for (const el of Array.from(elements)) {
    if (!el.name) continue;
    if (options.disabled && el.disabled) continue;
    const type = (el.type || '').toLowerCase();
    if (ignoreTypes.indexOf(type) > -1) continue;
    if (['submit', 'button', 'reset', 'file', 'image'].indexOf(type) > -1) continue;
    if ((type === 'checkbox' || type === 'radio') && !el.checked) continue;
    let value = el.value;
    if (type === 'select-multiple') {
      value = Array.from(el.options || []).filter((o) => o.selected).map((o) => o.value);
    }
    if (Object.prototype.hasOwnProperty.call(result, el.name)) {
      result[el.name] = [].concat(result[el.name], value);
    } else {
      result[el.name] = value;
    }
  }
  return result;
}

export function timer(initial = 0) {
  let total = initial;
  let started = null;
  return {
    start() {
      if (started === null) started = Date.now();
      return this;
    },
    pause() {
      if (started !== null) {
        total += Date.now() - started;
        started = null;
      }
      return this;
    },
    value() {
      return Math.round((total + (started !== null ? Date.now() - started : 0)) / 1000);
    },
    clear() {
      total = 0;
      started = null;
      return this;
    },
  };
}
