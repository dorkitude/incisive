import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import Dataset, { parseResponse, detectParser, defaultTitle } from 'incisive-dataset';

const day = (d) => String(d).padStart(2, '0');
const tf = (d) => ({ start: `2026-10-${day(d)}T00:00:00.000Z`, end: `2026-10-${day(d + 1)}T00:00:00.000Z` });

describe('Dataset table operations', () => {
  test('set creates rows and columns, filling gaps with null', () => {
    const ds = new Dataset();
    ds.set(['A', 'r1'], 1).set(['B', 'r2'], 2).set(['A', 'r2'], 3);
    assert.deepEqual(ds.data(), [['Index', 'A', 'B'], ['r1', 1, null], ['r2', 3, 2]]);
  });

  test('append, select, update, delete, sort', () => {
    const ds = new Dataset();
    ds.appendColumn('Count', []);
    ds.appendRow('b', [2]).appendRow('a', [1]);
    assert.deepEqual(ds.selectColumn('Count'), ['Count', 2, 1]);
    assert.deepEqual(ds.selectRow('a'), ['a', 1]);
    ds.updateColumn(1, (v) => v * 10);
    ds.sortRows('asc');
    assert.deepEqual(ds.data(), [['Index', 'Count'], ['a', 10], ['b', 20]]);
    ds.deleteRow('a').deleteColumn('Count');
    assert.deepEqual(ds.data(), [['Index'], ['b']]);
  });

  test('unknown parser throws', () => {
    assert.throws(() => Dataset.parser('nope'), /does not exist/);
  });
});

describe('parser selection from the echoed query', () => {
  test('metric', () => {
    const out = parseResponse({ result: 42, query: { analysis_type: 'count', event_collection: 'pageviews' } });
    assert.equal(out.parser, 'metric');
    assert.equal(out.type, 'metric');
    assert.equal(out.title, 'Count - pageviews');
    assert.deepEqual(out.dataset.data(), [['Index', 'Value'], ['Result', 42]]);
  });

  test('grouped metric', () => {
    const out = parseResponse({
      result: [{ country: 'US', result: 3 }, { country: 'FR', result: 1 }],
      query: { analysis_type: 'count', group_by: 'country' },
    });
    assert.equal(out.parser, 'grouped-metric');
    assert.equal(out.type, 'bar');
    assert.deepEqual(out.dataset.data(), [['Index', 'Result'], ['US', 3], ['FR', 1]]);
  });

  test('double grouped metric', () => {
    const out = parseResponse({
      result: [{ a: 'x', b: 'y', result: 3 }],
      query: { analysis_type: 'sum', group_by: ['a', 'b'] },
    });
    assert.equal(out.parser, 'double-grouped-metric');
    assert.deepEqual(out.dataset.data(), [['Index', 'Result'], ['x y', 3]]);
  });

  test('interval rows become Date objects, indexed by start or end', () => {
    const res = { result: [{ timeframe: tf(1), value: 5 }, { timeframe: tf(2), value: 7 }], query: { analysis_type: 'count', interval: 'daily' } };
    const out = parseResponse(res);
    assert.equal(out.parser, 'interval');
    assert.equal(out.type, 'area');
    const m = out.dataset.data();
    assert.deepEqual(m[0], ['Index', 'Result']);
    assert.ok(m[1][0] instanceof Date);
    assert.equal(m[1][0].toISOString(), tf(1).start);
    assert.equal(m[2][1], 7);
    const byEnd = parseResponse(res, { indexBy: 'timeframe.end' });
    assert.equal(byEnd.dataset.data()[1][0].toISOString(), tf(1).end);
  });

  test('grouped interval', () => {
    const out = parseResponse({
      result: [
        { timeframe: tf(1), value: [{ os: 'mac', result: 1 }, { os: 'win', result: 2 }] },
        { timeframe: tf(2), value: [] },
      ],
      query: { analysis_type: 'count', interval: 'daily', group_by: 'os' },
    });
    assert.equal(out.parser, 'grouped-interval');
    assert.equal(out.type, 'line');
    const m = out.dataset.data();
    assert.deepEqual(m[0], ['Index', 'mac', 'win']);
    assert.deepEqual(m[1].slice(1), [1, 2]);
    assert.deepEqual(m[2].slice(1), [null, null]);
  });

  test('funnel, ad-hoc (steps at top level) and saved (nested)', () => {
    const steps = [{ event_collection: 'visit' }, { event_collection: 'signup' }];
    const adHoc = parseResponse({ result: [10, 3], steps, query: { analysis_type: 'funnel' } });
    assert.equal(adHoc.parser, 'funnel');
    assert.equal(adHoc.type, 'horizontal-bar');
    assert.deepEqual(adHoc.dataset.data(), [['Index', 'Step Value'], ['visit', 10], ['signup', 3]]);
    const saved = parseResponse({ result: { result: [10, 3], steps }, query: { analysis_type: 'funnel' } });
    assert.deepEqual(saved.dataset.data(), adHoc.dataset.data());
  });

  test('select_unique becomes a list', () => {
    const out = parseResponse({ result: ['a', 'b'], query: { analysis_type: 'select_unique' } });
    assert.equal(out.parser, 'list');
    assert.equal(out.type, 'table');
    assert.deepEqual(out.dataset.data(), [['Index', 'Result'], ['1', 'a'], ['2', 'b']]);
  });

  test('extraction flattens nested properties into columns', () => {
    const out = parseResponse({
      result: [
        { user: { id: 1, plan: 'pro' }, incisive: { timestamp: 't1' } },
        { user: { id: 2 }, price: 5, incisive: { timestamp: 't2' } },
      ],
      query: { analysis_type: 'extraction' },
    });
    assert.equal(out.parser, 'extraction');
    assert.deepEqual(out.dataset.data(), [
      ['user.id', 'user.plan', 'incisive.timestamp', 'price'],
      [1, 'pro', 't1', null],
      [2, null, 't2', 5],
    ]);
    const cols = parseResponse({ result: [{ a: 1, b: 2 }], query: { analysis_type: 'extraction' } }, { columns: ['b'] });
    assert.deepEqual(cols.dataset.data(), [['b'], [2]]);
  });
});

