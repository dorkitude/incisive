# incisive JavaScript SDKs

Dependency-free ES modules for browsers and Node.js 18+ (anything with `fetch`).

| Package | What it does |
|---|---|
| `incisive-tracking` | Record events: fetch, Beacon API and image-pixel transports, retries, batching/deferred queue, event extensions, opt-out / Do Not Track, unique-event guard, browser auto-tracking (page views, clicks, form submissions, element views). |
| `incisive-analysis` | Run analyses (`count`, `count_unique`, `sum`, `average`, `minimum`, `maximum`, `median`, `percentile`, `select_unique`, `standard_deviation`, `extraction`, `funnel`, `multi_analysis`), saved/cached query results, cached dataset results, and raw `get/post/put/del` for management endpoints (saved query CRUD, collection discovery, deletes, access keys). |
| `incisive-dataset` | Turn analysis responses into chart-ready tables, picking the layout (metric, interval, grouped, double-grouped, funnel, list, extraction) from the response. |
| `incisive-core` | Shared configuration, URL building and event emitter. |

## Configuration

Every client takes the same connection options:

```js
const config = {
  projectId: 'PROJECT_ID',
  writeKey: 'WRITE_KEY',      // tracking
  readKey: 'READ_KEY',        // analysis
  masterKey: 'MASTER_KEY',    // deletes, saved query writes, access keys

  baseUrl: 'https://analytics.example.com', // default http://localhost:8080
  // or, equivalently: host: 'analytics.example.com', protocol: 'https'
  version: '1.0',                           // API version path segment
};
```

`client.url(name, ...segments, {query})` builds URLs from named resources (`base`, `version`,
`projects`, `projectId`, `events`, `queries`, `datasets`, `keys`); `client.resources({...})`
overrides any template, e.g. to route ingestion through a different host.

## Tracking

```js
import IncisiveTracking from 'incisive-tracking';

const tracker = new IncisiveTracking({ projectId, writeKey, baseUrl });

await tracker.recordEvent('purchases', { item: 'golden ticket', price: 4.5 });
tracker.recordEvent({ collection: 'signups', event: { plan: 'pro' }, unique: true });
await tracker.recordEvents({ purchases: [{ price: 1 }], signups: [{ plan: 'free' }] });

tracker.extendEvents({ app: { version: '1.2.0' } });        // merged into every event
tracker.extendEvent('purchases', () => ({ cart_id: cartId })); // per collection, evaluated per event

tracker.deferEvent('clicks', { x: 1 });   // queued; flushed as one batch
tracker.queueCapacity(100);               // flush at 100 queued events
tracker.queueInterval(10);                // or every 10 seconds
tracker.recordDeferredEvents();           // flush now
```

Every call returns a promise and also accepts a node-style `callback(err, res)`.

Transports (`requestType` on the client or per `recordEvent({...})` call):

- `fetch` (default): `POST /1.0/projects/{id}/events/{collection}` with `Authorization: <writeKey>`
  and a JSON body. Retries network errors and 408/500/502/503/504 with exponential backoff
  (`retry: { limit: 10, initialDelay: 200, retryOnResponseStatuses: [...] }`).
- `beaconAPI`: `navigator.sendBeacon(url + '?api_key=' + writeKey, JSON)`; survives page unloads,
  fire-and-forget. Falls back to fetch where unavailable.
- `img` (legacy): `GET .../events/{collection}?api_key=...&data=<base64 JSON>&modified=<ms>&c=clv1`
  via an image element.

Errors reject with a `TrackingError` carrying `status`, `error_code` and `body` (the server's
message). A batch where some events failed rejects with the per-event results on `err.response`.

Privacy: `IncisiveTracking.enabled = false`, `tracker.setOptOut(true)` (persisted in
`localStorage`), and `respectDoNotTrack: true`.

### Auto-tracking (browser)

