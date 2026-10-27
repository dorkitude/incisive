# incisive (Ruby)

Ruby client for an Incisive analytics server. Its only dependency is the Ruby standard
library (`net/http`, `json`, `openssl`).

```ruby
gem "incisive", path: "sdks/ruby"
```

## Configuration

```ruby
require "incisive"

client = Incisive::Client.new(
  project_id: "my-project",
  write_key:  "...",
  read_key:   "...",
  master_key: "...",
  api_url:    "http://localhost:8080", # the default; `base_url:` works too
  api_version: "3.0",                   # the default
  read_timeout: 300, open_timeout: 30,  # seconds (Net::HTTP defaults otherwise)
  proxy_url: "http://proxy:3128"        # optional
)
```

The module-level API (`Incisive.publish`, `Incisive.count`, ...) uses a default client built
from the environment:

| variable | option |
|---|---|
| `INCISIVE_PROJECT_ID` | `project_id` |
| `INCISIVE_WRITE_KEY` / `INCISIVE_READ_KEY` / `INCISIVE_MASTER_KEY` | keys |
| `INCISIVE_BASE_URL` (or `INCISIVE_API_URL`) | `api_url` |
| `INCISIVE_API_VERSION` | `api_version` |
| `INCISIVE_READ_TIMEOUT` / `INCISIVE_OPEN_TIMEOUT` | timeouts |
| `INCISIVE_PROXY_URL` / `INCISIVE_PROXY_TYPE` | proxy |

You can also set attributes directly (`Incisive.project_id = "..."`), or build the default client
yourself with `Incisive.default_client = Incisive::Client.new(...)`.

## Operations

```ruby
# Publishing (write key)
Incisive.publish(:signups, username: "lloyd", keen: { timestamp: "2026-01-01T00:00:00Z" })
Incisive.publish_batch(signups: [{ n: 1 }, { n: 2 }], purchases: [{ price: 5 }])
t = Incisive.publish_async(:signups, n: 3)   # runs on a Thread; t.value -> result or raises
Incisive.beacon_url(:opens, recipient: "a@b.com")          # GET URL with api_key + base64 data
Incisive.redirect_url(:clicks, { id: 1 }, "https://example.com/")

# Queries (read key). The return value is the `result` field.
Incisive.count(:purchases, timeframe: "this_7_days", group_by: "country")
Incisive.count_unique(:purchases, target_property: "user.id")
Incisive.sum / average / minimum / maximum / median / standard_deviation(:c, target_property: "price")
Incisive.percentile(:purchases, target_property: "price", percentile: 90)
Incisive.select_unique(:purchases, target_property: "country")
Incisive.extraction(:purchases, timeframe: "this_1_days", latest: 10, property_names: ["price"])
Incisive.funnel(steps: [{ event_collection: "a", actor_property: "user" }, ...])
Incisive.multi_analysis(:purchases, analyses: { total: { analysis_type: "sum", target_property: "price" } })
Incisive.query(:count, :purchases, { timeframe: "this_2_days" })
Incisive.query_url(:count, :purchases, { timeframe: "this_2_days" })  # adds &api_key=<read key>

# Query options (third argument)
Incisive.count(:c, { timeframe: "this_1_days" }, method: :post)      # JSON body instead of GET
Incisive.count(:c, {}, response: :all_keys)                           # full response, not just result
Incisive.count(:c, {}, headers: { "X-Trace" => "1" })

# Maintenance (master key)
Incisive.delete(:purchases)                                   # drop the collection
Incisive.delete(:purchases, filters: [...], timeframe: "...") # delete matching events
Incisive.event_collections
Incisive.event_collection(:purchases)
Incisive.project_info

# Saved queries
Incisive.saved_queries.all
Incisive.saved_queries.get("name")          # definition (master key)
Incisive.saved_queries.get("name", true)    # results (read key)
Incisive.saved_queries.create("name", query: { analysis_type: "count", event_collection: "c" })
Incisive.saved_queries.update("name", refresh_rate: 3600)   # GET, merge, PUT
Incisive.saved_queries.cache("name", 3600) / uncache("name") / delete("name")

# Cached datasets
Incisive.cached_datasets.create("ds", "user.id", query_hash, "Display name")
Incisive.cached_datasets.get_results("ds", { start: "...", end: "..." }, "user.id")
Incisive.cached_datasets.list(limit: 5, after_name: "x")
Incisive.cached_datasets.get_definition("ds") / delete("ds")

# Access keys (master key)
Incisive.access_keys.create(name: "reader", is_active: true, permitted: ["queries"], options: {})
Incisive.access_keys.all / get(key) / update(key, body) / revoke(key) / unrevoke(key) / delete(key)

# Scoped keys (AES-256-CBC with the master key, hex encoded)
scoped = Incisive::ScopedKey.new(master_key, { "allowed_operations" => ["read"] }).encrypt!
Incisive::ScopedKey.decrypt!(master_key, scoped).data
```

## Wire format

- Auth uses the `Authorization: <key>` header (no scheme). Generated URLs use `api_key=`.
- Every request sends `Content-Type: application/json` and `User-Agent: incisive-ruby/<version>`.
- For GET queries, `filters`, `steps` and `analyses`, along with any Hash or Array value
  (`timeframe`, `group_by`, `property_names`), are sent as JSON strings. Other values are
  stringified. Values are form-escaped (a space becomes `+`), and `nil` values are dropped.
  `event_collection` is added as a parameter.
- Collection, saved query, dataset and key names are percent-encoded as single path segments.
- Responses: 2xx returns the parsed JSON (and a query returns only its `result`). 204 returns `true`.
  400, 401 and 404 raise `Incisive::BadRequestError`, `AuthenticationError` and `NotFoundError`.
  Any other status raises `Incisive::HttpError`. HTTP errors expose `status`, `body`,
  `error_code` and `api_message` (taken from `{"message", "error_code"}`). `message` is the raw
  body. A transport failure raises `HttpError` with `original_error` set and no `status`.
  A missing project id or key raises `Incisive::ConfigurationError`.

## Tests

```sh
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -e BUNDLE_PATH=/tmp/bundle \
  -v "$PWD":/w -w /w ruby:3.3.6 bash -lc 'bundle install && bundle exec rake test'
```

Unit tests run against an in-process mock HTTP server. Integration tests run only when
`INCISIVE_BASE_URL` and `INCISIVE_PROJECT_ID` are set, plus the keys they need. Add
`--network host` to reach a server on the host.
