"""Deletes, discovery, access keys, saved queries, cached datasets."""

import pytest

import incisive

KEY = {"key": "abc", "name": "old", "is_active": True, "permitted": ["queries"],
       "options": {"queries": {"filters": []}}, "project_id": "proj1"}


def test_delete_events(server, client):
    server.respond(204, None)
    filters = [{"property_name": "a", "operator": "eq", "property_value": 1}]
    assert client.delete_events("my coll", timeframe="this_1_days", timezone=3600,
                                filters=filters) is True
    req = server.last
    assert req.method == "DELETE"
    assert req.raw_path.startswith("/3.0/projects/proj1/events/my%20coll?")
    assert req.headers["Authorization"] == "mk"
    assert req.query["timeframe"] == "this_1_days"
    assert req.query["timezone"] == "3600"
    assert req.query_json("filters") == filters


def test_delete_whole_collection(server, client):
    server.respond(204, None)
    client.delete_events("c")
    assert server.last.raw_path == "/3.0/projects/proj1/events/c"


def test_delete_requires_master(server):
    c = incisive.IncisiveClient("p", read_key="rk", write_key="wk", base_url=server.url)
    with pytest.raises(incisive.InvalidEnvironmentError, match="master key"):
        c.delete_events("c")


def test_discovery(server, client):
    server.respond(200, {"properties": {"a": "num"}})
    assert client.get_collection("c") == {"properties": {"a": "num"}}
    assert (server.last.method, server.last.path) == ("GET", "/3.0/projects/proj1/events/c")
    assert server.last.headers["Authorization"] == "rk"
    server.respond(200, [{"name": "c", "properties": {}}])
    assert client.get_all_collections() == [{"name": "c", "properties": {}}]
    assert server.last.path == "/3.0/projects/proj1/events"


def test_access_key_crud(server, client):
    server.respond(201, KEY)
    assert client.create_access_key("n", permitted=["writes"]) == KEY
    req = server.last
    assert (req.method, req.path) == ("POST", "/3.0/projects/proj1/keys")
    assert req.headers["Authorization"] == "mk"
    assert req.json() == {"name": "n", "is_active": True, "permitted": ["writes"], "options": {}}

    server.respond(200, [KEY])
    assert client.list_access_keys() == [KEY]
    assert (server.last.method, server.last.path) == ("GET", "/3.0/projects/proj1/keys")

    server.respond(200, KEY)
    assert client.get_access_key("abc") == KEY
    assert server.last.path == "/3.0/projects/proj1/keys/abc"

    server.respond(200, KEY)
    client.update_access_key_full("abc", "x", False, ["schema"], {"o": 1})
    assert (server.last.method, server.last.path) == ("POST", "/3.0/projects/proj1/keys/abc")
    assert server.last.json() == {"name": "x", "is_active": False, "permitted": ["schema"],
                                  "options": {"o": 1}}

    server.respond(200, dict(KEY, is_active=False))
    assert client.revoke_access_key("abc")["is_active"] is False
    assert (server.last.method, server.last.path) == ("POST", "/3.0/projects/proj1/keys/abc/revoke")

    server.respond(204, None)
    assert client.unrevoke_access_key("abc") is None
    assert server.last.path == "/3.0/projects/proj1/keys/abc/unrevoke"

    server.respond(204, None)
    assert client.delete_access_key("abc") is True
    assert (server.last.method, server.last.path) == ("DELETE", "/3.0/projects/proj1/keys/abc")


@pytest.mark.parametrize("call,expected", [
    (lambda c: c.update_access_key_name("abc", "new"), {"name": "new"}),
    (lambda c: c.add_access_key_permissions("abc", ["writes", "queries"]),
     {"permitted": ["queries", "writes"]}),
    (lambda c: c.remove_access_key_permissions("abc", ["queries"]), {"permitted": []}),
    (lambda c: c.update_access_key_permissions("abc", ["schema"]), {"permitted": ["schema"]}),
    (lambda c: c.update_access_key_options("abc", {"writes": {}}), {"options": {"writes": {}}}),
])
def test_access_key_partial_updates(server, client, call, expected):
    server.respond(200, KEY)
    server.respond(200, KEY)
    call(client)
    get, post = server.requests
    assert (get.method, get.path) == ("GET", "/3.0/projects/proj1/keys/abc")
    assert (post.method, post.path) == ("POST", "/3.0/projects/proj1/keys/abc")
    want = {f: KEY[f] for f in ("name", "is_active", "permitted", "options")}
    want.update(expected)
    assert post.json() == want


