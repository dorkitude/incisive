import { test, describe, before, after, beforeEach } from 'node:test';
import assert from 'node:assert/strict';

import IncisiveAnalysis, { AnalysisError, Query } from 'incisive-analysis';
import { startMockServer } from './helpers/mock-server.js';

let server;

function client(extra = {}) {
  return new IncisiveAnalysis({ projectId: 'PROJECT', readKey: 'READ_KEY', baseUrl: server.url, ...extra });
}

before(async () => {
  server = await startMockServer();
});
after(() => server.close());
beforeEach(() => server.reset());

describe('standard analyses', () => {
  test('query(type, params) POSTs JSON with analysis_type and the read key', async () => {
    server.reply({ status: 200, body: { result: 42 } });
    const res = await client().query('count', {
      eventCollection: 'pageviews',
      timeframe: 'this_7_days',
      timezone: 'UTC',
      filters: [{ propertyName: 'user.age', operator: 'gt', propertyValue: 21 }],
      groupBy: ['country'],
    });
    const req = server.requests[0];
    assert.equal(req.method, 'POST');
    assert.equal(req.path, '/3.0/projects/PROJECT/queries/count');
    assert.equal(req.headers.authorization, 'READ_KEY');
    assert.equal(req.headers['content-type'], 'application/json');
    assert.equal(req.rawQuery, '');
    assert.deepEqual(req.body, {
      analysis_type: 'count',
      event_collection: 'pageviews',
      timeframe: 'this_7_days',
      timezone: 'UTC',
      filters: [{ property_name: 'user.age', operator: 'gt', property_value: 21 }],
      group_by: ['country'],
    });
    assert.equal(res.result, 42);
    // The request is echoed as `query` when the server does not include one.
    assert.equal(res.query.analysis_type, 'count');
    assert.equal(res.query.event_collection, 'pageviews');
  });

  test('single config object form, with the local timezone offset in seconds by default', async () => {
    server.reply({ status: 200, body: { result: 1, query: { analysis_type: 'sum', from: 'server' } } });
    const res = await client().query({
      analysisType: 'sum', eventCollection: 'purchases', targetProperty: 'price', timeframe: 'this_1_days',
    });
    const body = server.requests[0].body;
    assert.equal(server.requests[0].path, '/3.0/projects/PROJECT/queries/sum');
    assert.equal(body.target_property, 'price');
    assert.equal(body.timezone, new Date().getTimezoneOffset() * -60 || 0);
    assert.equal(typeof body.timezone, 'number');
    assert.deepEqual(res.query, { analysis_type: 'sum', from: 'server' });
  });

  test('every analysis type routes to /queries/{type}', async () => {
    const types = ['count', 'count_unique', 'sum', 'average', 'minimum', 'maximum', 'median', 'percentile',
      'select_unique', 'standard_deviation', 'extraction', 'funnel', 'multi_analysis'];
    server.setFallback({ status: 200, body: { result: 0 } });
    const c = client();
    for (const type of types) await c.query(type, { event_collection: 'e', timezone: 'UTC' });
    assert.deepEqual(server.requests.map((r) => r.path), types.map((t) => `/3.0/projects/PROJECT/queries/${t}`));
    assert.deepEqual(server.requests.map((r) => r.body.analysis_type), types);
  });

  test('falls back to the master key when no read key is set', async () => {
    server.reply({ status: 200, body: { result: 0 } });
    await new IncisiveAnalysis({ projectId: 'PROJECT', masterKey: 'MASTER', baseUrl: server.url })
      .query('count', { event_collection: 'e' });
    assert.equal(server.requests[0].headers.authorization, 'MASTER');
  });

  test('funnel and multi_analysis bodies pass through', async () => {
    server.reply({ status: 200, body: { result: [3, 1], steps: [{ event_collection: 'a' }, { event_collection: 'b' }] } });
    const res = await client().query('funnel', {
      steps: [
        { eventCollection: 'a', actorProperty: 'user.id', timeframe: 'this_7_days' },
        { eventCollection: 'b', actorProperty: 'user.id', timeframe: 'this_7_days', optional: true },
      ],
    });
    assert.deepEqual(server.requests[0].body.steps[1], {
      event_collection: 'b', actor_property: 'user.id', timeframe: 'this_7_days', optional: true,
    });
    assert.deepEqual(res.result, [3, 1]);
    assert.equal(res.query, undefined); // no event_collection at top level, nothing echoed

    server.reply({ status: 200, body: { result: { total: '10', uniques: 4 } } });
    const multi = await client().query('multi_analysis', {
      event_collection: 'e',
      analyses: { total: { analysis_type: 'count' }, uniques: { analysis_type: 'count_unique', target_property: 'u' } },
    });
    assert.deepEqual(server.requests[1].body.analyses.uniques, { analysis_type: 'count_unique', target_property: 'u' });
    assert.deepEqual(multi.result, { total: '10', uniques: 4 });
  });

  test('include_metadata from client config', async () => {
    server.reply({ status: 200, body: { result: 1, execution_metadata: { total_processing_time: 0.1 } } });
    const res = await client({ includeMetadata: true }).query('count', { event_collection: 'e' });
    assert.equal(server.requests[0].body.include_metadata, true);
    assert.equal(res.execution_metadata.total_processing_time, 0.1);
  });

  test('wrong arguments reject with SDKError', async () => {
    await assert.rejects(client().query('count'), (err) => err.error_code === 'SDKError');
  });

  test('missing credentials throw synchronously', () => {
    assert.throws(() => new IncisiveAnalysis({ readKey: 'R' }).query('count', {}), /project ID/);
    assert.throws(() => new IncisiveAnalysis({ projectId: 'P' }).query('count', {}), /API key/);
  });
});

