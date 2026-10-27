"""Opt-in tests against a real server.

Run with INCISIVE_BASE_URL, INCISIVE_PROJECT_ID and INCISIVE_WRITE_KEY / INCISIVE_READ_KEY
(and INCISIVE_MASTER_KEY for the delete test) set.
"""

import os
import time
import uuid

import pytest

import incisive

ENV = {k: os.environ.get("INCISIVE_" + k.upper()) for k in
       ("base_url", "project_id", "write_key", "read_key", "master_key")}

pytestmark = pytest.mark.skipif(
    not (ENV["base_url"] and ENV["project_id"] and ENV["write_key"] and ENV["read_key"]),
    reason="set INCISIVE_BASE_URL, INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY, INCISIVE_READ_KEY",
)


@pytest.fixture
def live():
    return incisive.IncisiveClient(**ENV)


def _eventually(fn, want, timeout=15.0):
    deadline = time.time() + timeout
    while True:
        got = fn()
        if got == want or time.time() > deadline:
            return got
        time.sleep(0.5)


def test_add_event_then_count(live):
    collection = "sdk_python_" + uuid.uuid4().hex[:10]
    run = uuid.uuid4().hex
    live.add_event(collection, {"run": run, "price": 3})
    live.add_events({collection: [{"run": run, "price": 4}, {"run": run, "price": 5}]})
    filters = [{"property_name": "run", "operator": "eq", "property_value": run}]
    got = _eventually(lambda: live.count(collection, timeframe="this_1_days", filters=filters), 3)
    assert got == 3
    assert live.sum(collection, "price", timeframe="this_1_days", filters=filters) == 12
    if ENV["master_key"]:
        assert live.delete_events(collection, filters=filters) is True


def test_bad_key_raises(live):
    bad = incisive.IncisiveClient(ENV["project_id"], read_key="definitely-not-a-key",
                                  base_url=ENV["base_url"])
    with pytest.raises(incisive.IncisiveApiError) as info:
        bad.count("anything", timeframe="this_1_days")
    assert info.value.status_code in (401, 403)
