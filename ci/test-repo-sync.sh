#!/usr/bin/env bash
# Check the repo-sync CLI with local-only Git fixtures.
set -euo pipefail
root=$(cd -- "$(dirname -- "$0")/.." && pwd)
cli=$root/scripts/repo-sync.sh
mkdir -p "$root/.cache"
tmp=$(mktemp -d "$root/.cache/repo-sync-test.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/checkouts" "$tmp/empty"
remote=$tmp/origin.git
repo=$tmp/checkouts/demo
git init -q --bare -b main "$remote"
git clone -q "$remote" "$repo" 2>/dev/null
commit() {
  git -C "$1" -c user.name=Test -c user.email=test@example.invalid \
    -c commit.gpgsign=false commit -qm "$2" --allow-empty
}
commit "$repo" initial
git -C "$repo" push -q origin main

expect() {
  local expected=$1 text=$2 actual=0 output
  shift 2
  output=$("$cli" "$@" </dev/null 2>&1) || actual=$?
  if [[ $actual != "$expected" || $output != *"$text"* || $output == *$'\033'* ]]; then
    printf 'Expected status %s and plain text %s; got %s:\n%s\n' \
      "$expected" "$text" "$actual" "$output" >&2
    exit 1
  fi
}

export TYPEDNOTES_REPOS_ROOT=$tmp/checkouts
expect 0 'Commands:'
expect 0 "$tmp/empty (--root)" check --root "$tmp/empty" --help # S1
expect 2 'use --yes' check # S2
expect 0 '1 checked: 1 matching' check --yes # S3, S4

git clone -q "$remote" "$tmp/writer"
commit "$tmp/writer" upstream
git -C "$tmp/writer" push -q origin main
original=$(git -C "$repo" rev-parse HEAD)
upstream=$(git -C "$tmp/writer" rev-parse HEAD)
expect 0 'no origins queried' pull --dry-run # S2, S5
[[ $(git -C "$repo" rev-parse HEAD) == "$original" ]]
expect 0 '1 OK' pull --yes # S5
[[ $(git -C "$repo" rev-parse HEAD) == "$upstream" ]]
: >"$repo/untracked"
expect 1 'working tree is dirty' undo --yes # S6
rm -- "$repo/untracked"
expect 0 '1 OK' undo --yes # S6
[[ $(git -C "$repo" rev-parse HEAD) == "$original" ]]
expect 0 'nothing to undo' undo --yes
commit "$repo" divergent
expect 1 'not a fast-forward' pull --yes # S5

git -C "$repo" branch --no-track local-only "$original"
git --git-dir="$remote" branch origin-only main
git --git-dir="$remote" update-ref refs/tags/release "$upstream"
git -C "$repo" update-ref refs/tags/release HEAD
: >"$repo/untracked"
expect 1 'different:   refs/heads/main' check --yes # S3
expect 1 'different:   refs/tags/release' check --yes
expect 1 'local only:  refs/heads/local-only' check --yes
expect 1 'origin only: refs/heads/origin-only' check --yes
expect 1 'working tree is dirty' check --yes

# S7, S8: only prune exact ancestors of live origin HEAD; retain linked branches.
git -C "$repo" branch kept-worktree "$original"
git -C "$repo" worktree add -q "$tmp/linked" kept-worktree
git -C "$repo" branch unmerged HEAD
git -C "$repo" branch --track default-alias origin/main >/dev/null
expect 0 'PLAN  demo: local-only' prune --dry-run
[[ $(git -C "$repo" rev-parse refs/heads/local-only) == "$original" ]]
expect 0 'OK    demo: prune local-only' prune --yes
if git -C "$repo" show-ref --verify --quiet refs/heads/local-only; then exit 1; fi
for kept in main kept-worktree unmerged default-alias; do
  git -C "$repo" show-ref --verify --quiet "refs/heads/$kept"
done
expect 0 'PLAN  demo: local-only' undo --pruned --dry-run
git -C "$repo" branch local-only HEAD
expect 1 'local-only already exists' undo --pruned --yes
[[ $(git -C "$repo" rev-parse refs/heads/local-only) != "$original" ]]
git -C "$repo" update-ref -d refs/heads/local-only
expect 0 'OK    demo: undo local-only' undo --pruned --yes
[[ $(git -C "$repo" rev-parse refs/heads/local-only) == "$original" ]]

# S9, S10: update only the requested local keys and refuse to undo newer edits.
git -C "$repo" config --local --add user.name 'Original Name'
git -C "$repo" config --local --add user.name 'Duplicate Name'
git -C "$repo" config --local commit.gpgsign true
git -C "$repo" config --local alias.example status
cp "$repo/.git/config" "$tmp/original-config"
expect 0 'PLAN  demo: set user.name' config --dry-run
cmp "$repo/.git/config" "$tmp/original-config"
expect 0 'local Git config updated' config --yes
[[ $(git -C "$repo" config --local --no-includes --get-all user.name) == 'Nicolas Grislain' ]]
[[ $(git -C "$repo" config --local --get user.email) == 'nicolas.grislain@gmail.com' ]]
[[ $(git -C "$repo" config --local --get commit.gpgsign) == false ]]
[[ $(git -C "$repo" config --local --get tag.gpgsign) == false ]]
[[ $(git -C "$repo" config --local --get alias.example) == status ]]
expect 0 'config already set' config --yes
cp "$repo/.git/config" "$tmp/updated-config"
git -C "$repo" config --local alias.example log
expect 1 'config changed since the recorded update' undo --config --yes
cp "$tmp/updated-config" "$repo/.git/config"
expect 0 'PLAN  demo: restore local Git config' undo --config --dry-run
cmp "$repo/.git/config" "$tmp/updated-config"
expect 0 'local Git config updated' undo --config --yes
cmp "$repo/.git/config" "$tmp/original-config"
expect 0 'no config backup' undo --config --yes

# S11: preview without writing; publish only missing topic branches, not tags.
git -C "$repo" branch --no-track master "$original"
git -C "$repo" branch --no-track publish-me HEAD
expect 0 'PLAN  demo: push publish-me' push --dry-run
if git --git-dir="$remote" show-ref --verify --quiet refs/heads/publish-me; then exit 1; fi
expect 0 'OK    demo: published publish-me' push --yes
[[ $(git --git-dir="$remote" rev-parse refs/heads/publish-me) == "$(git -C "$repo" rev-parse HEAD)" ]]
[[ $(git --git-dir="$remote" rev-parse refs/heads/main) == "$upstream" ]]
[[ $(git --git-dir="$remote" rev-parse refs/tags/release) == "$upstream" ]]
for protected in master default-alias; do
  if git --git-dir="$remote" show-ref --verify --quiet "refs/heads/$protected"; then exit 1; fi
done
expect 0 'no local-only topic branches' push --yes

git -C "$repo" remote remove origin
expect 0 '1 checkout(s); no origins queried.' check --dry-run --yes # S2
expect 2 'no origin remote' check --yes # S4
expect 2 'no Git checkouts' check --root "$tmp/empty" --dry-run
printf 'Repository sync CLI tests passed.\n'