```js
tracker.initAutoTracking({
  recordPageViews: true,
  recordClicks: true,              // links
  recordFormSubmits: true,         // password fields are never captured
  recordElementViews: true,        // elements with class "track-element-view"
  recordScrollState: true,
  collectIpAddress: true,
  catchError: (err) => {},         // switches the transport to fetch so errors are observable
  cookieName: 'incisive',          // first-party cookie holding the visitor uuid
});
```

Auto-tracked events include page, referrer, browser and visitor details, and instructions for the
server's enrichment add-ons (user agent, URL, referrer, date-time and IP-to-geo parsing) inside the
reserved metadata object. To keep visitor ids continuous when migrating from another tracker, pass
that tracker's cookie name as `cookieName`.

## Analysis

```js
import IncisiveAnalysis from 'incisive-analysis';

const client = new IncisiveAnalysis({ projectId, readKey, baseUrl });

const { result } = await client.query('count', {
  eventCollection: 'pageviews',       // camelCase keys are converted to snake_case
  timeframe: 'this_14_days',
  interval: 'daily',
  groupBy: ['country'],
  filters: [{ propertyName: 'user.age', operator: 'gte', propertyValue: 21 }],
});

await client.query({ analysisType: 'funnel', steps: [...] });
await client.query({ savedQueryName: 'daily-pageviews' });               // saved / cached query result
await client.query({ datasetName: 'per-customer', indexBy: 'customer.id', timeframe: 'this_7_days' });
await client.run([queryA, 'saved-query-name']);                         // Promise.all
```

Analyses `POST /1.0/projects/{id}/queries/{type}` with a JSON body that includes `analysis_type`
and, when not given, `timezone` as the local UTC offset in seconds. Saved query and dataset
results are `GET`s with `?api_key=`. The read key is used, or the master key when no read key is
set.

Response handling: numeric strings in results become numbers; `resultParsers: [fn, ...]` post-process
every value; if the server does not echo `query`, the request parameters are attached as `query`
(the dataset helper uses it). `includeMetadata: true` asks for execution metadata. `cache: { maxAge }`
enables an in-memory result cache. Each returned promise has `.abort()`, and `timeout` (ms) aborts
slow requests. Errors reject with an `AnalysisError` carrying `status`, `error_code`, `body`.

Management endpoints use the raw helpers:

```js
client.get({ url: client.url('queries', 'saved'), api_key: masterKey });
client.put({ url: client.url('queries', 'saved', 'daily'), api_key: masterKey,
             params: { refreshRate: 0, query: { analysisType: 'count', eventCollection: 'pageviews', timeframe: 'this_7_days' } } });
client.del({ url: client.url('queries', 'saved', 'daily'), api_key: masterKey });
client.get({ url: client.url('events'), api_key: masterKey });           // collections and schemas
client.del({ url: client.url('events', 'purchases'), api_key: masterKey,
             filters: [{ propertyName: 'test', operator: 'eq', propertyValue: true }], timeframe: 'this_7_days' });
client.post({ url: client.url('keys'), api_key: masterKey, params: { name: 'dashboard', permitted: ['queries'] } });
```

`GET`/`DELETE` send the key both as `Authorization` and `?api_key=`; other parameters are put in the
query string (strings as-is, everything else JSON). `DELETE` resolves `{}` on success.

## Charting results

```js
import { parseResponse } from 'incisive-dataset';

const res = await client.query('count', { eventCollection: 'pageviews', interval: 'daily', groupBy: 'os' });
const { dataset, parser, type, title } = parseResponse(res, { indexBy: 'timeframe.start' });
dataset.data(); // [['Index', 'mac', 'win'], [Date, 3, 5], ...]
// parser: 'grouped-interval', type: 'line', title: 'Count - pageviews'
```

## Tests

```sh
# unit tests (mock HTTP server, asserts the exact wire format)
docker run --rm -v "$PWD":/w -w /w node:22.11.0-bookworm-slim \
  bash -c 'npm install --silent && npm test'

# against a running server
INCISIVE_BASE_URL=http://localhost:8080 INCISIVE_PROJECT_ID=... \
INCISIVE_WRITE_KEY=... INCISIVE_READ_KEY=... INCISIVE_MASTER_KEY=... npm run test:integration
```
