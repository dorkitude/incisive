#!/usr/bin/env bash
# Uploads src/web/tracker.js to a public static bucket as js/incisive.js (short cache)
# and js/incisive-VERSION.js (immutable), for serving the tracker from a CDN instead of the server.
# Usage: BUCKET=my-static-bucket PROFILE=my-aws-profile [ENDPOINT=https://s3-endpoint] scripts/publish_tracker.sh
set -euo pipefail
cd "$(dirname "$0")/.."
: "${BUCKET:?set BUCKET to a public bucket}" "${PROFILE:?set PROFILE to an AWS profile that can write to it}"
endpoint=()
if [ -n "${ENDPOINT:-}" ]; then endpoint=(--endpoint-url "$ENDPOINT"); fi
version=$(grep -o 'VERSION = "[0-9.]*"' src/web/tracker.js | grep -o '[0-9.]*')
ct="application/javascript; charset=utf-8"
up() { aws s3 cp src/web/tracker.js "s3://$BUCKET/js/$1" --profile "$PROFILE" "${endpoint[@]}" --content-type "$ct" --cache-control "$2" --only-show-errors; }
up "incisive-$version.js" "public, max-age=31536000, immutable"
up incisive.js "public, max-age=300"
echo "published tracker $version"
