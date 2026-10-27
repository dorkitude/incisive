// In-process HTTP server that records requests and replays queued responses.

import http from 'node:http';

export async function startMockServer() {
  const requests = [];
  const responses = [];
  let fallback = { status: 200, body: {} };

  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const url = new URL(req.url, 'http://127.0.0.1');
      const bodyText = Buffer.concat(chunks).toString('utf8');
      const record = {
        method: req.method,
        rawUrl: req.url,
        path: url.pathname,
        rawPath: req.url.split('?')[0],
        query: Object.fromEntries(url.searchParams.entries()),
        rawQuery: req.url.indexOf('?') > -1 ? req.url.slice(req.url.indexOf('?') + 1) : '',
        headers: req.headers,
        bodyText,
        body: (() => {
          try {
            return bodyText ? JSON.parse(bodyText) : undefined;
          } catch (e) {
            return undefined;
          }
        })(),
      };
      requests.push(record);
      const next = responses.length ? responses.shift() : fallback;
      const spec = typeof next === 'function' ? next(record) : next;
      const headers = { ...(spec.headers || {}) };
      let payload = '';
      if (spec.raw !== undefined) {
        payload = spec.raw;
      } else if (spec.body !== undefined && spec.status !== 204) {
        payload = JSON.stringify(spec.body);
        headers['content-type'] = headers['content-type'] || 'application/json';
      }
      res.writeHead(spec.status || 200, headers);
      res.end(payload);
    });
  });

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();

  return {
    url: `http://127.0.0.1:${port}`,
    host: `127.0.0.1:${port}`,
    requests,
    reply(spec) {
      responses.push(spec);
      return this;
    },
    setFallback(spec) {
      fallback = spec;
      return this;
    },
    reset() {
      requests.length = 0;
      responses.length = 0;
      fallback = { status: 200, body: {} };
    },
    close() {
      return new Promise((resolve) => server.close(resolve));
    },
  };
}