describe('parser selection by shape sniffing (no echoed query)', () => {
  const cases = [
    [{ result: 5 }, 'metric'],
    [{ result: [{ timeframe: tf(1), value: 1 }] }, 'interval'],
    [{ result: [{ timeframe: tf(1), value: null }] }, 'interval'],
    [{ result: [{ x: 'a', result: 1 }] }, 'grouped-metric'],
    [{ result: [{ timeframe: tf(1), value: [{ x: 'a', result: 1 }] }] }, 'grouped-interval'],
    [{ result: [3, 1], steps: [{ event_collection: 'a' }, { event_collection: 'b' }] }, 'funnel'],
    [{ result: ['a', 'b'] }, 'list'],
    [{ result: [{ any: 'event' }] }, 'extraction'],
  ];
  for (const [res, expected] of cases) {
    test(`${JSON.stringify(res).slice(0, 60)} -> ${expected}`, () => {
      assert.equal(detectParser(res).parser, expected);
    });
  }

  test('heatmap type picks the heatmap parser', () => {
    assert.equal(detectParser({ result: [{ x: 1, y: 2, result: 3 }] }, { type: 'heatmap' }).parser, 'heatmap-axis');
  });
});

describe('titles and chart types from saved query metadata', () => {
  test('metadata wins over defaults', () => {
    const out = parseResponse({
      result: 1,
      query: { analysis_type: 'count_unique', event_collection: 'e' },
      metadata: { display_name: 'Uniques', visualization: { chart_type: 'metric-big' } },
      execution_metadata: { total_processing_time: 1 },
    });
    assert.equal(out.title, 'Uniques');
    assert.equal(out.type, 'metric-big');
    assert.deepEqual(out.executionMetadata, { total_processing_time: 1 });
  });

  test('default title replaces the first underscore and capitalizes words', () => {
    assert.equal(defaultTitle({ analysis_type: 'count_unique', event_collection: 'signups' }), 'Count Unique - signups');
  });
});