describe('result handling', () => {
  test('numeric strings become numbers for simple, grouped and interval results', async () => {
    const c = client();
    server.reply({ status: 200, body: { result: '12.5' } });
    assert.equal((await c.query('average', { event_collection: 'e' })).result, 12.5);

    server.reply({ status: 200, body: { result: [{ country: 'US', result: '3' }, { country: 'FR', result: null }] } });
    const grouped = await c.query('count', { event_collection: 'e', group_by: 'country' });
    assert.deepEqual(grouped.result, [{ country: 'US', result: 3 }, { country: 'FR', result: null }]);

    server.reply({ status: 200, body: { result: [{ timeframe: { start: 's', end: 'e' }, value: '7' }] } });
    const interval = await c.query('count', { event_collection: 'e', interval: 'daily' });
    assert.equal(interval.result[0].value, 7);

    server.reply({ status: 200, body: { result: [{ timeframe: { start: 's', end: 'e' }, value: [{ c: 'US', result: '2' }] }] } });
    const both = await c.query('count', { event_collection: 'e', interval: 'daily', group_by: 'c' });
    assert.equal(both.result[0].value[0].result, 2);

    server.reply({ status: 200, body: { result: ['1', '2'] } });
    const unique = await c.query('select_unique', { event_collection: 'e', target_property: 'x' });
    assert.deepEqual(unique.result, ['1', '2']);
  });

  test('resultParsers run over every value', async () => {
    server.reply({ status: 200, body: { result: [{ g: 'a', result: 1.26 }, { g: 'b', result: 2.71 }] } });
    const c = client({ resultParsers: [(v) => Math.round(v), (v) => v * 10] });
    const res = await c.query('average', { event_collection: 'e', group_by: 'g' });
    assert.deepEqual(res.result.map((r) => r.result), [10, 30]);
  });

  test('error envelopes reject with AnalysisError fields', async () => {
    server.reply({ status: 400, body: { message: 'Bad timeframe', error_code: 'TimeframeDefinitionError' } });
    await assert.rejects(client().query('count', { event_collection: 'e' }), (err) => {
      assert.ok(err instanceof AnalysisError);
      assert.equal(err.ok, false);
      assert.equal(err.status, 400);
      assert.equal(err.error_code, 'TimeframeDefinitionError');
      assert.equal(err.body, 'Bad timeframe');
      assert.equal(err.message, 'Bad timeframe');
      return true;
    });
  });

  test('an error_code in a 200 body is still an error', async () => {
    server.reply({ status: 200, body: { message: 'nope', error_code: 'QueryError' } });
    await assert.rejects(client().query('count', { event_collection: 'e' }), (err) => err.error_code === 'QueryError');
  });

  test('non-JSON error bodies are surfaced as the message', async () => {
    server.reply({ status: 502, raw: 'Bad Gateway' });
    await assert.rejects(client().query('count', { event_collection: 'e' }), (err) => err.status === 502 && err.body === 'Bad Gateway');
  });

  test('client cache serves repeated queries without a request', async () => {
    server.setFallback({ status: 200, body: { result: 5 } });
    const c = client({ cache: { maxAge: 60000 } });
    await c.query('count', { event_collection: 'e', timezone: 'UTC' });
    const second = await c.query('count', { event_collection: 'e', timezone: 'UTC' });
    assert.equal(second.result, 5);
    assert.equal(server.requests.length, 1);
    await c.query({ analysis_type: 'count', event_collection: 'e', timezone: 'UTC', cache: false });
    assert.equal(server.requests.length, 2);
  });

  test('abort() cancels an in-flight request', async () => {
    server.reply(() => ({ status: 200, body: { result: 1 } }));
    const c = client();
    const p = c.query('count', { event_collection: 'e' });
    p.abort();
    await assert.rejects(p, (err) => err.name === 'AbortError');
  });
});

