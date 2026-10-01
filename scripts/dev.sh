#!/usr/bin/env bash
# Launch the full local app against local or explicitly configured remote services.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
env_file=${TYPEDNOTES_ENV_FILE:-$root/.env.local}
if [[ ! -f "$env_file" ]]; then
  printf 'Create %s from .env.local.example, then run bash scripts/dev.sh. See docs/local-development.md.\n' "$env_file" >&2
  exit 1
fi
set -a
source "$env_file"
set +a
export PUBLIC_URL=${PUBLIC_URL:-http://localhost:8080}
export TYPEDNOTES_BACKGROUND=${TYPEDNOTES_BACKGROUND:-0}
[[ -n ${DATABASE_URL:-} ]] || { printf 'DATABASE_URL is required.\n' >&2; exit 1; }
if [[ ${1:-} == --check ]]; then
  printf 'Local server environment loaded; background=%s. No services contacted.\n' "$TYPEDNOTES_BACKGROUND"
  exit 0
fi
command -v dx >/dev/null || { printf 'Install the Dioxus 0.7 CLI (dx) first.\n' >&2; exit 1; }
cd "$root"
exec dx serve -p web --fullstack true --addr 127.0.0.1 "$@"
