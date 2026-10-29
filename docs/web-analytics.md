# Web analytics

The browser tracker is `src/web/tracker.js`, and your Incisive server serves it at
`/js/incisive.js`. To serve it from a CDN or static bucket instead, upload it with
`scripts/publish_tracker.sh` (as `js/incisive.js`, cached 5 minutes, and the immutable
`js/incisive-VERSION.js`) and set `server.tracker_url` so the Install page points there.

```html
<script defer src="https://YOUR_INCISIVE_HOST/js/incisive.js"
  data-project="PROJECT_ID" data-key="WRITE_KEY" data-host="https://YOUR_INCISIVE_HOST"></script>
```

Or start it with one call:

```js
incisive.trackAll({
  project: "PROJECT_ID", key: "WRITE_KEY", host: "https://YOUR_INCISIVE_HOST",
  clicks: true, forms: true, engagement: true, views: true, errors: false,
  // Optional: properties added to every event. These are placeholders; a function is called on each event.
  // properties: { app: { version: "2.3.1" }, cart: { items: () => window.cart.length } },
});
```

`properties` are joined into every event; functions are called per event with
`{collection, event}`. The page load is always recorded once tracking starts, even if the visitor
leaves before the page finishes loading. In script-tag mode, `data-auto-track` takes `false` or a
comma list of the flags.

It keeps a first-party visitor ID, starts a session after 30 minutes of inactivity, at midnight
UTC, or on arrival with new campaign parameters, and sends, over `sendBeacon` (falling back to
`fetch` and then an image):

| Collection | When |
|---|---|
| `web_session` | once per session |
| `web_pageview` | every page, including single-page-app navigations |
| `web_click` | clicks on links, buttons and `data-track` elements, with outbound and download detection |
| `web_form_submit` | form submissions (never field values) |
| `web_engagement` | when a page view ends: engaged seconds, total seconds, max scroll depth |
| `web_element_view` | `data-track-view` elements scrolling into view |
| `web_error` | uncaught errors (off by default) |

All use the standard schema, which is deliberately denormalized: every event repeats its
session's landing page, external referrer, UTM parameters, ad click ID and derived channel, plus
the visitor's identity, device, browser, language and timezone, so any question is one query on
one collection. The server fills `ip_address` and `user_agent.raw` when absent and derives URL
parts, referrer source and medium, the default channel grouping (Direct, Organic Search, Paid
Search, Organic Social, Paid Social, Email, Referral, Affiliates, Display), browser, OS, device
type, bot detection, geo (with a database) and local time. Client values always win over derived
ones. The full field list is on the Schema page of the app and in `src/standard.rs`.

`incisive.track(collection, props)` sends custom events with the same context;
`incisive.identify(userId, traits)` adds `visitor.user_id` and `visitor.traits` from then on;
`incisive.extend(props)` adds properties to every later event.
