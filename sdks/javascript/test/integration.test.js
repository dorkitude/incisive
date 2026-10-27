// Opt-in tests against a real incisive server. Set INCISIVE_BASE_URL,
// INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY and INCISIVE_READ_KEY (and optionally
// INCISIVE_MASTER_KEY) to run them; otherwise they are skipped.

import { test } from 'node:test';
import assert from 'node:assert/strict';

import IncisiveTracking from 'incisive-tracking';
import IncisiveAnalysis from 'incisive-analysis';
import { parseResponse } from 'incisive-dataset';

const env = process.env;
const enabled = Boolean(env.INCISIVE_BASE_URL && env.INCISIVE_PROJECT_ID && env.INCISIVE_WRITE_KEY && env.INCISIVE_READ_KEY);
const skip = enabled ? false : 'set INCISIVE_BASE_URL, INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY and INCISIVE_READ_KEY';

const collection = `sdk_js_${Date.now()}`;

test('record events, then count them', { skip, timeout: 60000 }, async () => {
  const tracker = new IncisiveTracking({
    baseUrl: env.INCISIVE_BASE_URL,
    projectId: env.INCISIVE_PROJECT_ID,
    writeKey: env.INCISIVE_WRITE_KEY,
  });
  const single = await tracker.recordEvent(collection, { n: 1, nested: { flag: true } });
  assert.equal(single.created, true);
  const batch = await tracker.recordEvents({ [collection]: [{ n: 2 }, { n: 3 }] });
  assert.ok(batch[collection].every((r) => r.success));

  const analysis = new IncisiveAnalysis({
    baseUrl: env.INCISIVE_BASE_URL,
    projectId: env.INCISIVE_PROJECT_ID,
    readKey: env.INCISIVE_READ_KEY,
  });
  let count = 0;
  for (let i = 0; i < 20 && count < 3; i++) {
    const res = await analysis.query('count', { eventCollection: collection, timeframe: 'this_1_days', timezone: 'UTC' });
    count = res.result;
    if (count < 3) await new Promise((r) => setTimeout(r, 500));
  }
  assert.equal(count, 3);

  const sum = await analysis.query('sum', { eventCollection: collection, targetProperty: 'n', timeframe: 'this_1_days', timezone: 'UTC' });
  assert.equal(sum.result, 6);
  assert.equal(parseResponse(sum).parser, 'metric');

  if (env.INCISIVE_MASTER_KEY) {
    analysis.masterKey(env.INCISIVE_MASTER_KEY);
    await analysis.del({ url: analysis.url('events', encodeURIComponent(collection)), api_key: env.INCISIVE_MASTER_KEY });
  }
});
