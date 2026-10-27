// Loads the served tracker in jsdom for several simulated visits and checks what the server stored.
// Usage: node tracker_e2e.cjs <base_url> <project_id> <write_key> <read_key>
const { JSDOM } = require("jsdom");
const [base, project, key, readKey] = process.argv.slice(2);

const sent = [];
const pending = [];
async function visit({ url, referrer, ua, storage, navigate = [] }) {
  const html = `<!doctype html><html><head><title>Page</title>
    <script src="${base}/js/incisive.js" data-project="${project}" data-key="${key}"></script></head><body></body></html>`;
  const vc = new (require("jsdom").VirtualConsole)();
  vc.on("jsdomError", (e) => console.error("jsdom:", e.message));
  vc.on("warn", (m) => console.error("page warn:", m));
  vc.on("error", (m) => console.error("page error:", m));
  const dom = new JSDOM(html, {
    virtualConsole: vc,
    url, referrer: referrer || undefined, runScripts: "dangerously", resources: "usable", userAgent: ua, pretendToBeVisual: true,
    beforeParse(w) {
      // jsdom has no sendBeacon; forward to Node's fetch like a browser would.
      w.navigator.sendBeacon = (u, blob) => {
        sent.push(u);
        const fr = new w.FileReader();
        const text = new Promise((res) => { fr.onload = () => res(fr.result); fr.readAsText(blob); });
        pending.push(text.then((b) => fetch(u, { method: "POST", body: b, headers: { "content-type": "text/plain", "user-agent": ua, "x-forwarded-for": "198.51.100.7" } }).then((r) => {
          if (r.status !== 201) return r.text().then((t) => console.error("write failed", r.status, t));
        }).catch((e) => console.error("fetch failed", e))));
        return true;
      };
      for (const [k, v] of Object.entries(storage)) w.localStorage.setItem(k, v);
    },
  });
  if (dom.window.document.readyState !== "complete") await new Promise((res) => dom.window.addEventListener("load", res));
  if (process.env.DEBUG) console.log("loaded", url, "sent so far", sent.length, "api?", typeof dom.window.incisive);
  await new Promise((r) => setTimeout(r, 300));
  for (const path of navigate) {
    dom.window.history.pushState({}, "", path);
    await new Promise((r) => setTimeout(r, 100));
  }
  // Persist storage like a browser profile would.
  const out = {};
  for (let i = 0; i < dom.window.localStorage.length; i++) {
    const k = dom.window.localStorage.key(i);
    out[k] = dom.window.localStorage.getItem(k);
  }
  dom.window.close();
  return out;
}

async function q(analysis, params) {
  const u = new URL(`${base}/3.0/projects/${project}/queries/${analysis}`);
  for (const [k, v] of Object.entries(params)) u.searchParams.set(k, typeof v === "string" ? v : JSON.stringify(v));
  const r = await fetch(u, { headers: { authorization: readKey } });
  const j = await r.json();
  if (r.status !== 200) throw new Error(JSON.stringify(j));
  return j.result;
}

function assert(c, m) { if (!c) { console.error("FAIL:", m); process.exitCode = 1; } else console.log("ok:", m); }

(async () => {
  const chrome = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
  const iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
  // Visitor A: arrives from Google, browses two more pages in an SPA.
  const a = await visit({ url: "https://shop.example/", referrer: "https://www.google.com/", ua: chrome, storage: {}, navigate: ["/pricing", "/signup"] });
  // Visitor A returns in the same session.
  await visit({ url: "https://shop.example/docs", referrer: "https://shop.example/signup", ua: chrome, storage: a });
  // Visitor B: newsletter campaign on a phone, bounces.
  await visit({ url: "https://shop.example/pricing?utm_source=newsletter&utm_medium=email&utm_campaign=oct", referrer: "", ua: iphone, storage: {} });
  await Promise.all(pending);
  console.log("beacons sent:", sent.map((u) => u.split("/events/")[1].split("?")[0]));
  console.log("counts", await q("count", { event_collection: "web_pageview", timeframe: "this_1_days" }), await q("count", { event_collection: "web_session", timeframe: "this_1_days" }));
  const tf = "this_1_days";
  assert((await q("count", { event_collection: "web_pageview", timeframe: tf })) === 5, "5 pageviews");
  assert((await q("count", { event_collection: "web_session", timeframe: tf })) === 2, "2 sessions");
  assert((await q("count_unique", { event_collection: "web_pageview", target_property: "visitor.id", timeframe: tf })) === 2, "2 visitors");
  const ch = await q("count", { event_collection: "web_pageview", timeframe: tf, group_by: "session.channel" });
  const byCh = Object.fromEntries(ch.map((r) => [r["session.channel"], r.result]));
  assert(byCh["Organic Search"] === 4 && byCh["Email"] === 1, "channels denormalized onto pageviews " + JSON.stringify(byCh));
  const idx = await q("maximum", { event_collection: "web_pageview", target_property: "session.pageview_index", timeframe: tf });
  assert(idx === 4, "pageview_index reaches 4 in visitor A's session");
  const ex = await q("extraction", { event_collection: "web_pageview", timeframe: tf, latest: 10 });
  const phone = ex.find((e) => e.device && e.device.type === "mobile");
  assert(phone && phone.session.utm.campaign === "oct" && phone.os.name === "iOS", "phone pageview carries campaign and OS");
  assert(ex.every((e) => e.ip_address === "198.51.100.7"), "IP placeholder replaced with the client IP");
  assert(ex.every((e) => e.user_agent.raw && !e.user_agent.raw.includes("${")), "user agent placeholder replaced");
  assert(ex.some((e) => e.page.path === "/signup" && e.page.referrer === "https://shop.example/pricing"), "SPA navigation tracked with internal referrer");
  assert(ex.every((e) => e.browser.timezone && e.visitor.id && e.session.id), "timezone, visitor and session on every pageview");
  console.log("sample:", JSON.stringify(ex.find((e) => e.page.path === "/pricing" && e.session.channel === "Organic Search"), null, 1).slice(0, 2500));
})();
