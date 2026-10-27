import pytest

import incisive


def test_json_error(server, client):
    server.respond(404, {"message": "Collection not found", "error_code": "ResourceNotFoundError",
                         "unique_id": "u-1"})
    with pytest.raises(incisive.IncisiveApiError) as info:
        client.count("missing")
    err = info.value
    assert err.status_code == 404
    assert err.message == "Collection not found"
    assert err.error_code == "ResourceNotFoundError"
    assert err.api_error["unique_id"] == "u-1"
    assert "Collection not found" in str(err) and "Unique ID: u-1" in str(err)
    assert isinstance(err, incisive.BaseIncisiveClientError)


def test_non_json_error(server, client):
    server.respond(502, "<html>bad gateway</html>", content_type="text/html")
    with pytest.raises(incisive.IncisiveApiError) as info:
        client.add_event("c", {})
    err = info.value
    assert err.status_code == 502
    assert err.error_code == "502"
    assert "did not respond with JSON" in err.message and "bad gateway" in err.message


@pytest.mark.parametrize("call", [
    lambda c: c.add_events({"c": [{}]}),
    lambda c: c.delete_events("c"),
    lambda c: c.get_all_collections(),
    lambda c: c.list_access_keys(),
    lambda c: c.saved_queries.all(),
    lambda c: c.cached_datasets.get("d"),
])
def test_errors_raise_everywhere(server, client, call):
    server.respond(401, {"message": "bad key", "error_code": "InvalidApiKeyError"})
    with pytest.raises(incisive.IncisiveApiError) as info:
        call(client)
    assert info.value.status_code == 401
    assert info.value.error_code == "InvalidApiKeyError"


def test_error_missing_fields(server, client):
    server.respond(400, {"oops": True})
    with pytest.raises(incisive.IncisiveApiError) as info:
        client.count("c")
    assert info.value.message is None
    assert info.value.api_error == {"oops": True}
