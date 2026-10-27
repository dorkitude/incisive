import { test, describe, before, after, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';

import IncisiveTracking, { TrackingError, helpers } from 'incisive-tracking';
import { base64Decode } from 'incisive-core';
import { startMockServer } from './helpers/mock-server.js';

let server;

function client(extra = {}) {
  return new IncisiveTracking({
    projectId: 'PROJECT',
    writeKey: 'WRITE_KEY',
    baseUrl: server.url,
    retry: { initialDelay: 1 },
    ...extra,
  });
}

before(async () => {
  server = await startMockServer();
});
after(() => server.close());
beforeEach(() => {
  server.reset();
  IncisiveTracking.enabled = true;
  IncisiveTracking.optedOut = false;
  IncisiveTracking.doNotTrack = false;
});

describe('configuration', () => {
  test('defaults to localhost:8080 over http and API version 3.0', () => {
    const c = new IncisiveTracking({ projectId: 'P', writeKey: 'W' });
    assert.equal(c.url('events'), 'http://localhost:8080/3.0/projects/P/events');
  });

  test('host + protocol form is supported, including a port', () => {
    const c = new IncisiveTracking({ projectId: 'P', writeKey: 'W', host: 'example.com:9000', protocol: 'https' });
    assert.equal(c.url('events', 'x'), 'https://example.com:9000/3.0/projects/P/events/x');
  });

  test('host with a scheme sets the protocol', () => {
    const c = new IncisiveTracking({ projectId: 'P', host: 'https://example.com/' });
    assert.equal(c.url('projectId'), 'https://example.com/3.0/projects/P');
  });

  test('baseUrl with a path prefix and custom resources', () => {
    const c = new IncisiveTracking({ projectId: 'P', baseUrl: 'https://a.example/analytics/' });
    assert.equal(c.url('queries', 'count'), 'https://a.example/analytics/3.0/projects/P/queries/count');
    c.resources({ events: 'https://ingest.example/{projectId}/ev' });
    assert.equal(c.url('events'), 'https://ingest.example/P/ev');
    assert.equal(c.url('/custom', { a: 1 }), 'https://a.example/analytics/custom?a=1');
  });

  test('credential accessors get and set', () => {
    const c = client();
    assert.equal(c.projectId(), 'PROJECT');
    c.writeKey('OTHER');
    assert.equal(c.writeKey(), 'OTHER');
    c.referrerPolicy('no-referrer');
    assert.equal(c.referrerPolicy(), 'no-referrer');
  });
});

describe('recordEvent over fetch', () => {
  test('POSTs JSON to /events/{collection} with the write key header', async () => {
    server.reply({ status: 201, body: { created: true } });
    const c = client();
    let cbArgs;
    const res = await c.recordEvent('purchases', { price: 10, keen: { timestamp: '2026-01-01T00:00:00.000Z' } }, (...args) => { cbArgs = args; });
    assert.deepEqual(res, { created: true });
    assert.deepEqual(cbArgs, [null, { created: true }]);
    const req = server.requests[0];
    assert.equal(req.method, 'POST');
    assert.equal(req.path, '/3.0/projects/PROJECT/events/purchases');
    assert.equal(req.headers.authorization, 'WRITE_KEY');
    assert.equal(req.headers['content-type'], 'application/json');
    assert.deepEqual(Object.keys(req.headers).filter((h) => h.endsWith('-sdk')), []);
    assert.deepEqual(req.body, { price: 10, keen: { timestamp: '2026-01-01T00:00:00.000Z' } });
    assert.equal(req.rawQuery, '');
  });

  test('collection names are percent-encoded in the path', async () => {
    server.reply({ status: 201, body: { created: true } });
    await client().recordEvent('my coll/x?y', { a: 1 });
    assert.equal(server.requests[0].rawPath, '/3.0/projects/PROJECT/events/my%20coll%2Fx%3Fy');
  });

  test('object form with collection/event/callback', async () => {
    server.reply({ status: 201, body: { created: true } });
    const c = client();
    const res = await c.recordEvent({ collection: 'signups', event: { plan: 'pro' } });
    assert.equal(res.created, true);
    assert.deepEqual(server.requests[0].body, { plan: 'pro' });
  });

  test('extensions are deep-merged: global, then collection, then event', async () => {
    server.reply({ status: 201, body: { created: true } });
    const c = client();
    c.extendEvents({ app: { name: 'web', version: 1 }, source: 'global' });
    c.extendEvents(() => ({ dynamic: true }));
    c.extendEvent('clicks', { source: 'collection', app: { version: 2 } });
    await c.recordEvent('clicks', { source: 'event' });
    assert.deepEqual(server.requests[0].body, {
      app: { name: 'web', version: 2 }, source: 'event', dynamic: true,
    });
  });

  test('API error envelope rejects with error_code, body, status', async () => {
    server.reply({ status: 401, body: { message: 'Invalid write key', error_code: 'InvalidApiKeyError' } });
    const c = client();
    const errors = [];
    c.on('error', (e) => errors.push(e));
    await assert.rejects(c.recordEvent('x', {}), (err) => {
      assert.ok(err instanceof TrackingError);
      assert.equal(err.ok, false);
      assert.equal(err.status, 401);
      assert.equal(err.error_code, 'InvalidApiKeyError');
      assert.equal(err.body, 'Invalid write key');
      return true;
    });
    assert.equal(errors.length, 1);
  });

  test('created:false is treated as a failure', async () => {
    server.reply({ status: 200, body: { created: false } });
    await assert.rejects(client().recordEvent('x', {}), TrackingError);
  });

  test('callback-only callers receive the error without an unhandled rejection', async () => {
    server.reply({ status: 400, body: { message: 'bad', error_code: 'InvalidPropertyNameError' } });
    const err = await new Promise((resolve) => {
      client().recordEvent('x', { '': 1 }, (e) => resolve(e));
    });
    assert.equal(err.error_code, 'InvalidPropertyNameError');
  });

  test('retries on 503 and then succeeds', async () => {
    server.reply({ status: 503, body: { message: 'busy', error_code: 'ServiceUnavailable' } });
    server.reply({ status: 503, body: { message: 'busy', error_code: 'ServiceUnavailable' } });
    server.reply({ status: 201, body: { created: true } });
    const res = await client().recordEvent('x', { n: 1 });
    assert.equal(res.created, true);
    assert.equal(server.requests.length, 3);
    assert.deepEqual(server.requests.map((r) => r.body), [{ n: 1 }, { n: 1 }, { n: 1 }]);
  });

  test('gives up after the retry limit and surfaces the last response', async () => {
    server.setFallback({ status: 500, body: { message: 'boom', error_code: 'InternalServerError' } });
    await assert.rejects(client({ retry: { limit: 2, initialDelay: 1 } }).recordEvent('x', {}), (err) => err.status === 500);
    assert.equal(server.requests.length, 3);
  });

  test('does not retry on 400', async () => {
    server.reply({ status: 400, body: { message: 'bad', error_code: 'InvalidCollectionNameError' } });
    await assert.rejects(client().recordEvent('x', {}));
    assert.equal(server.requests.length, 1);
  });
});

describe('validation and privacy switches', () => {
  test('missing write key: callback gets a message, nothing is sent, returns undefined', () => {
    const c = new IncisiveTracking({ projectId: 'P', baseUrl: server.url });
    let msg;
    const ret = c.recordEvent('x', {}, (err) => { msg = err; });
    assert.equal(ret, undefined);
    assert.match(msg, /missing a writeKey/);
    assert.equal(server.requests.length, 0);
  });

  test('missing project id', () => {
    const c = new IncisiveTracking({ writeKey: 'W' });
    let msg;
    c.recordEvent('x', {}, (err) => { msg = err; });
    assert.match(msg, /missing a projectId/);
  });

  test('non-string collection', () => {
    let msg;
    client().recordEvent(42, {}, (err) => { msg = err; });
    assert.match(msg, /Collection name must be a string/);
  });

  test('opt-out resolves created:false without a request', async () => {
    const c = client();
    c.setOptOut(true);
    const res = await c.recordEvent('x', {});
    assert.equal(res.created, false);
    assert.equal(server.requests.length, 0);
    c.setOptOut(false);
  });

  test('disabled tracking reports an error and sends nothing', () => {
    IncisiveTracking.enabled = false;
    let msg;
    const ret = client().recordEvent('x', {}, (e) => { msg = e; });
    assert.equal(ret, false);
    assert.match(msg, /disabled/);
    assert.equal(server.requests.length, 0);
  });

  test('unique events are only sent once within the cache window', async () => {
    server.setFallback({ status: 201, body: { created: true } });
    const c = client();
    const first = await c.recordEvent({ collection: 'x', event: { id: 1 }, unique: true });
    const second = await c.recordEvent({ collection: 'x', event: { id: 1 }, unique: true });
    const third = await c.recordEvent({ collection: 'x', event: { id: 2 }, unique: true });
    assert.equal(first.created, true);
    assert.equal(second.created, false);
    assert.match(second.message, /NOT_UNIQUE/);
    assert.equal(third.created, true);
    assert.equal(server.requests.length, 2);
  });
});

describe('recordEvents (batch)', () => {
  test('POSTs the collection map to /events', async () => {
    server.reply({ status: 200, body: { a: [{ success: true }, { success: true }], b: [{ success: true }] } });
    const c = client();
    c.extendEvent('a', { tagged: true });
    const res = await c.recordEvents({ a: [{ n: 1 }, { n: 2 }], b: [{ n: 3 }] });
    assert.equal(res.a.length, 2);
    const req = server.requests[0];
    assert.equal(req.method, 'POST');
    assert.equal(req.path, '/3.0/projects/PROJECT/events');
    assert.equal(req.headers.authorization, 'WRITE_KEY');
    assert.deepEqual(req.body, { a: [{ tagged: true, n: 1 }, { tagged: true, n: 2 }], b: [{ n: 3 }] });
  });

  test('a partially failed batch rejects and exposes per-event results', async () => {
    const body = {
      a: [{ success: true }, { success: false, error: { name: 'InvalidPropertyNameError', description: 'bad name' } }],
    };
    server.reply({ status: 200, body });
    await assert.rejects(client().recordEvents({ a: [{ ok: 1 }, { '.bad': 1 }] }), (err) => {
      assert.ok(err instanceof TrackingError);
      assert.deepEqual(err.response, body);
      assert.equal(err.a[1].error.name, 'InvalidPropertyNameError');
      return true;
    });
  });

  test('rejects arrays as the first argument', () => {
    let msg;
    client().recordEvents([{ a: 1 }], (e) => { msg = e; });
    assert.match(msg, /must be an object/);
  });
});

describe('deferred events', () => {
  test('recordDeferredEvents flushes the queue as one batch', async () => {
    server.reply({ status: 200, body: { a: [{ success: true }], b: [{ success: true }, { success: true }] } });
    const c = client();
    c.deferEvent('a', { n: 1 });
    c.deferEvents({ b: [{ n: 2 }, { n: 3 }] });
    assert.equal(c.queue.capacity, 3);
    c.recordDeferredEvents();
    await c._lastDeferred;
    assert.equal(server.requests.length, 1);
    assert.deepEqual(server.requests[0].body, { a: [{ n: 1 }], b: [{ n: 2 }, { n: 3 }] });
    assert.equal(c.queue.capacity, 0);
  });

  test('reaching queue capacity flushes automatically', async () => {
    server.reply({ status: 200, body: { a: [{ success: true }, { success: true }] } });
    const c = client({ queue: { capacity: 2, interval: 0 } });
    assert.equal(c.queueCapacity(), 2);
    c.deferEvent('a', { n: 1 });
    assert.equal(server.requests.length, 0);
    c.deferEvent('a', { n: 2 });
    await c._lastDeferred;
    assert.equal(server.requests.length, 1);
  });

  test('the interval timer flushes queued events', async () => {
    server.reply({ status: 200, body: { a: [{ success: true }] } });
    const c = client();
    c.queueInterval(0.05);
    c.deferEvent('a', { n: 1 });
    await new Promise((r) => setTimeout(r, 150));
    await c._lastDeferred;
    assert.equal(server.requests.length, 1);
  });

  test('failed deferred delivery emits recordDeferredEventsError', async () => {
    server.reply({ status: 400, body: { message: 'bad', error_code: 'X' } });
    const c = client();
    const seen = new Promise((resolve) => c.on('recordDeferredEventsError', (err, events) => resolve({ err, events })));
    c.deferEvent('a', { n: 1 });
    c.recordDeferredEvents();
    const { err, events } = await seen;
    assert.equal(err.error_code, 'X');
    assert.deepEqual(events, { a: [{ n: 1 }] });
  });
});

describe('browser transports', () => {
  let saved;
  beforeEach(() => {
    saved = { navigator: Object.getOwnPropertyDescriptor(globalThis, 'navigator'), document: globalThis.document };
  });
  afterEach(() => {
    if (saved.navigator) Object.defineProperty(globalThis, 'navigator', saved.navigator);
    else delete globalThis.navigator;
    if (saved.document === undefined) delete globalThis.document;
    else globalThis.document = saved.document;
  });

  function stubNavigator(nav) {
    Object.defineProperty(globalThis, 'navigator', { value: nav, configurable: true, writable: true });
  }

  test('beaconAPI sends text JSON to ?api_key= without an Authorization header', async () => {
    const calls = [];
    stubNavigator({
      userAgent: 'test',
      sendBeacon(url, body) {
        calls.push({ url, body });
        // Deliver it like a browser would: text/plain body, no custom headers.
        fetch(url, { method: 'POST', body, headers: { 'content-type': 'text/plain;charset=UTF-8' } });
        return true;
      },
    });
    const c = client({ requestType: 'beaconAPI' });
    const res = await c.recordEvent('pageviews', { page: '/' });
    assert.deepEqual(res, { queued: true });
    assert.equal(calls[0].url, `${server.url}/3.0/projects/PROJECT/events/pageviews?api_key=WRITE_KEY`);
    assert.equal(calls[0].body, '{"page":"/"}');
    await new Promise((r) => setTimeout(r, 50));
    const req = server.requests[0];
    assert.equal(req.method, 'POST');
    assert.equal(req.query.api_key, 'WRITE_KEY');
    assert.equal(req.headers.authorization, undefined);
    assert.match(req.headers['content-type'], /^text\/plain/);
    assert.deepEqual(JSON.parse(req.bodyText), { page: '/' });
  });

  test('beaconAPI batch goes to /events?api_key=', async () => {
    const calls = [];
    stubNavigator({ userAgent: 'test', sendBeacon: (url, body) => { calls.push({ url, body }); return true; } });
    await client({ requestType: 'beaconAPI' }).recordEvents({ a: [{ n: 1 }] });
    assert.equal(calls[0].url, `${server.url}/3.0/projects/PROJECT/events?api_key=WRITE_KEY`);
    assert.deepEqual(JSON.parse(calls[0].body), { a: [{ n: 1 }] });
  });

  test('per-call requestType overrides the client transport', async () => {
    const calls = [];
    stubNavigator({ userAgent: 'test', sendBeacon: (url, body) => { calls.push({ url, body }); return true; } });
    await client().recordEvent({ collection: 'x', event: {}, requestType: 'beaconAPI' });
    assert.equal(calls.length, 1);
    assert.equal(server.requests.length, 0);
  });

  test('beaconAPI falls back to fetch when sendBeacon is unavailable', async () => {
    server.reply({ status: 201, body: { created: true } });
    await client({ requestType: 'beaconAPI' }).recordEvent('x', { a: 1 });
    assert.equal(server.requests[0].headers.authorization, 'WRITE_KEY');
  });

  test('img transport issues a GET with api_key, base64 data, modified and c=clv1', async () => {
    stubNavigator({ userAgent: 'test' });
    const gif = Buffer.from('R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7', 'base64');
    server.reply({ status: 200, raw: gif, headers: { 'content-type': 'image/gif' } });
    globalThis.document = {
      createElement() {
        const img = {
          naturalWidth: 0,
          naturalHeight: 0,
          set src(value) {
            fetch(value).then(async (r) => {
              const buf = Buffer.from(await r.arrayBuffer());
              if (r.ok && buf.length) {
                img.naturalWidth = 1;
                img.naturalHeight = 1;
                img.onload.call(img);
              } else {
                img.onerror();
              }
            });
          },
        };
        return img;
      },
    };
    const event = { page: '/ünïcode', n: 1 };
    const res = await client({ requestType: 'img' }).recordEvent('page views', event);
    assert.deepEqual(res, { created: true });
    const req = server.requests[0];
    assert.equal(req.method, 'GET');
    assert.equal(req.rawPath, '/3.0/projects/PROJECT/events/page%20views');
    assert.equal(req.query.api_key, 'WRITE_KEY');
    assert.equal(req.query.c, 'clv1');
    assert.match(req.query.modified, /^\d{13}$/);
    assert.deepEqual(JSON.parse(base64Decode(req.query.data)), event);
    assert.equal(req.headers.authorization, undefined);
  });

  test('img transport reports an empty image as an error', async () => {
    stubNavigator({ userAgent: 'test' });
    globalThis.document = {
      createElement() {
        const img = { naturalWidth: 0, naturalHeight: 0 };
        Object.defineProperty(img, 'src', { set() { setTimeout(() => img.onload.call(img), 0); } });
        return img;
      },
    };
    await assert.rejects(client({ requestType: 'img' }).recordEvent('x', {}), /empty image/);
  });
});

describe('auto-tracking', () => {
  test('records a page view over the Beacon API with enrichment instructions', async () => {
    const saved = {
      navigator: Object.getOwnPropertyDescriptor(globalThis, 'navigator'),
      window: globalThis.window,
      document: globalThis.document,
    };
    const beacons = [];
    const listeners = {};
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      writable: true,
      value: { userAgent: 'test-agent', language: 'en', sendBeacon: (url, body) => { beacons.push({ url, body }); return true; } },
    });
    globalThis.document = {
      cookie: '',
      title: 'Home',
      referrer: 'https://ref.example/',
      hidden: false,
      addEventListener: (type, fn) => { listeners[type] = fn; },
      querySelector: () => null,
      querySelectorAll: () => [],
    };
    globalThis.window = {
      location: { hostname: 'www.example.com', href: 'https://www.example.com/pricing' },
      addEventListener: () => {},
    };
    try {
      const c = client();
      c.initAutoTracking({ recordClicks: false, recordFormSubmits: false, recordScrollState: false, recordElementViews: false });
      assert.equal(c.config.requestType, 'beaconAPI');
      await new Promise((r) => setTimeout(r, 10));
      assert.equal(beacons.length, 1);
      assert.equal(beacons[0].url, `${server.url}/3.0/projects/PROJECT/events/pageviews?api_key=WRITE_KEY`);
      const event = JSON.parse(beacons[0].body);
      assert.equal(event.url.full, 'https://www.example.com/pricing');
      assert.equal(event.referrer.full, 'https://ref.example/');
      assert.equal(event.page.title, 'Home');
      assert.equal(event.ip_address, '${keen.ip}');
      assert.equal(event.user_agent, '${keen.user_agent}');
      assert.match(event.user.uuid, /^[0-9a-f-]{36}$/);
      assert.match(event.tracked_by, /^incisive-tracking-/);
      assert.match(event.keen.timestamp, /^\d{4}-\d\d-\d\dT/);
      assert.deepEqual(event.keen.addons.map((a) => a.name), [
        'keen:ua_parser', 'keen:url_parser', 'keen:url_parser', 'keen:date_time_parser', 'keen:date_time_parser', 'keen:ip_to_geo',
      ]);
      assert.deepEqual(event.keen.addons[5], { name: 'keen:ip_to_geo', input: { ip: 'ip_address', remove_ip_property: false }, output: 'geo' });
      assert.match(decodeURIComponent(globalThis.document.cookie), /^incisive=\{.*"uuid"/);
    } finally {
      if (saved.navigator) Object.defineProperty(globalThis, 'navigator', saved.navigator);
      else delete globalThis.navigator;
      if (saved.window === undefined) delete globalThis.window; else globalThis.window = saved.window;
      if (saved.document === undefined) delete globalThis.document; else globalThis.document = saved.document;
    }
  });

  test('catchError switches the default transport to fetch', async () => {
    const saved = { window: globalThis.window, document: globalThis.document };
    globalThis.document = { cookie: '', title: '', referrer: '', addEventListener: () => {}, querySelector: () => null, querySelectorAll: () => [] };
    globalThis.window = { location: { hostname: 'localhost', href: 'http://localhost/' }, addEventListener: () => {} };
    server.reply({ status: 201, body: { created: true } });
    try {
      const c = client();
      c.initAutoTracking({ catchError: () => {}, recordClicks: false, recordFormSubmits: false, recordScrollState: false, recordElementViews: false });
      assert.equal(c.config.requestType, 'fetch');
      await new Promise((r) => setTimeout(r, 50));
      assert.equal(server.requests[0].path, '/3.0/projects/PROJECT/events/pageviews');
      assert.equal(server.requests[0].headers.authorization, 'WRITE_KEY');
    } finally {
      if (saved.window === undefined) delete globalThis.window; else globalThis.window = saved.window;
      if (saved.document === undefined) delete globalThis.document; else globalThis.document = saved.document;
    }
  });
});

