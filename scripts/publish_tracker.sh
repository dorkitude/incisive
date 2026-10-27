#!/usr/bin/env bash
# Uploads src/web/tracker.js to the public static bucket as js/incisive.js (short cache)
# and js/incisive-VERSION.js (immutable). Needs an AWS profile with write access to the bucket.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${BUCKET:=incisive-static}" "${PROFILE:=r2-static}" "${ENDPOINT:?set ENDPOINT to the S3 endpoint}"
version=$(grep -o 'VERSION = "[0-9.]*"' src/web/tracker.js | grep -o '[0-9.]*')
ct="application/javascript; charset=utf-8"
up() { aws s3 cp src/web/tracker.js "s3://$BUCKET/js/$1" --profile "$PROFILE" --endpoint-url "$ENDPOINT" --content-type "$ct" --cache-control "$2" --only-show-errors; }
up "incisive-$version.js" "public, max-age=31536000, immutable"
up incisive.js "public, max-age=300"
echo "published tracker $version"
