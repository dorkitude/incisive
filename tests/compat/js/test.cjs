// Runs the unmodified, pinned upstream JavaScript packages (see package-lock.json) against a live
// server. Configuration comes from the environment; see ../run.sh.
const test = require('node:test');
const assert = require('node:assert/strict');

const env = (k) => {
  if (!process.env[k]) throw new Error(`${k} is required`);
  return process.env[k];
};
const projectId = env('COMPAT_PROJECT_ID');
const host = env('COMPAT_HOST');
const writeKey = env('COMPAT_WRITE_KEY');
const readKey = env('COMPAT_READ_KEY');
const masterKey = env('COMPAT_MASTER_KEY');

const Tracking = require('keen-tracking');
const Analysis = require('keen-analysis');
// keen-dataviz expects a browser; give it a DOM before loading it.
const { JSDOM } = require('jsdom');
const dom = new JSDOM('<!doctype html><html><body><div id="chart"></div></body></html>', { pretendToBeVisual: true });
globalThis.window = dom.window;
for (const k of Object.getOwnPropertyNames(dom.window)) {
  if (!(k in globalThis)) {
    try { globalThis[k] = dom.window[k]; } catch (_) { /* read-only */ }
  }
}
// jsdom has no SVG path API; c3 (a dataviz dependency) patches it at load time.
dom.window.SVGPathElement = dom.window.SVGPathElement || class SVGPathElement {};
const Dataviz = require('keen-dataviz');

const TrackingClient = Tracking.default || Tracking.KeenTracking || Tracking;
const AnalysisClient = Analysis.default || Analysis.KeenAnalysis || Analysis;
const DatavizClass = Dataviz.default || Dataviz.KeenDataviz || Dataviz;

const tracker = new TrackingClient({ projectId, writeKey, host, protocol: 'https' });
const analysis = new AnalysisClient({ projectId, readKey, masterKey, host, protocol: 'https' });

const run = Date.now().toString(36);
const coll = (name) => `${name}_${run}`;
const tf = { start: '2026-01-01T00:00:00.000Z', end: '2026-01-04T00:00:00.000Z' };

test('tracking: recordEvent resolves created', async () => {
  const res = await tracker.recordEvent(coll('purchases'), {
    user: 'u1', item: { price: 10, name: 'hat' }, keen: { timestamp: '2026-01-01T10:00:00.000Z' },
  });
  assert.equal(res.created, true);
});

test('tracking: recordEvents batch resolves per-event success', async () => {
  const res = await tracker.recordEvents({
    [coll('purchases')]: [
      { user: 'u2', item: { price: 20, name: 'shoe' }, keen: { timestamp: '2026-01-02T10:00:00.000Z' } },
      { user: 'u1', item: { price: 5, name: 'hat' }, keen: { timestamp: '2026-01-03T10:00:00.000Z' } },
    ],
    [coll('signups')]: [{ user: 'u1', keen: { timestamp: '2026-01-01T09:00:00.000Z' } },
      { user: 'u3', keen: { timestamp: '2026-01-01T09:00:00.000Z' } }],
  });
  assert.deepEqual(res[coll('purchases')], [{ success: true }, { success: true }]);
  assert.deepEqual(res[coll('signups')], [{ success: true }, { success: true }]);
});

test('tracking: invalid event rejects with error_code', async () => {
  await assert.rejects(tracker.recordEvent(coll('bad'), { 'a.b': 1 }), (err) => {
    assert.match(String(err.code || err.error_code || err.message), /InvalidPropertyName/);
    return true;
  });
});

test('tracking: wrong key rejects', async () => {
  const bad = new TrackingClient({ projectId, writeKey: 'nope', host, protocol: 'https' });
  await assert.rejects(bad.recordEvent(coll('x'), { a: 1 }), (err) => {
    assert.match(String(err.code || err.error_code || err.message), /InvalidApiKey/);
    return true;
  });
});

const q = (type, params) => analysis.query(type, { event_collection: coll('purchases'), timeframe: tf, timezone: 'UTC', ...params });

test('analysis: count / sum / average / percentile', async () => {
  assert.equal((await q('count', {})).result, 3);
  assert.equal((await q('sum', { target_property: 'item.price' })).result, 35);
  assert.equal((await q('average', { target_property: 'item.price' })).result, 35 / 3);
  assert.equal((await q('median', { target_property: 'item.price' })).result, 10);
  assert.equal((await q('percentile', { target_property: 'item.price', percentile: 50 })).result, 10);
  assert.equal((await q('count_unique', { target_property: 'user' })).result, 2);
  assert.deepEqual((await q('select_unique', { target_property: 'user' })).result, ['u1', 'u2']);
});

