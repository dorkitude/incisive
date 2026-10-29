<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="src/web/brand/incisive-logo-dark.svg">
    <img src="src/web/brand/incisive-logo.svg" alt="Incisive" height="72">
  </picture>
</p>

<p align="center"><b>Event analytics you host yourself.</b><br>
Website analytics, product events and API usage, in one small server, with your data in your own bucket.</p>

---

Incisive is a single binary. Send it JSON events from a web page, a server or `curl`, then ask
questions: how many, how many unique, sums and percentiles, funnels, raw extractions, grouped by
any property and bucketed by hour, day or month. It comes with a web app, a website tracker and
client libraries for seven languages.

- **Web analytics in one script tag.** Pageviews, sessions, sources and campaigns, clicks,
  outbound links, downloads, form submissions, scroll depth and engaged time, with a dashboard
  ready the moment events arrive.
- **Any event you like.** Events are schemaless JSON. Track signups, purchases, API calls or
  messages, with whatever properties matter to you, and query them right away.
- **A query builder, event stream, pivot tables and funnels** in the web app, and the same
  analyses over a plain HTTP API for your own dashboards.
- **Keys you can hand out.** Write-only keys are safe to embed in web pages. Read keys can be
  limited to certain queries, or locked to one customer's data with filters, for embedded
  dashboards.

## Why you can trust it with your data

- **Your data stays with you.** Events are stored in a bucket you control: any S3-compatible
  storage (AWS S3, Cloudflare R2, MinIO, Backblaze B2, ...) or a local directory. Nothing is sent
  to anyone else.
- **Open formats.** Events land as compressed newline-delimited JSON and are compacted into
  Parquet files. DuckDB, pandas, Spark or anything else that reads Parquet can read your data
  directly, with or without Incisive. You are never locked in.
- **Nothing acknowledged is lost.** A write gets its `201` only after its events are stored in the
  bucket. Accounts, projects and keys are backed up to the same bucket every few minutes and
  restored automatically on a fresh machine.
- **Careful with visitors.** The tracker is first-party (your domain, your server), never sends
  form field values, can honor Do Not Track, and has an "Ignore my traffic" switch so your own
  visits don't skew your numbers.
- **Careful with secrets.** API keys are stored hashed. Sign-in uses one-time emailed codes, with
  no passwords to leak.
- **Tested.** Every analysis is checked against hand-computed answers before and after data is
  compacted, alongside tests for every write path, permission and key type, an end-to-end test of
  the website tracker, and test suites for every client library.

## Try it in two minutes

You need [Rust](https://rustup.rs). By default Incisive stores everything under `./data`, so
there is nothing else to set up.

```bash
git clone https://github.com/dorkitude/incisive && cd incisive
cargo build --release
./target/release/incisive serve
```

Open http://localhost:8080 and sign in with any email address. Until you configure an email
provider, the sign-in code is printed in the server's log instead of being sent. You'll land on a
new project with its keys and a ready-to-paste tracking snippet.

Prefer the terminal? Create a project and send it an event:

```bash
./target/release/incisive create-project "My app" --owner you@example.com
# prints the project id and its master, read and write keys

curl -X POST "http://localhost:8080/1.0/projects/$PROJECT_ID/events/signups" \
  -H "Authorization: $WRITE_KEY" -H "Content-Type: application/json" \
  -d '{"plan": "pro", "user": {"id": 42, "country": "NZ"}}'

curl "http://localhost:8080/1.0/projects/$PROJECT_ID/queries/count?event_collection=signups&group_by=plan&timeframe=this_7_days" \
  -H "Authorization: $READ_KEY"
```

Want to explore the dashboard first? `uv run scripts/seed_demo.py http://localhost:8080 $PROJECT_ID $WRITE_KEY`
fills a project with 30 days of realistic website traffic.

## Track your website

The **Install** page in the app gives you this snippet with your keys filled in. Paste it before
`</head>`:

```html
<script defer src="https://YOUR_INCISIVE_HOST/js/incisive.js"
  data-project="PROJECT_ID" data-key="WRITE_KEY" data-host="https://YOUR_INCISIVE_HOST"></script>
```

That's all. Custom events and signed-in users take one line each:

```js
incisive.track("signup", { plan: "pro" });
incisive.identify("user-42", { plan: "pro" });
```

Every event carries its full context (visitor, session, landing page, referrer, campaign and
channel, device, browser, location), so each question is a single query on a single collection.
See [docs/web-analytics.md](docs/web-analytics.md) for every option and property.

## Track from your servers

Client libraries live in [`sdks/`](sdks/): Python, Ruby, JavaScript, Go, PHP, Java and .NET. They
cover sending events, every analysis, saved queries and key management. The Go library also has
a server-side tracker with `net/http` middleware that records every API call (endpoint, status,
latency, errors, and who made it) without slowing requests down.

```python
from incisive import IncisiveClient

client = IncisiveClient(project_id="PROJECT_ID", write_key="WRITE_KEY", base_url="https://YOUR_INCISIVE_HOST")
client.add_event("purchases", {"item": {"name": "hat", "price": 25}, "customer": {"id": "c-1"}})
```

Or skip the library: it's one HTTP request. See [docs/api.md](docs/api.md) for the whole API.

## Run it for real

On a server, put Incisive behind a reverse proxy for HTTPS, point it at a bucket, and give it an
email provider so people can sign in. [docs/self-hosting.md](docs/self-hosting.md) walks
through it, with a ready-made Caddy config, systemd unit and example configuration in
[`deploy/`](deploy/).

## Learn more

- [docs/self-hosting.md](docs/self-hosting.md): configuration, storage, email, HTTPS, geolocation, backups
- [docs/web-analytics.md](docs/web-analytics.md): the website tracker and its standard schema
- [docs/api.md](docs/api.md): writing events, queries, keys and permissions
- [docs/how-it-works.md](docs/how-it-works.md): how events are stored, compacted and queried
- [Contributing and tests](docs/how-it-works.md#tests)

## License

MIT, for the server and every client library. See [LICENSE](LICENSE).
