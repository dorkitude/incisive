"""Module-level shared-client API."""

import pytest

import incisive


def test_requires_project_id(reset_module):
    with pytest.raises(incisive.InvalidEnvironmentError, match="INCISIVE_PROJECT_ID"):
        incisive.add_event("c", {})


def test_from_environment(reset_module, monkeypatch, server):
    monkeypatch.setenv("INCISIVE_PROJECT_ID", "envproj")
    monkeypatch.setenv("INCISIVE_WRITE_KEY", "ewk")
    monkeypatch.setenv("INCISIVE_READ_KEY", "erk")
    monkeypatch.setenv("INCISIVE_MASTER_KEY", "emk")
    monkeypatch.setenv("INCISIVE_BASE_URL", server.url)
    incisive.add_event("c", {"a": 1})
    assert server.last.path == "/1.0/projects/envproj/events/c"
    assert server.last.headers["Authorization"] == "ewk"
    assert (incisive.project_id, incisive.read_key, incisive.master_key) == ("envproj", "erk", "emk")


def test_globals_win_over_environment(reset_module, monkeypatch, server):
    monkeypatch.setenv("INCISIVE_PROJECT_ID", "envproj")
    monkeypatch.setenv("INCISIVE_READ_KEY", "erk")
    incisive.project_id = "globalproj"
    incisive.read_key = "grk"
    incisive.base_url = server.url
    server.respond(200, {"result": 7})
    assert incisive.count("c", timeframe="this_1_days") == 7
    assert server.last.path == "/1.0/projects/globalproj/queries/count"
    assert server.last.headers["Authorization"] == "grk"


@pytest.mark.parametrize("name", ["sum", "minimum", "maximum", "average", "median",
                                  "count_unique", "select_unique"])
def test_targeted_shortcuts_forward_everything(reset_module, server, name):
    incisive.project_id, incisive.read_key, incisive.base_url = "p", "rk", server.url
    server.respond(200, {"result": 1})
    getattr(incisive, name)("c", "t", group_by="g", order_by={"property_name": "result"}, limit=3)
    q = server.last.query
    assert server.last.path == "/1.0/projects/p/queries/" + name
    assert (q["target_property"], q["group_by"], q["limit"]) == ("t", "g", "3")


def test_other_shortcuts(reset_module, server):
    incisive.project_id, incisive.read_key, incisive.write_key = "p", "rk", "wk"
    incisive.master_key, incisive.base_url = "mk", server.url
    server.respond(200, {"result": 2.5})
    assert incisive.percentile("c", "t", 50) == 2.5
    server.respond(200, {"result": []})
    incisive.extraction("c", latest=1)
    server.respond(200, {"result": [1]})
    incisive.funnel([{"event_collection": "c", "actor_property": "u"}])
    server.respond(200, {"result": {}})
    incisive.multi_analysis("c", {"n": {"analysis_type": "count"}})
    server.respond(200, {"c": [{"success": True}]})
    assert incisive.add_events({"c": [{}]}) == {"c": [{"success": True}]}
    server.respond(204, None)
    assert incisive.delete_events("c") is True
    server.respond(200, [])
    incisive.get_all_collections()
    server.respond(200, {})
    incisive.get_collection("c")
    server.respond(200, [])
    incisive.list_access_keys()
    paths = [(r.method, r.path) for r in server.requests]
    assert paths == [
        ("GET", "/1.0/projects/p/queries/percentile"),
        ("GET", "/1.0/projects/p/queries/extraction"),
        ("GET", "/1.0/projects/p/queries/funnel"),
        ("GET", "/1.0/projects/p/queries/multi_analysis"),
        ("POST", "/1.0/projects/p/events"),
        ("DELETE", "/1.0/projects/p/events/c"),
        ("GET", "/1.0/projects/p/events"),
        ("GET", "/1.0/projects/p/events/c"),
        ("GET", "/1.0/projects/p/keys"),
    ]
    assert incisive.generate_image_beacon("c", {}).startswith(server.url)