describe('saved queries and cached datasets', () => {
  test('saved_query_name reads /queries/saved/{name}/result with api_key and header', async () => {
    server.reply({ status: 200, body: { result: 9, query_name: 'daily' } });
    const res = await client().query({ savedQueryName: 'daily' });
    const req = server.requests[0];
    assert.equal(req.method, 'GET');
    assert.equal(req.path, '/3.0/projects/PROJECT/queries/saved/daily/result');
    assert.equal(req.query.api_key, 'READ_KEY');
    assert.equal(req.headers.authorization, 'READ_KEY');
    assert.equal(res.result, 9);
  });

  test("legacy query('saved', name) form does not double /result", async () => {
    server.setFallback({ status: 200, body: { result: 1 } });
    await client().query('saved', 'daily');
    await client().query('saved', 'daily/result');
    assert.equal(server.requests[0].path, '/3.0/projects/PROJECT/queries/saved/daily/result');
    assert.equal(server.requests[1].path, '/3.0/projects/PROJECT/queries/saved/daily/result');
  });

  test('run() accepts names, Query objects and arrays', async () => {
    server.setFallback({ status: 200, body: { result: 2 } });
    const q = new Query('count', { eventCollection: 'e', timezone: 'UTC' }).addFilter('a', 'eq', 1);
    assert.equal(q.get('eventCollection'), 'e');
    const [a, b] = await client().run([q, 'saved-one']);
    assert.equal(a.result, 2);
    assert.equal(b.result, 2);
    const paths = server.requests.map((r) => r.path).sort();
    assert.deepEqual(paths, ['/3.0/projects/PROJECT/queries/count', '/3.0/projects/PROJECT/queries/saved/saved-one/result']);
    const countReq = server.requests.find((r) => r.method === 'POST');
    assert.deepEqual(countReq.body.filters, [{ property_name: 'a', operator: 'eq', property_value: 1 }]);
  });

  test('dataset_name reads /datasets/{name}/results with serialized params', async () => {
    server.reply({ status: 200, body: { result: [{ value: 1 }] } });
    await client().query({ datasetName: 'dash', indexBy: 'customer.id', timeframe: { start: '2026-01-01T00:00:00Z', end: '2026-01-02T00:00:00Z' } });
    const req = server.requests[0];
    assert.equal(req.method, 'GET');
    assert.equal(req.path, '/3.0/projects/PROJECT/datasets/dash/results');
    assert.equal(req.query.api_key, 'READ_KEY');
    assert.equal(req.query.index_by, 'customer.id');
    assert.equal(req.query.dataset_name, 'dash');
    assert.deepEqual(JSON.parse(req.query.timeframe), { start: '2026-01-01T00:00:00Z', end: '2026-01-02T00:00:00Z' });
  });

  test("legacy query('dataset', {name}) form", async () => {
    server.reply({ status: 200, body: { result: [] } });
    await client().query('dataset', { name: 'dash', indexBy: 'x', timeframe: 'this_3_days' });
    assert.equal(server.requests[0].path, '/3.0/projects/PROJECT/datasets/dash/results');
    assert.equal(server.requests[0].query.timeframe, 'this_3_days');
  });
});

