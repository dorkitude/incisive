#!/usr/bin/env bash
# Run every SDK's test suite. Go and Python run locally (Python through uv);
# the others run in pinned Docker images.
#
#   sdks/test-all.sh                 # all SDKs, unit tests (integration tests skip)
#   sdks/test-all.sh go javascript   # a subset
#
# Integration tests run against a live server when these are exported:
#   INCISIVE_BASE_URL INCISIVE_PROJECT_ID INCISIVE_WRITE_KEY INCISIVE_READ_KEY INCISIVE_MASTER_KEY
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
USER_FLAG="$(id -u):$(id -g)"
CACHE="${INCISIVE_SDK_CACHE:-${TMPDIR:-/tmp}/incisive-sdk-cache}"
mkdir -p "$CACHE"/{npm,bundle,composer,m2,nuget,home}
ENV_FLAGS=()
for v in INCISIVE_BASE_URL INCISIVE_PROJECT_ID INCISIVE_WRITE_KEY INCISIVE_READ_KEY INCISIVE_MASTER_KEY; do
  if [ -n "${!v:-}" ]; then ENV_FLAGS+=(-e "$v"); fi
done
DOCKER=(docker run --rm --network host --user "$USER_FLAG" "${ENV_FLAGS[@]}")

status=0
run() {
  local name=$1
  shift
  echo "===== $name"
  if ! "$@"; then
    echo "!!!!! $name FAILED"
    status=1
  fi
}

for sdk in "${@:-python go javascript ruby php java dotnet}"; do
  for lang in $sdk; do
    case $lang in
      python) run python bash -c "cd '$ROOT/python' && uv run pytest -q" ;;
      go) run go bash -c "cd '$ROOT/go' && go vet ./... && go test -count=1 ./..." ;;
      javascript) run javascript "${DOCKER[@]}" -e HOME=/cache/home -e npm_config_cache=/cache/npm \
          -v "$CACHE":/cache -v "$ROOT/javascript":/w -w /w node:22.11.0-bookworm-slim \
          bash -c "npm install --no-audit --no-fund --silent && npm test" ;;
      ruby) run ruby "${DOCKER[@]}" -e HOME=/cache/home -e BUNDLE_PATH=/cache/bundle \
          -v "$CACHE":/cache -v "$ROOT/ruby":/w -w /w ruby:3.3.6 \
          bash -lc "bundle install --quiet && bundle exec rake test" ;;
      php) run php bash -c "$(printf '%q ' "${DOCKER[@]}") -e COMPOSER_HOME=/cache/composer -v '$CACHE':/cache -v '$ROOT/php':/app -w /app composer:2.8.3 composer install -q && \
          $(printf '%q ' "${DOCKER[@]}") -v '$ROOT/php':/app -w /app php:8.3.14-cli vendor/bin/phpunit" ;;
      java) run java "${DOCKER[@]}" -e HOME=/cache/home -v "$CACHE":/cache -v "$ROOT/java":/w -w /w \
          maven:3.9.9-eclipse-temurin-21 mvn -B -q -Dmaven.repo.local=/cache/m2 -Duser.home=/cache/home test ;;
      dotnet) run dotnet "${DOCKER[@]}" -e HOME=/cache/home -e DOTNET_CLI_TELEMETRY_OPTOUT=1 -e DOTNET_NOLOGO=1 \
          -e NUGET_PACKAGES=/cache/nuget -v "$CACHE":/cache -v "$ROOT/dotnet":/w -w /w \
          mcr.microsoft.com/dotnet/sdk:8.0.404 dotnet test Incisive.sln ;;
      *) echo "unknown SDK: $lang"; status=1 ;;
    esac
  done
done
exit $status
