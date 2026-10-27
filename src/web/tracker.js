/*! Incisive web tracker. Usage:
 *  <script defer src="https://HOST/js/incisive.js" data-project="PROJECT_ID" data-key="WRITE_KEY"></script>
 *  incisive.track("signup", {plan: "pro"}); incisive.identify("user-42", {plan: "pro"});
 * Options (data- attributes): data-host, data-spa="false" (no history tracking),
 * data-respect-dnt="true", data-cookie-domain=".example.com" (share the visitor across subdomains).
 */
(function (w, d) {
  "use strict";
  var VERSION = "1.0.0";
  var SESSION_TIMEOUT = 30 * 60 * 1000;
  var script = d.currentScript || d.querySelector("script[data-project][src*='incisive']");
  if (!script) return;
  var attr = function (n) { return script.getAttribute("data-" + n); };
  var project = attr("project"), key = attr("key");
  if (!project || !key) { if (w.console) console.warn("incisive: data-project and data-key are required"); return; }
  var host = (attr("host") || new URL(script.src, location.href).origin).replace(/\/$/, "");
  var cookieDomain = attr("cookie-domain");
  if (attr("respect-dnt") === "true" && (navigator.doNotTrack === "1" || w.doNotTrack === "1")) return;

  // ---- storage: localStorage with a first-party cookie fallback ----
  function readCookie(n) {
    var m = d.cookie.match(new RegExp("(?:^|; )" + n + "=([^;]*)"));
    return m ? decodeURIComponent(m[1]) : null;
  }
  function get(n) {
    var v = null;
    try { v = localStorage.getItem(n); } catch (e) {}
    if (v == null) v = readCookie(n);
    try { return v ? JSON.parse(v) : null; } catch (e) { return null; }
  }
  function set(n, obj, days) {
    var v = JSON.stringify(obj);
    try { localStorage.setItem(n, v); } catch (e) {}
    var c = n + "=" + encodeURIComponent(v) + "; path=/; SameSite=Lax; max-age=" + Math.round(days * 86400);
    if (cookieDomain) c += "; domain=" + cookieDomain;
    if (location.protocol === "https:") c += "; Secure";
    d.cookie = c;
  }
  function uid() {
    if (w.crypto && crypto.randomUUID) return crypto.randomUUID();
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function (c) {
      var r = (Math.random() * 16) | 0;
      return (c === "x" ? r : (r & 3) | 8).toString(16);
    });
  }
  function iso(t) { return new Date(t).toISOString(); }

  // ---- campaign and referrer ----
  var UTM = ["source", "medium", "campaign", "term", "content", "id"];
  var CLICK_IDS = ["gclid", "gbraid", "wbraid", "fbclid", "msclkid", "ttclid", "li_fat_id", "twclid"];
  function params(url) {
    try { return new URL(url).searchParams; } catch (e) { return new URLSearchParams(""); }
  }
  function utmOf(url) {
    var p = params(url), out = null;
    UTM.forEach(function (k) { var v = p.get("utm_" + k); if (v) { out = out || {}; out[k] = v; } });
    return out;
  }
  function clickIdOf(url) {
    var p = params(url);
    for (var i = 0; i < CLICK_IDS.length; i++) if (p.get(CLICK_IDS[i])) return CLICK_IDS[i];
    return null;
  }
  function externalReferrer() {
    var r = d.referrer;
    if (!r) return null;
    try { if (new URL(r).hostname === location.hostname) return null; } catch (e) { return null; }
    return r;
  }

  // ---- visitor and session ----
  var now = Date.now();
  var visitor = get("_inc_v");
  var firstVisit = !visitor;
  if (!visitor) visitor = { id: uid(), first_seen_at: iso(now), sessions: 0 };
  var identity = get("_inc_u") || {};

  function newSession(t) {
    visitor.sessions = (visitor.sessions || 0) + 1;
    set("_inc_v", visitor, 730);
    var s = {
      id: uid(), started_at: iso(t), last: t, pageviews: 0,
      landing_url: location.href, landing_path: location.pathname,
      referrer: externalReferrer(), utm: utmOf(location.href), click_id: clickIdOf(location.href)
    };
    set("_inc_s", s, 1);
    return s;
  }
  function currentSession(t) {
    var s = get("_inc_s");
    var expired = !s || t - s.last > SESSION_TIMEOUT || new Date(s.started_at).getUTCDate() !== new Date(t).getUTCDate();
    // A visit arriving with new campaign parameters starts a new session, as in most analytics tools.
    var u = utmOf(location.href);
    if (!expired && u && JSON.stringify(u) !== JSON.stringify(s.utm)) expired = true;
    if (expired) { s = newSession(t); s.isNew = true; }
    return s;
  }

  // ---- context shared by every event ----
  function context(s) {
    var scr = w.screen || {};
    var ctx = {
      visitor: {
        id: visitor.id, is_new: firstVisit && visitor.sessions <= 1,
        first_seen_at: visitor.first_seen_at, session_count: visitor.sessions
      },
      session: {
        id: s.id, started_at: s.started_at,
        landing_page: { url: s.landing_url, path: s.landing_path }
      },
      device: {
        screen: { width: scr.width, height: scr.height, pixel_ratio: w.devicePixelRatio || 1 },
        viewport: { width: w.innerWidth, height: w.innerHeight },
        touch: "ontouchstart" in w || navigator.maxTouchPoints > 0
      },
      browser: { language: navigator.language },
      ip_address: "${keen.ip}",
      user_agent: { raw: "${keen.user_agent}" },
      tracker: { name: "incisive.js", version: VERSION }
    };
    try { ctx.browser.timezone = Intl.DateTimeFormat().resolvedOptions().timeZone; } catch (e) {}
    if (s.referrer) ctx.session.referrer = { url: s.referrer };
    if (s.utm) ctx.session.utm = s.utm;
    if (s.click_id) ctx.session.click_id = { type: s.click_id };
    if (identity.user_id) ctx.visitor.user_id = identity.user_id;
    if (identity.traits) ctx.visitor.traits = identity.traits;
    return ctx;
  }

  function merge(a, b) {
    for (var k in b) {
      if (!Object.prototype.hasOwnProperty.call(b, k)) continue;
      if (b[k] && typeof b[k] === "object" && !Array.isArray(b[k]) && a[k] && typeof a[k] === "object") merge(a[k], b[k]);
      else a[k] = b[k];
    }
    return a;
  }

  // ---- transport: beacon, then fetch keepalive, then an image ----
  function send(collection, ev) {
    var url = host + "/3.0/projects/" + encodeURIComponent(project) + "/events/" + encodeURIComponent(collection) + "?api_key=" + encodeURIComponent(key);
    var body = JSON.stringify(ev);
    try {
      if (navigator.sendBeacon && navigator.sendBeacon(url, new Blob([body], { type: "text/plain" }))) return;
    } catch (e) {}
    try {
      if (w.fetch) { fetch(url, { method: "POST", body: body, keepalive: true, mode: "no-cors", headers: { "Content-Type": "text/plain" } }); return; }
    } catch (e) {}
    var data = btoa(unescape(encodeURIComponent(body)));
    new Image().src = url + "&c=img&data=" + encodeURIComponent(data);
  }

  function touch(s, t) { s.last = t; set("_inc_s", s, 1); }

  function performanceTimings() {
    try {
      var n = performance.getEntriesByType("navigation")[0];
      if (!n) return null;
      var p = { ttfb_ms: Math.round(n.responseStart), dom_interactive_ms: Math.round(n.domInteractive) };
      if (n.loadEventEnd > 0) p.load_ms = Math.round(n.loadEventEnd);
      return p;
    } catch (e) { return null; }
  }

  var lastPath = null, sentPerf = false;
  function pageview(extra) {
    var t = Date.now();
    var s = currentSession(t);
    if (s.isNew) {
      delete s.isNew;
      send("web_session", context(s));
    }
    s.pageviews += 1;
    touch(s, t);
    var ev = context(s);
    ev.session.pageview_index = s.pageviews;
    ev.session.elapsed_seconds = Math.round((t - new Date(s.started_at).getTime()) / 1000);
    ev.page = { url: location.href, path: location.pathname, title: d.title };
    if (d.referrer) ev.page.referrer = lastPath ? location.origin + lastPath : d.referrer;
    var c = navigator.connection;
    if (c && c.effectiveType) ev.connection = { effective_type: c.effectiveType };
    if (!sentPerf) { var perf = performanceTimings(); if (perf) { ev.performance = perf; sentPerf = true; } }
    lastPath = location.pathname + location.search;
    send("web_pageview", extra ? merge(ev, extra) : ev);
  }

  function track(collection, props) {
    var t = Date.now();
    var s = currentSession(t);
    if (s.isNew) { delete s.isNew; send("web_session", context(s)); }
    touch(s, t);
    var ev = context(s);
    ev.page = { url: location.href, path: location.pathname, title: d.title };
    send(collection, merge(ev, props || {}));
  }

  function identify(userId, traits) {
    identity = { user_id: userId == null ? null : String(userId), traits: traits || identity.traits };
    set("_inc_u", identity, 730);
  }

  // ---- single-page apps ----
  function onNavigate() {
    var p = location.pathname + location.search;
    if (p !== lastPath) pageview();
  }
  if (attr("spa") !== "false" && w.history && history.pushState) {
    ["pushState", "replaceState"].forEach(function (m) {
      var orig = history[m];
      history[m] = function () { var r = orig.apply(this, arguments); setTimeout(onNavigate, 0); return r; };
    });
    w.addEventListener("popstate", onNavigate);
  }

  // ---- public API, replaying calls queued before load ----
  var api = { track: track, identify: identify, pageview: pageview, version: VERSION };
  var queued = (w.incisive && w.incisive.q) || [];
  w.incisive = api;
  queued.forEach(function (c) { if (api[c[0]]) api[c[0]].apply(null, Array.prototype.slice.call(c, 1)); });

  function start() {
    if (d.visibilityState === "prerender") {
      d.addEventListener("visibilitychange", function once() {
        if (d.visibilityState !== "prerender") { d.removeEventListener("visibilitychange", once); pageview(); }
      });
    } else {
      pageview();
    }
  }
  // Start as soon as the document is parsed; waiting for "load" can miss it entirely when the
  // script is injected late.
  if (d.readyState === "loading") d.addEventListener("DOMContentLoaded", start); else start();
})(window, document);
