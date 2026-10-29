/*! Incisive web tracker v1.3.0
 *
 * Script tag (auto-tracks everything; no other code needed):
 *   <script defer src="https://YOUR_INCISIVE_HOST/js/incisive.js"
 *     data-project="PROJECT_ID" data-key="WRITE_KEY" data-host="https://API_HOST"></script>
 *   data-auto-track="false" turns auto-tracking off; a list such as "pageviews,clicks" picks parts.
 *   Other options: data-spa="false", data-respect-dnt="true", data-cookie-domain=".example.com".
 *
 * Programmatic, one call:
 *   incisive.trackAll({
 *     project: "PROJECT_ID", key: "WRITE_KEY", host: "https://API_HOST",
 *     pageviews: true, clicks: true, forms: true, engagement: true, views: true, errors: false,
 *     // Joined into every event. Functions run on each event and receive {collection, event}.
 *     properties: {app: {version: "2.3.1"}, cart: {items: function () { return cart.length; }}}
 *   });
 *   incisive.track("signup", {plan: "pro"});
 *   incisive.identify("user-42", {plan: "pro"});
 *   incisive.extend({experiment: {variant: "b"}});   // add more global properties later
 *
 * Mark elements with data-track="name" to name their clicks, data-track-view to record when they
 * scroll into view, and data-incisive-ignore to never record them. Form field values are never sent.
 */
