// Small dependency-free helpers shared by the tracking and analysis clients.

export function each(target, fn) {
  if (!target) return target;
  if (Array.isArray(target)) {
    for (let i = 0; i < target.length; i++) {
      if (fn(target[i], i, target) === false) break;
    }
    return target;
  }
  if (typeof target === 'object') {
    for (const key of Object.keys(target)) {
      if (fn(target[key], key, target) === false) break;
    }
  }
  return target;
}

// Shallow copy of own enumerable properties from each source onto target.
export function extend(target, ...sources) {
  for (const source of sources) {
    if (!source || typeof source !== 'object') continue;
    for (const key of Object.keys(source)) {
      target[key] = source[key];
    }
  }
  return target;
}

export function isPlainObject(value) {
  if (value === null || typeof value !== 'object') return false;
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}

// Recursive merge: plain objects are merged key by key, arrays and scalars replace.
export function deepExtend(target, ...sources) {
  for (const source of sources) {
    if (!source || typeof source !== 'object') continue;
    for (const key of Object.keys(source)) {
      const value = source[key];
      if (isPlainObject(value)) {
        if (!isPlainObject(target[key])) target[key] = {};
        deepExtend(target[key], value);
      } else if (Array.isArray(value)) {
        target[key] = value.map((item) => (isPlainObject(item) ? deepExtend({}, item) : item));
      } else {
        target[key] = value;
      }
    }
  }
  return target;
}

// Query-string serializer used by GET/DELETE requests: strings are sent as-is,
// every other value is JSON encoded; both are percent-encoded.
export function serialize(data) {
  const parts = [];
  each(data, (value, key) => {
    if (value === undefined) return;
    const str = typeof value === 'string' ? value : JSON.stringify(value);
    parts.push(`${key}=${encodeURIComponent(str)}`);
  });
  return parts.join('&');
}

// UTF-8 safe standard base64 (with + / = alphabet).
export function base64Encode(str) {
  const bytes = new TextEncoder().encode(String(str));
  let binary = '';
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

export function base64Decode(b64) {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return new TextDecoder().decode(bytes);
}

const UPPER = /[A-Z]/g;
const EXCLUDED_KEYS = ['cache'];

export function toUnderscore(key) {
  return key.replace(UPPER, (c) => `_${c.toLowerCase()}`);
}

// Recursively converts camelCase keys to snake_case (the HTTP API uses snake_case).
// The `cache` option is passed through untouched.
export function mapKeysToUnderscore(obj) {
  if (!obj || typeof obj !== 'object') return obj;
  if (Array.isArray(obj)) return obj.map(mapKeysToUnderscore);
  if (!isPlainObject(obj)) return obj;
  const out = {};
  for (const key of Object.keys(obj)) {
    const value = obj[key];
    if (EXCLUDED_KEYS.includes(key)) {
      out[key] = value;
      continue;
    }
    out[toUnderscore(key)] = mapKeysToUnderscore(value);
  }
  return out;
}