def test_saved_queries(server, client):
    sq = client.saved_queries
    server.respond(200, [{"query_name": "q"}])
    assert sq.all() == [{"query_name": "q"}]
    assert (server.last.method, server.last.path) == ("GET", "/3.0/projects/proj1/queries/saved")
    assert server.last.headers["Authorization"] == "mk"

    server.respond(200, {"query_name": "q"})
    sq.get("my q")
    assert server.last.raw_path == "/3.0/projects/proj1/queries/saved/my%20q"

    server.respond(200, {"result": 5, "query_name": "q"})
    assert sq.results("q") == {"result": 5, "query_name": "q"}
    assert server.last.path == "/3.0/projects/proj1/queries/saved/q/result"
    assert server.last.headers["Authorization"] == "rk"

    definition = {"refresh_rate": 0, "query": {"analysis_type": "count", "event_collection": "c"}}
    server.respond(201, dict(definition, query_name="q"))
    sq.create("q", definition)
    assert (server.last.method, server.last.path) == ("PUT", "/3.0/projects/proj1/queries/saved/q")
    assert server.last.json() == definition

    server.respond(200, {})
    sq.update_full("q", '{"refresh_rate": 0}')  # pre-encoded strings pass through
    assert server.last.json() == {"refresh_rate": 0}

    server.respond(204, None)
    assert sq.delete("q") is True
    assert server.last.method == "DELETE"


def test_saved_query_update_merges(server, client):
    old = {"query_name": "q", "refresh_rate": 0, "urls": {"x": 1}, "run_information": None,
           "metadata": {"display_name": "Q"},
           "query": {"analysis_type": "count", "event_collection": "c", "group_by": None,
                     "timeframe": "this_1_days", "filters": []}}
    server.respond(200, old)
    server.respond(200, {})
    client.saved_queries.update("q", {"refresh_rate": 3600, "query": {"timeframe": "this_2_days"}})
    put = server.requests[1]
    assert put.method == "PUT"
    assert put.json() == {"query_name": "q", "refresh_rate": 3600,
                          "metadata": {"display_name": "Q"},
                          "query": {"analysis_type": "count", "event_collection": "c",
                                    "timeframe": "this_2_days"}}


def test_saved_query_non_json_body(server, client):
    server.respond(200, "ok", content_type="text/plain")
    assert client.saved_queries.all() == "No JSON available."


def test_cached_datasets(server, client):
    cd = client.cached_datasets
    server.respond(200, {"datasets": []})
    assert cd.all() == {"datasets": []}
    assert (server.last.method, server.last.path) == ("GET", "/3.0/projects/proj1/datasets")
    assert server.last.headers["Authorization"] == "rk"

    server.respond(200, {"dataset_name": "d"})
    cd.get("d")
    assert server.last.path == "/3.0/projects/proj1/datasets/d"

    query = {"analysis_type": "count", "event_collection": "c", "timeframe": "this_30_days",
             "interval": "daily"}
    server.respond(201, {})
    cd.create("d", query, ["user.id"], "Daily")
    req = server.last
    assert (req.method, req.path) == ("PUT", "/3.0/projects/proj1/datasets/d")
    assert req.headers["Authorization"] == "mk"
    assert req.json() == {"query": query, "index_by": ["user.id"], "display_name": "Daily"}

    server.respond(200, {"result": []})
    tf = {"start": "2026-01-01T00:00:00Z", "end": "2026-01-02T00:00:00Z"}
    assert cd.results("d", "u1", tf) == {"result": []}
    req = server.last
    assert req.path == "/3.0/projects/proj1/datasets/d/results"
    assert req.query["index_by"] == "u1"
    assert req.query_json("timeframe") == tf
    server.respond(200, {"result": []})
    cd.results("d", ["u1", "u2"], "this_2_days")
    assert server.last.query_json("index_by") == ["u1", "u2"]
    assert server.last.query["timeframe"] == "this_2_days"

    server.respond(204, None)
    assert cd.delete("d") is True
    assert server.last.method == "DELETE"
