// Environment helpers that produce commonly tracked properties.
// All browser-only helpers degrade gracefully (return empty data) outside a browser.

const hasWindow = () => typeof window !== 'undefined' && window;
const hasDocument = () => typeof document !== 'undefined' && document;

export function getUniqueId() {
  if (typeof crypto !== 'undefined' && crypto && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  // RFC 4122 v4 fallback
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

// Calendar breakdown of a date, useful for grouping without timezone math server side.
export function getDatetimeIndex(input) {
  const date = input || new Date();
  return {
    hour_of_day: date.getHours(),
    day_of_week: parseInt(1 + date.getDay(), 10),
    day_of_month: date.getDate(),
    month: parseInt(1 + date.getMonth(), 10),
    year: date.getFullYear(),
  };
}

// "www.example.co.uk" -> "example.co.uk" (best effort: keeps two labels, or three
// when the second-level label is short, like co.uk / com.au).
export function getDomainName(hostname) {
  if (!hostname) return hostname;
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(hostname) || hostname.indexOf('.') === -1) return hostname;
  const parts = hostname.split('.');
  if (parts.length <= 2) return hostname;
  const sld = parts[parts.length - 2];
  const keep = sld.length <= 3 && parts[parts.length - 1].length <= 2 ? 3 : 2;
  return parts.slice(-keep).join('.');
}

export function getScreenProfile() {
  if (typeof screen === 'undefined' || !screen) return {};
  const profile = {
    height: screen.height,
    width: screen.width,
    colorDepth: screen.colorDepth,
    pixelDepth: screen.pixelDepth,
    availHeight: screen.availHeight,
    availWidth: screen.availWidth,
    orientation: {},
  };
  if (screen.orientation) {
    profile.orientation = { angle: screen.orientation.angle, type: screen.orientation.type };
  }
  return profile;
}

export function getWindowProfile() {
  if (!hasWindow()) return {};
  const doc = hasDocument() ? document : {};
  const body = doc.body || {};
  const html = doc.documentElement || {};
  return {
    height: window.innerHeight || html.clientHeight || body.clientHeight || 0,
    width: window.innerWidth || html.clientWidth || body.clientWidth || 0,
    scrollHeight: Math.max(body.scrollHeight || 0, html.scrollHeight || 0, body.offsetHeight || 0, html.offsetHeight || 0),
  };
}

export function getBrowserProfile() {
  const nav = typeof navigator !== 'undefined' && navigator ? navigator : {};
  let description = null;
  if (hasDocument() && typeof document.querySelector === 'function') {
    const meta = document.querySelector('meta[name="description"]');
    description = meta ? meta.getAttribute('content') : null;
  }
  return {
    cookies: typeof nav.cookieEnabled !== 'undefined' ? nav.cookieEnabled : false,
    codeName: nav.appCodeName,
    description,
    language: nav.language,
    name: nav.appName,
    online: nav.onLine,
    platform: nav.platform,
    useragent: nav.userAgent,
    version: nav.appVersion,
    screen: getScreenProfile(),
    window: getWindowProfile(),
  };
}

export function getScrollState(previous) {
  const prev = previous && typeof previous === 'object' ? previous : {};
  if (!hasWindow()) return prev;
  const win = getWindowProfile();
  const offset = window.pageYOffset || (document.documentElement && document.documentElement.scrollTop) || 0;
  const pixel = offset + win.height;
  const height = win.scrollHeight || 1;
  const ratio = Math.min(1, Number((pixel / height).toFixed(2)));
  return {
    pixel,
    pixel_max: Math.max(prev.pixel_max || 0, pixel),
    ratio,
    ratio_max: Math.max(prev.ratio_max || 0, ratio),
  };
}

// CSS-ish path to an element, e.g. "body > div:eq(1) > a#cta".
export function getDomNodePath(el) {
  if (!el || !el.nodeName) return '';
  const stack = [];
  let node = el;
  while (node && node.parentNode && node.nodeName) {
    const name = node.nodeName.toLowerCase();
    if (node.id) {
      stack.unshift(`${name}#${node.id}`);
    } else {
      const siblings = Array.prototype.filter.call(node.parentNode.childNodes || [], (s) => s.nodeName === node.nodeName);
      const index = siblings.indexOf(node);
      stack.unshift(siblings.length > 1 ? `${name}:eq(${index})` : name);
    }
    node = node.parentNode;
  }
  return stack.filter((n) => n !== '#document').join(' > ');
}

export function getDomNodeProfile(el) {
  if (!el) return {};
  const attr = (name) => (typeof el.getAttribute === 'function' ? el.getAttribute(name) : undefined);
  return {
    action: el.action,
    class: el.className,
    href: el.href || null,
    id: el.id,
    event_key: attr('data-event-key'),
    method: el.method,
    name: el.name,
    node_name: el.nodeName,
    selector: getDomNodePath(el),
    text: el.text,
    title: el.title,
    type: el.type,
    x_position: el.offsetLeft || el.clientLeft || null,
    y_position: el.offsetTop || el.clientTop || null,
  };
}
