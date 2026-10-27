"""In-process mock HTTP server recording every request."""

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

import pytest

import incisive


class Recorded:
    def __init__(self, method, raw_path, headers, body):
        self.method = method
        self.raw_path = raw_path
        split = urlsplit(raw_path)
        self.path = split.path
        self.raw_query = split.query
        self.query = {k: v[0] for k, v in parse_qs(split.query, keep_blank_values=True).items()}
        self.headers = headers
        self.body = body

    def json(self):
        return json.loads(self.body.decode("utf-8"))

    def query_json(self, name):
        return json.loads(self.query[name])


class MockServer:
    def __init__(self):
        self.requests = []
        self.responses = []  # FIFO of (status, body, content_type); falls back to default
        self.default = (200, {"result": None}, "application/json")
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def _handle(self):
                length = int(self.headers.get("Content-Length") or 0)
                body = self.rfile.read(length) if length else b""
                outer.requests.append(Recorded(self.command, self.path, dict(self.headers), body))
                status, payload, ctype = outer.responses.pop(0) if outer.responses else outer.default
                if payload is None:
                    data = b""
                elif isinstance(payload, (bytes, str)):
                    data = payload.encode() if isinstance(payload, str) else payload
                else:
                    data = json.dumps(payload).encode()
                self.send_response(status)
                if data:
                    self.send_header("Content-Type", ctype)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            do_GET = do_POST = do_PUT = do_DELETE = _handle

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = "http://127.0.0.1:{0}".format(self.httpd.server_address[1])
        self.thread = threading.Thread(target=self.httpd.serve_forever, kwargs={"poll_interval": 0.01},
                                       daemon=True)
        self.thread.start()

    def respond(self, status=200, body=None, content_type="application/json"):
        self.responses.append((status, body, content_type))

    @property
    def last(self):
        return self.requests[-1]

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()


@pytest.fixture
def server():
    s = MockServer()
    yield s
    s.stop()


@pytest.fixture
def client(server):
    return incisive.IncisiveClient("proj1", write_key="wk", read_key="rk", master_key="mk",
                                   base_url=server.url)


@pytest.fixture
def reset_module(monkeypatch):
    for name in ("INCISIVE_PROJECT_ID", "INCISIVE_WRITE_KEY", "INCISIVE_READ_KEY",
                 "INCISIVE_MASTER_KEY", "INCISIVE_BASE_URL"):
        monkeypatch.delenv(name, raising=False)
    for attr in ("_client", "project_id", "write_key", "read_key", "master_key", "base_url"):
        monkeypatch.setattr(incisive, attr, None)
    yield
