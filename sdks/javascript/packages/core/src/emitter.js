// Minimal event emitter, mixed into both client instances and client classes.

function own(target) {
  if (!Object.prototype.hasOwnProperty.call(target, '_listeners')) {
    Object.defineProperty(target, '_listeners', { value: {}, writable: true, enumerable: false });
  }
  return target._listeners;
}

export const emitterMethods = {
  on(name, fn) {
    if (typeof fn !== 'function') return this;
    const all = own(this);
    (all[name] = all[name] || []).push(fn);
    return this;
  },

  once(name, fn) {
    const wrapper = (...args) => {
      this.off(name, wrapper);
      fn.apply(this, args);
    };
    wrapper.original = fn;
    return this.on(name, wrapper);
  },

  off(name, fn) {
    const all = own(this);
    if (name === undefined) {
      for (const key of Object.keys(all)) delete all[key];
      return this;
    }
    if (!fn) {
      delete all[name];
      return this;
    }
    all[name] = (all[name] || []).filter((l) => l !== fn && l.original !== fn);
    return this;
  },

  emit(name, ...args) {
    const list = own(this)[name] || [];
    for (const fn of list.slice()) {
      try {
        fn.apply(this, args);
      } catch (err) {
        // A listener failing must never break the caller's request flow.
        if (name !== 'error') this.emit('error', err);
      }
    }
    return this;
  },

  listeners(name) {
    return (own(this)[name] || []).slice();
  },
};

export function mixinEmitter(target) {
  Object.assign(target, emitterMethods);
  return target;
}