describe('raw requests for management endpoints', () => {
  test('get() lists saved queries with the master key', async () => {
    server.reply({ status: 200, body: [{ query_name: 'a' }] });
    const c = client({ masterKey: 'MASTER' });
    const res = await c.get({ url: c.url('queries', 'saved'), api_key: c.masterKey() });
    const req = server.requests[0];
    assert.equal(req.path, '/3.0/projects/PROJECT/queries/saved');
    assert.equal(req.rawQuery, 'api_key=MASTER');
    assert.equal(req.headers.authorization, 'MASTER');
    assert.deepEqual(res, [{ query_name: 'a' }]);
  });

  test('put() creates a saved query with a JSON body', async () => {
    server.reply({ status: 201, body: { query_name: 'daily', refresh_rate: 0 } });
    const c = client({ masterKey: 'MASTER' });
    await c.put({
      url: c.url('queries', 'saved', 'daily'),
      api_key: c.masterKey(),
      params: { refreshRate: 0, query: { analysisType: 'count', eventCollection: 'e', timeframe: 'this_14_days' } },
    });
    const req = server.requests[0];
    assert.equal(req.method, 'PUT');
    assert.equal(req.path, '/3.0/projects/PROJECT/queries/saved/daily');
    assert.deepEqual(req.body, { refresh_rate: 0, query: { analysis_type: 'count', event_collection: 'e', timeframe: 'this_14_days' } });
  });

  test('del() puts filters/timeframe/timezone in the query string as JSON and resolves {} on 204', async () => {
    server.reply({ status: 204 });
    const c = client({ masterKey: 'MASTER' });
    const res = await c.del({
      url: c.url('events', 'purchases'),
      api_key: c.masterKey(),
      filters: [{ propertyName: 'item.id', operator: 'eq', propertyValue: 7 }],
      timeframe: 'this_7_days',
      timezone: 'UTC',
    });
    assert.deepEqual(res, {});
    const req = server.requests[0];
    assert.equal(req.method, 'DELETE');
    assert.equal(req.path, '/3.0/projects/PROJECT/events/purchases');
    assert.deepEqual(JSON.parse(req.query.filters), [{ property_name: 'item.id', operator: 'eq', property_value: 7 }]);
    assert.equal(req.query.timeframe, '"this_7_days"');
    assert.equal(req.query.timezone, '"UTC"');
    assert.equal(req.query.api_key, 'MASTER');
    assert.equal(req.headers.authorization, 'MASTER');
  });

  test('post() to an analysis URL adds analysis_type', async () => {
    server.reply({ status: 200, body: { result: 1 } });
    const c = client();
    await c.post({ url: c.url('queries', 'count'), api_key: c.readKey(), params: { event_collection: 'e' } });
    assert.equal(server.requests[0].body.analysis_type, 'count');
  });

  test('requests without a key throw', () => {
    assert.throws(() => client().get({ url: 'http://x' }), /valid API key/);
  });
});
