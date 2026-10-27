import pytest

import incisive
from incisive import direction

FILTERS = [{"property_name": "device", "operator": "eq", "property_value": "iPhone"}]


def check_query(server, analysis):
    req = server.last
    assert req.method == "GET"
    assert req.path == "/1.0/projects/proj1/queries/" + analysis
    assert req.headers["Authorization"] == "rk"
    assert req.body == b""
    return req


def test_count_full_params(server, client):
    server.respond(200, {"result": [{"device": "x", "result": 3}]})
    res = client.count("purchases", timeframe="this_7_days", timezone="US/Pacific",
                       interval="daily", filters=FILTERS, group_by="device",
                       order_by={"property_name": "result", "direction": direction.DESCENDING},
                       max_age=300, limit=5)
    assert res == [{"device": "x", "result": 3}]
    req = check_query(server, "count")
    q = req.query
    assert q["event_collection"] == "purchases"
    assert q["timeframe"] == "this_7_days"
    assert q["timezone"] == "US/Pacific"
    assert q["interval"] == "daily"
    assert req.query_json("filters") == FILTERS
    assert q["group_by"] == "device"
    assert req.query_json("order_by") == [{"property_name": "result", "direction": "DESC"}]
    assert q["max_age"] == "300"
    assert q["limit"] == "5"
    assert set(q) == {"event_collection", "timeframe", "timezone", "interval", "filters",
                      "group_by", "order_by", "max_age", "limit"}


def test_structured_timeframe_and_group_by_list(server, client):
    server.respond(200, {"result": 1})
    tf = {"start": "2026-01-01T00:00:00Z", "end": "2026-02-01T00:00:00Z"}
    client.count("c", timeframe=tf, timezone=-28800, group_by=["a", "b"])
    req = server.last
    assert req.query_json("timeframe") == tf
    assert req.query_json("group_by") == ["a", "b"]
    assert req.query["timezone"] == "-28800"


def test_space_is_form_encoded(server, client):
    server.respond(200, {"result": 1})
    client.count("my coll")
    assert "event_collection=my+coll" in server.last.raw_query


@pytest.mark.parametrize("name", ["sum", "minimum", "maximum", "average", "median",
                                  "count_unique", "select_unique"])
def test_target_property_analyses(server, client, name):
    server.respond(200, {"result": 42})
    res = getattr(client, name)("c", "price", timeframe="this_1_days", group_by="g",
                                order_by=[{"property_name": "result"}], limit=2)
    assert res == 42
    req = check_query(server, name)
    assert req.query["target_property"] == "price"
    assert req.query["event_collection"] == "c"
    assert req.query_json("order_by") == [{"property_name": "result"}]
    assert req.query["limit"] == "2"


def test_percentile(server, client):
    server.respond(200, {"result": 9.5})
    assert client.percentile("c", "price", 99.5, timeframe="this_1_days") == 9.5
    req = check_query(server, "percentile")
    assert req.query["percentile"] == "99.5"


def test_extraction(server, client):
    server.respond(200, {"result": [{"a": 1}]})
    assert client.extraction("c", timeframe="this_1_days", latest=10,
                             property_names=["a", "keen.timestamp"]) == [{"a": 1}]
    req = check_query(server, "extraction")
    assert req.query["latest"] == "10"
    assert req.query_json("property_names") == ["a", "keen.timestamp"]


def test_email_extraction(server, client):
    server.respond(200, {"result": "Processing. Check the specified email for the extraction results."})
    client.extraction("c", timeframe="this_1_days", email="a@b.c")
    assert server.last.query["email"] == "a@b.c"


def test_funnel(server, client):
    steps = [{"event_collection": "signup", "actor_property": "user.id"},
             {"event_collection": "purchase", "actor_property": "user.id"}]
    body = {"result": [3, 1], "steps": steps}
    server.respond(200, body)
    assert client.funnel(steps, timeframe="this_7_days") == [3, 1]
    req = check_query(server, "funnel")
    assert req.query_json("steps") == steps
    assert "event_collection" not in req.query
    server.respond(200, body)
    assert client.funnel(steps, all_keys=True) == body


def test_multi_analysis(server, client):
    analyses = {"total": {"analysis_type": "sum", "target_property": "price"},
                "n": {"analysis_type": "count"}}
    server.respond(200, {"result": {"total": 10, "n": 2}})
    assert client.multi_analysis("c", analyses, timeframe="this_1_days") == {"total": 10, "n": 2}
    req = check_query(server, "multi_analysis")
    assert req.query_json("analyses") == analyses


def test_falsy_params_omitted(server, client):
    server.respond(200, {"result": 0})
    client.count("c", timeframe=None, filters=[], limit=0)
    assert set(server.last.query) == {"event_collection"}


@pytest.mark.parametrize("kwargs", [
    dict(order_by={"property_name": "result"}),  # no group_by
    dict(group_by="g", order_by={"direction": "ASC"}),
    dict(group_by="g", order_by={"property_name": "r", "direction": "UP"}),
    dict(group_by="g", order_by={"property_name": "r", "extra": 1}),
    dict(limit=3),  # no order_by
    dict(group_by="g", order_by={"property_name": "r"}, limit=-1),
])
def test_client_side_validation(server, client, kwargs):
    with pytest.raises(ValueError):
        client.count("c", **kwargs)
    assert server.requests == []


def test_read_key_required_even_with_master(server):
    c = incisive.IncisiveClient("p", master_key="mk", base_url=server.url)
    with pytest.raises(incisive.InvalidEnvironmentError, match="least privilege"):
        c.count("c")
