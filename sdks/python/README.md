# incisive (Python)

Python client for the incisive analytics server (`/1.0/projects/{project_id}/...` HTTP API).
It uses `requests` and covers event recording, analyses, saved queries, cached datasets,
schema discovery, deletes, access keys and scoped keys.

## Install

```bash
uv add incisive            # or: uv pip install ./sdks/python
```

## Configure

Client object:

```python
from incisive import IncisiveClient

client = IncisiveClient(
    project_id="my-project",
    write_key="...", read_key="...", master_key="...",
    base_url="http://localhost:8080",   # default
    api_version="1.0",                  # default
    get_timeout=305, post_timeout=305,  # seconds
)
```

Module-level shared client, configured by attributes or environment variables
(`INCISIVE_PROJECT_ID`, `INCISIVE_WRITE_KEY`, `INCISIVE_READ_KEY`, `INCISIVE_MASTER_KEY`,
`INCISIVE_BASE_URL`). Attributes take precedence. The client is built on first use, so if you
change the settings later, set `incisive._client = None` and it is built again.

```python
import incisive

incisive.project_id = "my-project"
incisive.write_key = "..."
incisive.read_key = "..."

incisive.add_event("purchases", {"price": 10, "user": {"id": 7}})
incisive.count("purchases", timeframe="this_7_days", group_by="user.id")
```

## Operations

Recording (write key):
- `add_event(collection, body, timestamp=None)` sends POST `/events/{collection}`. A `timestamp`
  (datetime) is written into the reserved metadata object as `timestamp`.
- `add_events({collection: [event, ...]})` sends POST `/events` and returns the per-event result body.
- `generate_image_beacon(collection, body)` returns a GET URL with `api_key` and base64 `data`.

Analyses (read key): each sends GET `/queries/{type}` and returns `result`:
`count`, `count_unique`, `sum`, `minimum`, `maximum`, `average`, `median`,
`percentile(collection, target_property, percentile, ...)`, `select_unique`,
`extraction(collection, timeframe, timezone, filters, latest, email, property_names)`,
`funnel(steps, timeframe, timezone, max_age, all_keys=False)` (with `all_keys=True` you get
the whole body) and `multi_analysis(collection, analyses, ...)`.
Common kwargs are `timeframe`, `timezone`, `interval`, `filters`, `group_by`, `order_by`,
`max_age` and `limit`. `order_by` needs `group_by`, and `limit` needs `order_by`. Both are
checked on the client and raise `ValueError`. For directions, use `incisive.direction.ASCENDING` /
`DESCENDING`.

Discovery (read key): `get_collection(name)` and `get_all_collections()`.

Deletes (master key): `delete_events(collection, timeframe=None, timezone=None, filters=None)`.
If you pass no filters and no timeframe, the whole collection is deleted.

Access keys (master key): `create_access_key`, `list_access_keys`, `get_access_key`,
`update_access_key_full`, `update_access_key_name`, `add_access_key_permissions`,
`remove_access_key_permissions`, `update_access_key_permissions`,
`update_access_key_options`, `revoke_access_key`, `unrevoke_access_key` and
`delete_access_key`. The partial updates first GET the key, then POST the full definition.

Saved queries, under `client.saved_queries`: `all()`, `get(name)` and `delete(name)` use the
master key. `results(name)` uses the read key. `create(name, definition)` and
`update_full(name, definition)` send a PUT. `update(name, attrs)` does a GET, deep-merges
`attrs`, then sends a PUT.

Cached datasets, under `client.cached_datasets`: `all()`, `get(name)` and
`results(name, index_by, timeframe)` use the read key. `create(name, query, index_by, display_name)`
and `delete(name)` use the master key.

Scoped keys:

```python
from incisive import scoped_keys
key = scoped_keys.encrypt(master_key, {"allowed_operations": ["read"], "filters": [...]})  # bytes
scoped_keys.decrypt(master_key, key)  # -> dict
```

For a 64-hex-char master key this uses AES-256-CBC. Any other key uses the legacy key-padding
mode. The output is hex(IV) + hex(ciphertext).

## Wire format

- Every request sends `Authorization: <key>` (the raw key, no scheme),
  `Content-Type: application/json` and `User-Agent: incisive-python/<version>`.
- Path segments (collection, key, query and dataset names) are percent-escaped, `/` included.
- Query parameters are form-encoded. `filters`, `steps`, `analyses` and `property_names` are
  JSON. `timeframe` and `group_by` are JSON when you pass a dict or list. `order_by` is always
  a JSON array. Falsy values are left out.
- A non-2xx response raises `incisive.IncisiveApiError` with these fields: `.status_code`,
  `.message`, `.error_code` and `.api_error` (the decoded body). If the body is not JSON,
  `error_code` holds the status code.
- Other exceptions: `InvalidProjectIdError`, `InvalidEnvironmentError` (a required key is
  missing) and `InvalidPersistenceStrategyError`. All of them subclass
  `BaseIncisiveClientError`.

Custom persistence: pass `persistence_strategy=` with a subclass of
`incisive.persistence_strategies.BasePersistenceStrategy`. The default is
`DirectPersistenceStrategy`, which sends synchronous HTTP.

## Tests

```bash
cd sdks/python
uv run pytest
```

The unit tests run against an in-process mock HTTP server. The integration tests in
`tests/test_integration.py` run only if `INCISIVE_BASE_URL`, `INCISIVE_PROJECT_ID`,
`INCISIVE_WRITE_KEY` and `INCISIVE_READ_KEY` are set. Set `INCISIVE_MASTER_KEY` as well to
run the cleanup delete.
