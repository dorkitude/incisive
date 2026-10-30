# Self-hosting Incisive

Incisive runs anywhere a Linux binary does. A server needs four things: the binary, a bucket (or
a local directory) for data, HTTPS in front, and an email provider for sign-in codes. Everything
below has a ready-made example in [`deploy/`](../deploy/).

## 1. Build and install

```bash
cargo build --release
sudo install -m755 target/release/incisive /usr/local/bin/incisive
sudo useradd --system --home /var/lib/incisive --create-home incisive
sudo mkdir -p /etc/incisive
sudo cp deploy/incisive.example.toml /etc/incisive/incisive.toml   # then edit it
```

## 2. Choose where data lives

Any S3-compatible bucket works: AWS S3, Cloudflare R2, MinIO, Backblaze B2, Wasabi and others.
Give Incisive its own private bucket (or its own `prefix` in a shared one) and credentials that
can read, write, list and delete in it, nothing more.

```toml
[storage]
backend = "s3"
bucket = "my-incisive-data"
endpoint = "https://<your S3 endpoint>"   # leave out for AWS S3
region = "auto"                            # your bucket's region on AWS
profile = "incisive"                       # a profile in ~/.aws/credentials
```

Credentials come from `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` or the named profile, and are
never written to Incisive's config. For a single machine you can also keep data on local disk
with `backend = "fs"` and `fs_path`, but then backing up that directory is up to you.

One Incisive server writes to a given bucket prefix. To run several, give each its own `prefix`.

## 3. Put HTTPS in front

[Caddy](https://caddyserver.com) gets and renews certificates automatically. With your DNS
pointing at the machine, [`deploy/Caddyfile`](../deploy/Caddyfile) is all it takes:

```
analytics.example.com {
	reverse_proxy 127.0.0.1:8080
}
```

Then tell Incisive its public address, and that it sits behind a proxy:

```toml
[server]
addr = "127.0.0.1:8080"
public_url = "https://analytics.example.com"
trust_forwarded_for = true   # client IPs come from the proxy's X-Forwarded-For
secure_cookies = true
```

Only enable `trust_forwarded_for` when the port isn't reachable except through your proxy, or
visitors could fake their address.

## 4. Send sign-in emails

People sign in with a 6-digit code sent by email. Incisive speaks the
[Resend](https://resend.com) API; any provider or relay with a compatible `POST /emails` works.

```toml
[email]
resend_url = "https://api.resend.com"
api_key = ""                   # or set INCISIVE_EMAIL__API_KEY in the environment
from = "Incisive <login@example.com>"
log_only = false
```

With `log_only = true` (the default), codes are printed to the log instead, which is handy on
your own machine.

## 5. Run it as a service

[`deploy/incisive.service`](../deploy/incisive.service) is a systemd unit:

```bash
sudo cp deploy/incisive.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now incisive
journalctl -u incisive -f
```

Sign in at your `public_url`. Your first sign-in creates your account and a first project.

## Optional: restrict new accounts

Set `INCISIVE_SIGNUP_ALLOWED_EMAIL_DOMAINS=example.com,example.org` in the service
environment, or `signup_allowed_email_domains = "example.com,example.org"` at the top
of the TOML file. The default empty value keeps signups open. Domains are trimmed,
case-insensitive and matched exactly; subdomains are separate domains.

The restriction applies to email-code signup before any code is stored or email sent,
and to all user creation, including CLI commands with `--owner`. Existing accounts
can still sign in. CLI commands must use the same configuration/environment as the service.
Public event ingestion is unaffected.

## Optional: visitor locations

Give Incisive a MaxMind-format city database and every event with an IP address gets country,
region, city and coordinates. [DB-IP City Lite](https://db-ip.com/db/download/ip-to-city-lite)
is free under CC BY 4.0 (the dashboard shows the credit its license asks for).
`scripts/update_geo.sh` downloads the current month's file, and
[`deploy/incisive-geo.timer`](../deploy/incisive-geo.timer) refreshes it monthly:

```bash
sudo install -m755 scripts/update_geo.sh /usr/local/bin/incisive-update-geo
sudo mkdir -p /var/lib/incisive/geo && sudo incisive-update-geo /var/lib/incisive/geo/city.mmdb
sudo cp deploy/incisive-geo.service deploy/incisive-geo.timer /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now incisive-geo.timer
```

```toml
[geo]
db_path = "/var/lib/incisive/geo/city.mmdb"
```

## Backups and moving machines

Events live in your bucket already. Accounts, projects, keys and saved queries live in a SQLite
file (`meta.path`) that Incisive copies to the bucket every five minutes (`backup_interval_secs`),
keeping one copy per day under `backups/`. On a new machine with an empty disk, Incisive restores
the latest copy automatically at startup. `incisive backup` takes one on demand.

The local disk cache (`cache.dir`) only holds copies of bucket data; it is safe to delete while
Incisive is stopped.

## Upgrading

Build the new version, replace the binary and restart. Data written by older versions is upgraded
automatically at startup when needed.

## Configuration reference

Configuration comes from defaults, then the TOML file (`--config`, or `./incisive.toml`), then
`INCISIVE_*` environment variables, with `__` between section and key (for example
`INCISIVE_STORAGE__BUCKET`). `incisive config` prints the effective configuration.

- `server`: `addr`, `public_url` (used in emails and snippets), `trust_forwarded_for`,
  `secure_cookies`, `redirect_allow_hosts` (hosts that tracking links may redirect to), and
  `tracker_url` (serve the website tracker from somewhere else, such as a CDN).
- `storage`: `backend` (`s3` or `fs`), `bucket`, `endpoint`, `region`, `profile`, `prefix`,
  `fs_path`.
- `cache`: disk cache `dir` and `max_disk_bytes`.
- `wal`, `compaction`: how long writes are grouped, and when they are compacted.
- `meta`: SQLite `path` and `backup_interval_secs`.
- `email`: `resend_url`, `api_key`, `from`, `log_only`.
- `limits`: event and request sizes, batch size, per-project write and query rates, query
  concurrency and timeout, and caps on extractions, funnel actors, groups, intervals and scans.
- `geo`: `db_path`.

Other commands:

- `incisive create-project NAME [--owner EMAIL]` creates a project and prints its keys.
- `incisive create-account-key NAME --owner EMAIL` prints an account master key, which manages
  all of that person's projects through the API.
- `incisive compact` compacts all pending writes now.
- `incisive backup` backs up accounts and projects now.
