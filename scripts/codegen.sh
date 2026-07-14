#!/usr/bin/env bash
# Regenerate checked-in bindings against a temporary migrated PostgreSQL database.
# Required tools are included in the development shell.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

workdir="$(mktemp -d)"
pgdata="$workdir/pgdata"
sock="$workdir/sock"
mkdir -p "$sock"

cleanup() {
  pg_ctl -D "$pgdata" -m immediate stop >/dev/null 2>&1 || true
  rm -rf "$workdir"
}
trap cleanup EXIT

echo "==> initdb"
initdb -D "$pgdata" -U postgres --auth=trust --no-sync >/dev/null

echo "==> starting postgres on unix socket $sock"
pg_ctl -D "$pgdata" \
  -o "-k $sock -c listen_addresses='' -c fsync=off" \
  -w start >/dev/null

export PGHOST="$sock"
export PGUSER=postgres

createdb -h "$sock" -U postgres circus_codegen

echo "==> applying migrations"
for f in crates/migrations/migrations/[0-9]*.sql; do
  echo "    $f"
  psql -v ON_ERROR_STOP=1 -h "$sock" -U postgres -d circus_codegen -q -f "$f"
done

echo "==> applying runner bootstrap table"
psql -v ON_ERROR_STOP=1 -h "$sock" -U postgres -d circus_codegen -q \
  -f crates/migrations/bootstrap.sql

echo "==> cornucopia live"
cornucopia live "host=$sock user=postgres dbname=circus_codegen"

echo "==> generated db/circus-codegen"