describe('helpers', () => {
  test('getDatetimeIndex', () => {
    const d = new Date(2026, 9, 27, 13, 5);
    assert.deepEqual(helpers.getDatetimeIndex(d), {
      hour_of_day: 13, day_of_week: d.getDay() + 1, day_of_month: 27, month: 10, year: 2026,
    });
  });

  test('getDomainName', () => {
    assert.equal(helpers.getDomainName('www.example.com'), 'example.com');
    assert.equal(helpers.getDomainName('a.b.example.co.uk'), 'example.co.uk');
    assert.equal(helpers.getDomainName('localhost'), 'localhost');
    assert.equal(helpers.getDomainName('10.0.0.1'), '10.0.0.1');
  });

  test('getUniqueId returns a v4 UUID', () => {
    assert.match(helpers.getUniqueId(), /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  });

  test('browser-only helpers degrade outside a browser', () => {
    assert.deepEqual(helpers.getWindowProfile(), {});
    assert.equal(helpers.getDomNodePath(null), '');
  });

  test('serializeForm skips password fields and unchecked boxes', () => {
    const form = {
      elements: [
        { name: 'email', type: 'email', value: 'a@b.c' },
        { name: 'pw', type: 'password', value: 'secret' },
        { name: 'news', type: 'checkbox', checked: false, value: 'yes' },
        { name: 'tags', type: 'checkbox', checked: true, value: 'a' },
        { name: 'tags', type: 'checkbox', checked: true, value: 'b' },
        { name: 'off', type: 'text', disabled: true, value: 'x' },
      ],
    };
    assert.deepEqual(IncisiveTracking.utils.serializeForm(form, { disabled: true }), { email: 'a@b.c', tags: ['a', 'b'] });
  });
});
