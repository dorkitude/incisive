import base64
import datetime
import json
from urllib.parse import parse_qs, urlsplit

import pytest

import incisive
from incisive import persistence_strategies


def assert_std_headers(req, key):
    assert req.headers["Authorization"] == key
    assert req.headers["Content-Type"] == "application/json"
    assert req.headers["User-Agent"].startswith("incisive-python/")
    assert not any(h.lower().endswith("-sdk") for h in req.headers)


def test_add_event_wire(server, client):
    server.respond(201, {"created": True})
    assert client.add_event("purchases", {"price": 10, "user": {"id": 1}}) is None
    req = server.last
    assert req.method == "POST"
    assert req.path == "/1.0/projects/proj1/events/purchases"
    assert req.raw_query == ""
    assert_std_headers(req, "wk")
    assert req.json() == {"price": 10, "user": {"id": 1}}


def test_add_event_timestamp_merges_reserved_object(server, client):
    ts = datetime.datetime(2026, 1, 2, 3, 4, 5)
    body = {"a": 1, "incisive": {"addons": []}}
    client.add_event("c", body, timestamp=ts)
    assert server.last.json() == {"a": 1, "incisive": {"addons": [], "timestamp": "2026-01-02T03:04:05"}}
    assert body == {"a": 1, "incisive": {"addons": []}}  # caller's dict untouched


def test_collection_name_is_escaped(server, client):
    client.add_event("my coll/with?odd#chars", {"x": 1})
    assert server.last.raw_path == "/1.0/projects/proj1/events/my%20coll%2Fwith%3Fodd%23chars"


def test_add_events_batch(server, client):
    reply = {"a": [{"success": True}], "b": [{"success": False, "error": {"name": "x"}}]}
    server.respond(200, reply)
    events = {"a": [{"n": 1}], "b": [{"n": 2}, {"n": 3}]}
    assert client.add_events(events) == reply
    req = server.last
    assert (req.method, req.path) == ("POST", "/1.0/projects/proj1/events")
    assert_std_headers(req, "wk")
    assert req.json() == events


def test_write_key_required(server):
    c = incisive.IncisiveClient("p", read_key="rk", base_url=server.url)
    with pytest.raises(incisive.InvalidEnvironmentError, match="write key"):
        c.add_event("c", {})
    assert server.requests == []


def test_api_version_and_base_url_trailing_slash(server):
    c = incisive.IncisiveClient("p", write_key="wk", base_url=server.url + "/", api_version="9.9")
    c.add_event("c", {})
    assert server.last.path == "/9.9/projects/p/events/c"


def test_default_base_url():
    c = incisive.IncisiveClient("p")
    assert c.api.base_url == "http://localhost:8080"
    assert c.api.api_version == "1.0"


def test_timeouts_passed(monkeypatch, server):
    seen = {}
    c = incisive.IncisiveClient("p", write_key="wk", read_key="rk", base_url=server.url,
                                get_timeout=3, post_timeout=7)
    real = c.api.fulfill

    def spy(method, *a, **kw):
        seen[method] = kw.get("timeout")
        return real(method, *a, **kw)

    monkeypatch.setattr(c.api, "fulfill", spy)
    server.respond(200, {"result": 1})
    c.add_event("c", {})
    c.count("c")
    assert seen == {"post": 7, "get": 3}


def test_image_beacon(client):
    url = client.generate_image_beacon("my coll", {"a": 1})
    parts = urlsplit(url)
    assert parts.path == "/1.0/projects/proj1/events/my%20coll"
    qs = parse_qs(parts.query)
    assert qs["api_key"] == ["wk"]
    assert json.loads(base64.b64decode(qs["data"][0])) == {"a": 1}


def test_invalid_project_id():
    with pytest.raises(incisive.InvalidProjectIdError):
        incisive.IncisiveClient("")
    with pytest.raises(incisive.InvalidProjectIdError):
        incisive.IncisiveClient(123)


def test_persistence_strategy_validation(server):
    with pytest.raises(incisive.InvalidPersistenceStrategyError):
        incisive.IncisiveClient("p", persistence_strategy=object())

    class Collect(persistence_strategies.BasePersistenceStrategy):
        def __init__(self):
            self.events = []

        def persist(self, event):
            self.events.append(event.to_dict())

    s = Collect()
    c = incisive.IncisiveClient("p", write_key="wk", persistence_strategy=s, base_url=server.url)
    c.add_event("c", {"z": 1})
    assert s.events == [{"z": 1}]
    assert server.requests == []
