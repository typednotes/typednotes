#!/usr/bin/env bash
# Apply migrations/*.sql, in name order, to a local development database.
#
#   DATABASE_URL=postgres://postgres:postgres@localhost:5432/typednotes scripts/dev-db.sh
#
# Production never runs this: typednotes-infra applies the same directory
# (via TypednotesMigrations.lean) as a declared migration history. Like
# ledger's `migrate`, this keeps no bookkeeping of its own and is meant for a
# fresh database — each file runs in one transaction and stops at the first
# error. Then, if the sibling checkouts exist (../ledger, ../liaison, or
# LEDGER_DIR / LIAISON_DIR), their `sql/` too, in that order: ledger's
# tables reference `orgs`/`users`, and liaison writes ledger's. Set
# SIBLINGS=0 to apply only this repository's migrations.
set -euo pipefail
cd "$(dirname "$0")/.."

: "${DATABASE_URL:?set DATABASE_URL to the local database to migrate}"
command -v psql >/dev/null || { echo "psql is required" >&2; exit 2; }

shopt -s nullglob
files=(migrations/[0-9]*_*.sql)
[ ${#files[@]} -gt 0 ] || { echo "no migrations found" >&2; exit 1; }

if [ "${SIBLINGS:-1}" != 0 ]; then
  for dir in "${LEDGER_DIR:-../ledger}" "${LIAISON_DIR:-../liaison}"; do
    if [ -d "$dir/sql" ]; then
      files+=("$dir"/sql/[0-9]*_*.sql)
    else
      echo "skipping $dir/sql (not found)"
    fi
  done
fi

for f in "${files[@]}"; do
  echo "applying $f"
  psql "$DATABASE_URL" --quiet -v ON_ERROR_STOP=1 --single-transaction -f "$f"
done
