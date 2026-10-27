#!/usr/bin/env bash
# Runs the unmodified, pinned upstream JavaScript client packages against a running server.
# The analysis client only speaks HTTPS on port 443, so a throwaway Caddy container with an
# internal CA terminates TLS for incisive.test and proxies to the server.
#
#   tests/compat/run.sh http://127.0.0.1:8081 PROJECT_ID WRITE_KEY READ_KEY MASTER_KEY
#
# Needs Docker and free host port 443. Use a fresh project: the suite writes events.
set -euo pipefail
upstream=${1#http://}
project=$2 write=$3 read=$4 master=$5
dir=$(cd "$(dirname "$0")" && pwd)
caddyfile=$(mktemp)
printf '{\n  admin off\n  http_port 18080\n}\nincisive.test {\n  tls internal\n  reverse_proxy %s\n}\n' "$upstream" >"$caddyfile"
docker rm -f incisive-compat-tls >/dev/null 2>&1 || true
docker run -d --name incisive-compat-tls --network host -v "$caddyfile:/etc/caddy/Caddyfile:ro" caddy:2.8 >/dev/null
trap 'docker rm -f incisive-compat-tls >/dev/null; rm -f "$caddyfile"' EXIT
timeout 30 bash -c 'until curl -sk --resolve incisive.test:443:127.0.0.1 https://incisive.test/health >/dev/null; do sleep 1; done'
docker run --rm --network host --add-host incisive.test:127.0.0.1 -u "$(id -u):$(id -g)" -e HOME=/tmp \
  -e NODE_TLS_REJECT_UNAUTHORIZED=0 -e COMPAT_HOST=incisive.test \
  -e COMPAT_BASE_URL=https://incisive.test/3.0 -e COMPAT_HTTP_BASE_URL="http://$upstream/3.0" \
  -e COMPAT_PROJECT_ID="$project" -e COMPAT_WRITE_KEY="$write" -e COMPAT_READ_KEY="$read" -e COMPAT_MASTER_KEY="$master" \
  -v "$dir/js:/w" -w /w node:22.11.0-bookworm-slim \
  sh -c 'npm ci --ignore-scripts --no-audit --no-fund >/dev/null && node --test --test-timeout=60000 test.cjs'