(function (w, d) {
  "use strict";
  var VERSION = "1.3.0";
  var SESSION_TIMEOUT = 30 * 60 * 1000;
  var AUTO = ["pageviews", "clicks", "forms", "engagement", "views", "errors"];
  var AUTO_DEFAULT = { pageviews: true, clicks: true, forms: true, engagement: true, views: true, errors: false };
  var cfg = null;
  var globals = []; // property objects (or functions) joined into every event

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
    if (cfg && cfg.cookieDomain) c += "; domain=" + cfg.cookieDomain;
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
  function warn(m) { if (w.console) console.warn("incisive: " + m); }

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
  var visitor = null, firstVisit = false, identity = {};

  function loadVisitor() {
    visitor = get("_inc_v");
    firstVisit = !visitor;
    if (!visitor) visitor = { id: uid(), first_seen_at: iso(Date.now()), sessions: 0 };
    identity = get("_inc_u") || {};
  }
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
  // Returns the current session, starting one (and recording web_session) when needed.
  function session(t) {
    var s = get("_inc_s");
    var expired = !s || t - s.last > SESSION_TIMEOUT || new Date(s.started_at).getUTCDate() !== new Date(t).getUTCDate();
    // A visit arriving with new campaign parameters starts a new session, as in most analytics tools.
    var u = utmOf(location.href);
    if (!expired && u && JSON.stringify(u) !== JSON.stringify(s.utm)) expired = true;
    if (expired) {
      s = newSession(t);
      send("web_session", context(s));
    }
    s.last = t;
    set("_inc_s", s, 1);
    return s;
  }

  // ---- context shared by every event ----
  var view = null; // the current page view: {id, started, engaged, visibleSince, maxScroll}
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
      page: { url: location.href, path: location.pathname, title: d.title },
      device: {
        screen: { width: scr.width, height: scr.height, pixel_ratio: w.devicePixelRatio || 1 },
        viewport: { width: w.innerWidth, height: w.innerHeight },
        touch: "ontouchstart" in w || navigator.maxTouchPoints > 0
      },
      browser: { language: navigator.language },
      ip_address: "${incisive.ip}",
      user_agent: { raw: "${incisive.user_agent}" },
      tracker: { name: "incisive.js", version: VERSION }
    };
    try { ctx.browser.timezone = Intl.DateTimeFormat().resolvedOptions().timeZone; } catch (e) {}
    if (view) ctx.page.view_id = view.id;
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
  // Resolves functions in a properties object for one event.
  function resolve(v, info) {
    if (typeof v === "function") {
      try { return resolve(v(info), info); } catch (e) { warn("property function failed: " + e); return undefined; }
    }
    if (v && typeof v === "object" && !Array.isArray(v)) {
      var out = {};
      for (var k in v) if (Object.prototype.hasOwnProperty.call(v, k)) { var r = resolve(v[k], info); if (r !== undefined) out[k] = r; }
      return out;
    }
    return v;
  }

  function send(collection, ev) {
    if (!cfg) return warn("call incisive.init() first");
    for (var i = 0; i < globals.length; i++) {
      var extra = resolve(globals[i], { collection: collection, event: ev });
      if (extra && typeof extra === "object") merge(ev, extra);
    }
    var url = cfg.host + "/1.0/projects/" + encodeURIComponent(cfg.project) + "/events/" + encodeURIComponent(collection) + "?api_key=" + encodeURIComponent(cfg.key);
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

  function performanceTimings() {
    try {
      var n = performance.getEntriesByType("navigation")[0];
      if (!n) return null;
      var p = { ttfb_ms: Math.round(n.responseStart), dom_interactive_ms: Math.round(n.domInteractive) };
      if (n.loadEventEnd > 0) p.load_ms = Math.round(n.loadEventEnd);
      return p;
    } catch (e) { return null; }
  }

  // ---- page views and engagement ----
  var lastPath = null, sentPerf = false, auto = {};

  function scrollPercent() {
    var el = d.documentElement, b = d.body || {};
    var h = Math.max(el.scrollHeight || 0, b.scrollHeight || 0);
    var seen = (w.scrollY || el.scrollTop || 0) + w.innerHeight;
    return h > 0 ? Math.min(100, Math.round((seen / h) * 100)) : 100;
  }
  function engagedMs() {
    if (!view) return 0;
    return view.engaged + (view.visibleSince ? Date.now() - view.visibleSince : 0);
  }
  // Sends web_engagement for the current view, once.
  function endView() {
    if (!view || view.ended || !auto.engagement) return;
    view.ended = true;
    var t = Date.now();
    var ev = context(session(t));
    ev.engagement = {
      engaged_seconds: Math.round(engagedMs() / 1000),
      total_seconds: Math.round((t - view.started) / 1000),
      max_scroll_percent: Math.max(view.maxScroll, scrollPercent())
    };
    send("web_engagement", ev);
  }

  function pageview(extra) {
    if (!cfg) return warn("call incisive.init() first");
    endView();
    var t = Date.now();
    var s = session(t);
    view = { id: uid(), started: t, engaged: 0, visibleSince: d.visibilityState === "visible" ? t : 0, maxScroll: 0 };
    s.pageviews += 1;
    set("_inc_s", s, 1);
    var ev = context(s);
    ev.session.pageview_index = s.pageviews;
    ev.session.elapsed_seconds = Math.round((t - new Date(s.started_at).getTime()) / 1000);
    if (d.referrer || lastPath) ev.page.referrer = lastPath ? location.origin + lastPath : d.referrer;
    var c = navigator.connection;
    if (c && c.effectiveType) ev.connection = { effective_type: c.effectiveType };
    if (!sentPerf) { var perf = performanceTimings(); if (perf) { ev.performance = perf; sentPerf = true; } }
    lastPath = location.pathname + location.search;
    send("web_pageview", extra ? merge(ev, extra) : ev);
  }

  function track(collection, props) {
    if (!cfg) return warn("call incisive.init() first");
    send(collection, merge(context(session(Date.now())), props || {}));
  }

  function identify(userId, traits) {
    identity = { user_id: userId == null ? null : String(userId), traits: traits || identity.traits };
    set("_inc_u", identity, 730);
  }

  // ---- auto-tracking ----
  function ignored(el) {
    for (; el && el.nodeType === 1; el = el.parentElement) if (el.hasAttribute("data-incisive-ignore")) return true;
    return false;
  }
  function text(el) {
    return ((el.innerText || el.textContent || el.value || el.getAttribute("aria-label") || el.getAttribute("title") || "") + "").replace(/\s+/g, " ").trim().slice(0, 100);
  }
  function selector(el) {
    var parts = [];
    for (var i = 0; el && el.nodeType === 1 && i < 5; el = el.parentElement, i++) {
      var p = el.tagName.toLowerCase();
      if (el.id) { parts.unshift(p + "#" + el.id); break; }
      var cls = (typeof el.className === "string" ? el.className : "").trim().split(/\s+/).filter(Boolean).slice(0, 2);
      if (cls.length) p += "." + cls.join(".");
      parts.unshift(p);
    }
    return parts.join(" > ");
  }
  function describe(el) {
    var out = { tag: el.tagName.toLowerCase(), text: text(el), selector: selector(el) };
    if (el.id) out.id = el.id;
    var cls = typeof el.className === "string" ? el.className.trim() : "";
    if (cls) out.classes = cls.split(/\s+/).slice(0, 10);
    var name = el.getAttribute("data-track") || el.getAttribute("data-track-view");
    if (name) out.track_name = name;
    return out;
  }
  var DOWNLOAD = /\.(pdf|zip|dmg|exe|msi|pkg|csv|xlsx?|docx?|pptx?|mp3|mp4|mov|gz|tgz|rar|7z|apk|epub)$/i;

  function onClick(e) {
    var el = e.target && e.target.closest ? e.target.closest("a,button,[role=button],input[type=submit],input[type=button],[data-track]") : null;
    if (!el || ignored(el)) return;
    var ev = { element: describe(el) };
    var href = el.tagName === "A" && el.getAttribute("href");
    if (href && !/^(javascript:|#)/i.test(href)) {
      var u;
      try { u = new URL(href, location.href); } catch (x) { u = null; }
      if (u) {
        ev.link = {
          url: u.href, hostname: u.hostname, path: u.pathname,
          is_outbound: u.hostname !== location.hostname && /^https?:$/.test(u.protocol),
          is_download: el.hasAttribute("download") || DOWNLOAD.test(u.pathname),
          is_mailto: u.protocol === "mailto:", is_tel: u.protocol === "tel:"
        };
      }
    }
    track("web_click", ev);
  }

  function onSubmit(e) {
    var f = e.target;
    if (!f || f.tagName !== "FORM" || ignored(f)) return;
    var form = { selector: selector(f), method: (f.getAttribute("method") || "get").toLowerCase(), field_count: f.elements ? f.elements.length : 0 };
    if (f.id) form.id = f.id;
    if (f.getAttribute("name")) form.name = f.getAttribute("name");
    if (f.getAttribute("action")) { try { form.action = new URL(f.getAttribute("action"), location.href).href; } catch (x) {} }
    if (f.getAttribute("data-track")) form.track_name = f.getAttribute("data-track");
    track("web_form_submit", { form: form });
  }

  var observer = null, seenViews = typeof WeakSet === "function" ? new WeakSet() : null;
  function observeViews() {
    if (!observer || !seenViews) return;
    var els = d.querySelectorAll("[data-track-view]");
    for (var i = 0; i < els.length; i++) if (!seenViews.has(els[i])) { seenViews.add(els[i]); observer.observe(els[i]); }
  }

  function onNavigate() {
    var p = location.pathname + location.search;
    if (p !== lastPath) { pageview(); setTimeout(observeViews, 500); }
  }

  function extend(props) {
    if (props && (typeof props === "object" || typeof props === "function")) globals.push(props);
  }

  var started = false;
  // Auto-tracks the page. Options: the AUTO flags, `properties` joined into every event, and
  // optionally project/key/host so a single call sets everything up.
  function trackAll(opts) {
    if (opts === true || opts == null) opts = {};
    if (!cfg && opts.project) init(opts);
    if (!cfg) return warn("call incisive.init() first, or pass project, key and host");
    if (opts.properties) extend(opts.properties);
    if (opts.extend) extend(opts.extend);
    if (started) return;
    started = true;
    AUTO.forEach(function (k) { auto[k] = k in opts ? !!opts[k] : AUTO_DEFAULT[k]; });
    // trackAll always records the page load.
    auto.pageviews = true;

    if (auto.engagement) {
      d.addEventListener("visibilitychange", function () {
        if (!view) return;
        if (d.visibilityState === "hidden") {
          if (view.visibleSince) { view.engaged += Date.now() - view.visibleSince; view.visibleSince = 0; }
          endView();
        } else if (!view.ended && !view.visibleSince) {
          view.visibleSince = Date.now();
        }
      });
      w.addEventListener("pagehide", endView);
      w.addEventListener("scroll", function () { if (view) view.maxScroll = Math.max(view.maxScroll, scrollPercent()); }, { passive: true });
    }
    if (auto.clicks) d.addEventListener("click", onClick, true);
    if (auto.forms) d.addEventListener("submit", onSubmit, true);
    if (auto.views && w.IntersectionObserver) {
      observer = new IntersectionObserver(function (entries) {
        entries.forEach(function (en) {
          if (!en.isIntersecting || ignored(en.target)) return;
          observer.unobserve(en.target);
          track("web_element_view", { element: describe(en.target) });
        });
      }, { threshold: 0.5 });
    }
    if (auto.errors) {
      w.addEventListener("error", function (e) {
        if (!e.message) return;
        track("web_error", { error: { message: String(e.message).slice(0, 500), source: e.filename, line: e.lineno, column: e.colno } });
      });
    }
    if (auto.pageviews && cfg.spa !== false && w.history && history.pushState) {
      ["pushState", "replaceState"].forEach(function (m) {
        var orig = history[m];
        history[m] = function () { var r = orig.apply(this, arguments); setTimeout(onNavigate, 0); return r; };
      });
      w.addEventListener("popstate", onNavigate);
    }

    var loaded = false;
    function start() {
      if (loaded) return;
      loaded = true;
      pageview();
      observeViews();
    }
    // If the visitor leaves before the page finished parsing, still record the page load.
    w.addEventListener("pagehide", start);
    function whenVisible() {
      if (d.visibilityState === "prerender") {
        d.addEventListener("visibilitychange", function once() {
          if (d.visibilityState !== "prerender") { d.removeEventListener("visibilitychange", once); start(); }
        });
      } else {
        start();
      }
    }
    // Start as soon as the document is parsed; waiting for "load" can miss it entirely when the
    // script is injected late.
    if (d.readyState === "loading") d.addEventListener("DOMContentLoaded", whenVisible); else whenVisible();
  }

  function init(o) {
    o = o || {};
    if (!o.project || !o.key || !o.host) return warn("init needs project, key and host");
    if (o.respectDnt && (navigator.doNotTrack === "1" || w.doNotTrack === "1")) return;
    cfg = { project: o.project, key: o.key, host: String(o.host).replace(/\/$/, ""), cookieDomain: o.cookieDomain, spa: o.spa };
    loadVisitor();
  }

  // ---- public API, replaying calls queued before load ----
  var api = { init: init, trackAll: trackAll, autoTrack: trackAll, extend: extend, track: track, identify: identify, pageview: pageview, version: VERSION };
  var queued = (w.incisive && w.incisive.q) || [];
  w.incisive = api;

  // Configure from the script tag when it carries the project and key.
  var script = d.currentScript || d.querySelector("script[data-project][src*='incisive']");
  var attr = function (n) { return script ? script.getAttribute("data-" + n) : null; };
  if (attr("project") && attr("key")) {
    var host = attr("host");
    if (!host) {
      // Without data-host, assume the script is served by the API server itself.
      try { host = new URL(script.src, location.href).origin; } catch (e) {}
    }
    init({ project: attr("project"), key: attr("key"), host: host, cookieDomain: attr("cookie-domain"),
      respectDnt: attr("respect-dnt") === "true", spa: attr("spa") !== "false" });
  }
  queued.forEach(function (c) { if (api[c[0]]) api[c[0]].apply(null, Array.prototype.slice.call(c, 1)); });
  var at = attr("auto-track");
  if (cfg && at !== "false" && !started && attr("project")) {
    if (at && at !== "true") {
      var picked = {};
      AUTO.forEach(function (k) { picked[k] = false; });
      at.split(",").forEach(function (k) { picked[k.trim()] = true; });
      trackAll(picked);
    } else {
      trackAll({});
    }
  }
})(window, document);
