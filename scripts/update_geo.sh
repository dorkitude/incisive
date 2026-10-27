#!/usr/bin/env bash
# Downloads this month's DB-IP City Lite database (CC BY 4.0, https://db-ip.com) and swaps it in
# atomically. Restart the server afterwards to load it. Usage: update_geo.sh /path/to/geo.mmdb
set -euo pipefail
dest=${1:?usage: update_geo.sh /path/to/geo.mmdb}
month=$(date -u +%Y-%m)
tmp=$(mktemp "$(dirname "$dest")/.geo.XXXXXX")
trap 'rm -f "$tmp" "$tmp.gz"' EXIT
# A new month's file can appear a day or two late; fall back to last month's.
for m in "$month" "$(date -u -d "$month-01 -1 month" +%Y-%m)"; do
  if curl -sfL -o "$tmp.gz" "https://download.db-ip.com/free/dbip-city-lite-$m.mmdb.gz"; then
    gunzip -c "$tmp.gz" > "$tmp"
    chmod 0644 "$tmp"
    mv "$tmp" "$dest"
    echo "installed dbip-city-lite-$m"
    exit 0
  fi
done
echo "no DB-IP database found for $month" >&2
exit 1