test('analysis: single-object query form and default local timezone', async () => {
  const res = await analysis.query({ analysis_type: 'count', event_collection: coll('purchases'), timeframe: tf });
  assert.equal(res.result, 3);
  assert.equal(res.query.analysis_type, 'count');
});

test('analysis: filters, group_by, interval', async () => {
  const f = await q('count', { filters: [{ property_name: 'item.name', operator: 'eq', property_value: 'hat' }] });
  assert.equal(f.result, 2);
  const g = await q('sum', { target_property: 'item.price', group_by: 'item.name' });
  assert.deepEqual(g.result, [{ 'item.name': 'hat', result: 15 }, { 'item.name': 'shoe', result: 20 }]);
  const i = await q('count', { interval: 'daily' });
  assert.deepEqual(i.result.map((x) => x.value), [1, 1, 1]);
  assert.equal(i.result[0].timeframe.start, '2026-01-01T00:00:00.000Z');
});

test('analysis: funnel, extraction, multi_analysis', async () => {
  const fn = await analysis.query('funnel', { timeframe: tf, steps: [
    { event_collection: coll('signups'), actor_property: 'user' },
    { event_collection: coll('purchases'), actor_property: 'user' }] });
  assert.deepEqual(fn.result, [2, 1]);
  assert.equal(fn.steps.length, 2);
  const ex = await q('extraction', { latest: 1, property_names: ['user'] });
  assert.deepEqual(ex.result, [{ user: 'u1' }]);
  const m = await q('multi_analysis', { analyses: { n: { analysis_type: 'count' }, total: { analysis_type: 'sum', target_property: 'item.price' } } });
  assert.deepEqual(m.result, { n: 3, total: 35 });
});

test('analysis: query errors reject with error_code', async () => {
  await assert.rejects(q('count', { timeframe: 'last_fortnight' }), (err) => {
    assert.match(String(err.error_code || err.code || err.message), /Timeframe/);
    return true;
  });
});

test('analysis: raw get/put/del for schema and saved queries', async () => {
  const schema = await analysis.get({ url: analysis.url('events', coll('purchases')), api_key: masterKey });
  assert.equal(schema.properties['item.price'], 'num');
  const name = `saved_${run}`;
  await analysis.put({ url: analysis.url('queries', 'saved', name), api_key: masterKey,
    params: { query: { analysis_type: 'count', event_collection: coll('purchases'), timeframe: tf }, metadata: { display_name: 'Purchases' } } });
  const saved = await analysis.query({ saved_query_name: name });
  assert.equal(saved.result, 3);
  await analysis.del({ url: analysis.url('queries', 'saved', name), api_key: masterKey });
});

test('dataviz: parses results into datasets', async () => {
  const parse = (res) => {
    const chart = new DatavizClass({ container: '#chart', type: 'table' });
    chart.data(res);
    return chart.dataset.matrix;
  };
  assert.deepEqual(parse(await q('count', {})), [['Index', 'Value'], ['Result', 3]]);
  const grouped = parse(await q('count', { group_by: 'item.name' }));
  assert.deepEqual(grouped.slice(1), [['hat', 2], ['shoe', 1]]);
  const interval = parse(await q('count', { interval: 'daily' }));
  assert.equal(interval.length, 4);
  assert.deepEqual(interval.slice(1).map((r) => r[1]), [1, 1, 1]);
  const gi = parse(await q('count', { interval: 'daily', group_by: 'item.name' }));
  assert.deepEqual(gi[0].slice(1).sort(), ['hat', 'shoe']);
  const funnel = parse(await analysis.query('funnel', { timeframe: tf, steps: [
    { event_collection: coll('signups'), actor_property: 'user' },
    { event_collection: coll('purchases'), actor_property: 'user' }] }));
  assert.deepEqual(funnel.slice(1).map((r) => r[1]), [2, 1]);
});

// Charting timers keep the event loop alive after the tests; exit once they are done.
test.after(() => { dom.window.close(); setTimeout(() => process.exit(process.exitCode || 0), 50).unref(); });
