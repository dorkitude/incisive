# HTTP API

Project routes are under `/1.0/projects/{project_id}`, account routes under `/1.0/account`. Send
the key in the `Authorization` header (or as `api_key`).

| Route | |
|---|---|
| `POST /events/{collection}` | one event (`201 {"created": true}`); a JSON list is treated as a batch |
| `POST /events` | batch: `{collection: [events]}`, answered per event |
| `GET /events/{collection}?data=BASE64` | write from a URL: answers JSON, a 1x1 GIF (`c=` or an image `Accept`), JSONP (`jsonp`/`callback`), or a redirect to an allowed host (`redirect`) |
| `GET /events`, `GET /events/{collection}`, `GET /events/{c}/properties/{p}` | schema discovery |
| `DELETE /events/{collection}` | delete by `filters`, `timeframe`, `timezone`; any other parameter is rejected so a misspelled filter cannot delete everything |
| `GET\|POST /queries/{analysis}` | `count`, `count_unique`, `sum`, `average`, `minimum`, `maximum`, `median`, `percentile`, `select_unique`, `extraction`, `funnel`, `multi_analysis` |
| `/queries/saved[/{name}[/result]]` | saved queries (also under `/queries/cached`) |
| `/keys[/{key}[/revoke\|/unrevoke]]` | key management |
| `GET /`, `POST /` | project details; `POST {"name": ...}` renames (master key) |
| `/datasets` | not supported (501) |

Events carry a reserved metadata object, `incisive`, holding `timestamp` (defaults to receipt
time), `created_at`, `id` and `addons`: data enrichment add-ons (`incisive:url_parser`,
`incisive:referrer_parser`, `incisive:ua_parser`, `incisive:date_time_parser`,
`incisive:ip_to_geo`), each `{"name", "input": {...}, "output": "path"}`. A string value equal to
`${incisive.ip}` or `${incisive.user_agent}` is replaced by the client's IP or user agent.

For Keen client libraries, Keen's spellings are accepted everywhere: a `keen` object on events,
`keen:` add-on names, `keen.` paths in add-ons and queries, and `${keen.ip}`. They are stored and
returned as `incisive`. Data written before the rename is upgraded once at startup (marker object
`migrations/reserved-incisive`).

## Keys and permissions

Keys look like `sk_incisive_` followed by 20 characters, and are stored hashed. Each project starts
with three:

- **Master**: everything, including deletes, saved query changes and key management. Its secret
  is retained so it can verify legacy encrypted scoped keys minted by client libraries.
- **Read**: `queries`, `saved_queries`, `cached_queries`, `schema`.
- **Write**: `writes`. Write-only keys are public by nature (they ship in web pages), so their
  secret stays visible in the app.

Custom keys combine `writes`, `queries`, `saved_queries`, `cached_queries`, `schema` and
`datasets`, with options: `writes.autofill` (properties stamped onto every event, overriding the
client), `queries.filters` (added to every query, for per-customer dashboards), and
`saved_queries`/`cached_queries` `allowed`/`blocked` name lists.

## Account master keys

Account master keys look like `ak_incisive_` followed by 32 characters. Each belongs to a user,
is stored hashed and is shown only once, at creation (web app: **Account keys**, or the
`create-account-key` command). It works as the master key of every project its user belongs to,
including projects created later, and it can use the account routes:

| Route | |
|---|---|
| `GET /1.0/account` | the user's email and projects |
| `GET /1.0/account/projects` | list projects |
| `POST /1.0/account/projects` | `{"name": ...}` creates a project; answers with its master, read and write keys |
| `GET\|POST /1.0/account/keys` | list account keys (no secrets) or create one (`{"name": ...}`) |
| `DELETE /1.0/account/keys/{id}` | delete an account key |

Project keys never work on account routes, and an account key gets nothing on projects its user
does not belong to.
