#!/usr/bin/env bash
# Check sibling Git checkouts against origin without fetching or writing refs.
set -euo pipefail
export GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0

say() {
  local color=$1 label palette message
  shift
  message=$*
  if [[ -t 1 && -z ${NO_COLOR:-} && ${TERM:-dumb} != dumb ]]; then
    case $message in
      'OK    '*) label=OK; palette='1;30;42' ;;
      'DIFF  '*) label=DIFF; palette='1;30;43' ;;
      'SKIP  '*) label=SKIP; palette='1;30;45' ;;
      'ERROR '*) label=ERROR; palette='1;37;41' ;;
      'PLAN  '*) label=PLAN; palette='1;30;46' ;;
      *) printf '\033[%sm%s\033[0m\n' "$color" "$message"; return ;;
    esac
    printf '\033[%sm %-5s \033[0m  %s\n' "$palette" "$label" "${message:6}"
  else
    printf '%s\n' "$message"
  fi
}

terminal_banner() {
  if [[ -t 1 && -z ${NO_COLOR:-} && ${TERM:-dumb} != dumb ]]; then
    printf '\n\033[1;35mTYPEDNOTES\033[0m  \033[1;36m%s\033[0m\n\n' "$1"
  fi
}

fail() {
  say '1;31' "Error: $*" >&2
  exit 2
}

help_main() {
  say '1;36' 'Typednotes · Repository sync'
  cat <<'EOF'
Usage: scripts/repo-sync.sh <command> [options]

Commands:
  check    Compare sibling repositories with live origin branches and tags.
  pull     Fast-forward clean, checked-out branches from origin.
  prune    Delete local branches fully merged into origin's default branch.
  push     Publish local-only topic branches to origin.
  config   Set local Git author identity and disable commit/tag signing.
  undo     Undo a recorded pull, prune, or config change.

Example: scripts/repo-sync.sh pull --dry-run
EOF
}

help_command() {
  local extra=
  if [[ $action == undo ]]; then extra=' [--pruned | --config]'; fi
  say '1;36' "Typednotes · $action"
  cat <<EOF
Usage: scripts/repo-sync.sh $action [--root DIR] [--yes] [--dry-run]$extra

Options:
  --root DIR  Scan direct, non-hidden Git checkouts under DIR.
              Effective root: $root ($root_source)
  --yes       Run without prompting (required without a terminal).
  --dry-run   Preview without changes, even with --yes.
  -h, --help  Show this help.

Exit codes: 0 success, 1 differences or skipped operations, 2 errors.
Example: scripts/repo-sync.sh $action --yes
EOF
  case $action in
    check) printf '\nCompares branches and tags by name and exact ref ID; flags dirty trees.\n' ;;
    pull) printf '\nFast-forwards only the checked-out branch. Never merges divergent history or updates tags.\n' ;;
    prune) printf '\nChecks live origin HEAD, including in --dry-run. If its commit is missing\nlocally, fetch origin separately and retry. Squash merges do not count.\nPreview: scripts/repo-sync.sh prune --dry-run\n' ;;
    push) printf '\nCreates missing branches only; never updates existing branches, pushes tags,\nor pushes main or master. Dry run reads live origin but publishes nothing.\nPreview: scripts/repo-sync.sh push --dry-run\n' ;;
    config) printf '\nSets user.name to %s, user.email to %s,\nand commit.gpgsign and tag.gpgsign to false. No global changes.\n' "$author_name" "$author_email" ;;
    undo) printf '\nRestores a pulled branch only if clean and still at the recorded tip.\n  --pruned   Restore saved pruned branches without replacing existing branches.\n  --config   Restore the latest local Git config backup if unchanged since.\n' ;;
  esac
}

root_source='script location'
if [[ -n ${TYPEDNOTES_REPOS_ROOT:-} ]]; then
  root=$TYPEDNOTES_REPOS_ROOT
  root_source='TYPEDNOTES_REPOS_ROOT'
else
  root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
fi

author_name='Nicolas Grislain'
author_email='nicolas.grislain@gmail.com'
case ${1:-} in
  ''|-h|--help) help_main; exit 0 ;;
  check|pull|prune|push|config|undo) action=$1; shift ;;
  *) fail "unknown command: $1. See --help." ;;
esac

yes=0 dry_run=0 show_help=0 restore_pruned=0 restore_config=0
while (( $# )); do
  case $1 in
    --root)
      [[ $# -ge 2 && -n $2 && $2 != --* ]] || fail '--root requires a directory'
      root=$2
      root_source='--root'
      shift
      ;;
    --yes) yes=1 ;;
    --dry-run) dry_run=1 ;;
    --pruned)
      [[ $action == undo ]] || fail '--pruned is only valid with undo'
      restore_pruned=1
      ;;
    --config)
      [[ $action == undo ]] || fail '--config is only valid with undo'
      restore_config=1
      ;;
    -h|--help) show_help=1 ;;
    *) fail "unknown option: $1. See $action --help." ;;
  esac
  shift
done
if (( restore_pruned && restore_config )); then fail 'choose either --pruned or --config'; fi
if (( show_help )); then help_command; exit 0; fi
[[ -d $root ]] || fail "not a directory: $root"
if (( ! dry_run && ! yes )); then
  [[ -t 0 ]] || fail 'use --yes to proceed, or --dry-run to list checkouts'
  printf 'Run %s for checkouts under %s? [y/N] ' "$action" "$root" >&2
  read -r reply || fail 'confirmation not received'
  case $reply in y|Y|yes|YES) ;; *) fail 'check cancelled' ;; esac
fi

if (( dry_run )); then terminal_banner "$action · preview"; else terminal_banner "$action"; fi

# Git refs exclude control characters and backslashes, but can include quotes.
journal() {
  local state=$1
  printf '{"state":"%s","id":"%s","command":"repo-sync.sh %s","branch_key":"%s","before":"%s","after":"%s","target":"%s","inverse":"%s"}\n' \
    "$state" "$id" "$action" "$branch_key" "$before" "$after" "$target" \
    "${inverse:-repo-sync.sh undo}" >>"$log"
}

skip() { say '1;33' "SKIP  $name: $*"; return 1; }
problem() { say '1;31' "ERROR $name: $*" >&2; return 2; }

change_branch() {
  local branch status before after target branch_key log id tracking ref advertised record
  status=$(git -C "$repo" status --porcelain=v1 --untracked-files=normal) ||
    { problem 'could not read local state'; return $?; }
  if [[ -n $status ]]; then skip 'working tree is dirty'; return $?; fi
  branch=$(git -C "$repo" symbolic-ref -q HEAD) ||
    { skip 'detached HEAD'; return $?; }
  before=$(git -C "$repo" rev-parse --verify HEAD 2>/dev/null) ||
    { skip 'branch has no commits'; return $?; }
  log=$(git -C "$repo" rev-parse --path-format=absolute --git-path repo-sync-undo.jsonl) ||
    { problem 'could not locate undo log'; return $?; }
  target=${branch//\"/\\\"}
  branch_key=$(printf '%s' "$branch" | git -C "$repo" hash-object --stdin) ||
    { problem 'could not identify branch for undo log'; return $?; }
  id="$(date -u +%Y-%m-%dT%H:%M:%SZ)/$$/$seen"

  if [[ $action == pull ]]; then
    tracking=$(git -C "$repo" config --get "branch.${branch#refs/heads/}.remote") || tracking=
    if [[ -n $tracking && $tracking != origin ]]; then skip "tracks $tracking, not origin"; return $?; fi
    ref=$branch
    if [[ $tracking == origin ]]; then
      ref=$(git -C "$repo" config --get "branch.${branch#refs/heads/}.merge") ||
        { problem 'missing origin upstream ref'; return $?; }
    fi
    if [[ $ref != refs/heads/* ]]; then skip 'upstream is not a branch'; return $?; fi
    if ! git -C "$repo" remote get-url origin >/dev/null 2>&1; then
      problem 'no origin remote'; return $?
    fi
    if advertised=$(git -C "$repo" ls-remote --exit-code --heads origin "$ref" </dev/null 2>/dev/null); then
      after=$(awk -v ref="$ref" '$2 == ref { print $1 }' <<<"$advertised")
      if [[ -z $after ]]; then skip "no origin branch ${ref#refs/heads/}"; return $?; fi
    elif [[ $? == 2 ]]; then
      skip "no origin branch ${ref#refs/heads/}"; return $?
    else
      problem 'could not query origin'; return $?
    fi
    if [[ $before == "$after" ]]; then say '1;32' "OK    $name: already current"; return 0; fi
    # Download only objects, not remote-tracking refs, tags, or FETCH_HEAD.
    if ! git -C "$repo" cat-file -e "$after^{commit}" 2>/dev/null &&
       ! git -C "$repo" fetch --quiet --no-tags --no-write-fetch-head --refmap= \
         origin "$after" </dev/null; then
      problem 'could not retrieve origin commit'; return $?
    fi
    if git -C "$repo" merge-base --is-ancestor "$after" "$before"; then
      skip 'local branch is ahead'; return $?
    fi
    if ! git -C "$repo" merge-base --is-ancestor "$before" "$after"; then
      skip 'not a fast-forward (diverged or incomplete history)'; return $?
    fi
  else
    if [[ ! -f $log ]]; then say '1;32' "OK    $name: nothing to undo"; return 0; fi
    # All fields used here precede the quoted branch name and contain no JSON escapes.
    record=$(awk -F '"' -v key="$branch_key" -v head="$before" '
      $16 != key { next }
      $4 == "pending" { pending=$8; command=$12; previous=$20; next_tip=$24; next }
      $4 == "done" && $8 == pending {
        pending=""
        if ($12 ~ / pull$/) { stack[++n]=$24 " " $20 }
        else if (n > 0) { n-- }
      }
      END {
        if (pending != "" && next_tip == head) {
          if (command ~ / pull$/) stack[++n]=next_tip " " previous
          else if (n > 0) n--
        }
        if (n > 0) print stack[n]
      }
    ' "$log") || { problem 'could not read undo log'; return $?; }
    if [[ -z $record ]]; then say '1;32' "OK    $name: nothing to undo"; return 0; fi
    read -r ref after <<<"$record"
    if [[ $before != "$ref" ]]; then skip 'branch changed since the recorded pull'; return $?; fi
    if [[ ! $after =~ ^[0-9a-f]{40}$ && ! $after =~ ^[0-9a-f]{64}$ ]]; then
      problem 'invalid undo record'; return $?
    fi
  fi

  # Recheck state after contacting origin; never intentionally replace newer work.
  if [[ $(git -C "$repo" symbolic-ref -q HEAD) != "$branch" ||
        $(git -C "$repo" rev-parse HEAD) != "$before" ]] ||
     ! status=$(git -C "$repo" status --porcelain=v1 --untracked-files=normal) ||
     [[ -n $status ]]; then
    skip 'checkout changed during the operation'; return $?
  fi
  journal pending || { problem 'could not write undo log'; return $?; }
  if [[ $action == pull ]]; then
    git -C "$repo" -c merge.autoStash=false merge --ff-only --no-overwrite-ignore --quiet "$after" ||
      { problem 'fast-forward failed'; return $?; }
  else
    git -C "$repo" reset --keep --quiet "$after" ||
      { problem 'undo failed'; return $?; }
  fi
  journal done || { problem 'branch changed but could not complete undo log'; return $?; }
  say '1;32' "OK    $name: ${before:0:8} -> ${after:0:8}"
}

prune_branches() {
  local namespace=refs/repo-sync/pruned/ inverse='repo-sync.sh undo --pruned'
  local -x GIT_NO_REPLACE_OBJECTS=1
  local log heads branch oid ref backup advertised base_ref base_oid local_default busy
  local common worktree_gitdir
  local before after id target branch_key changed=0 skipped=0
  common=$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir) ||
    { problem 'could not locate prune log'; return $?; }
  log=$common/repo-sync-prune.jsonl
  if [[ $action == prune ]]; then
    advertised=$(git -C "$repo" ls-remote --exit-code --symref origin HEAD </dev/null 2>/dev/null) ||
      { problem 'could not query origin HEAD'; return $?; }
    base_ref=$(awk '$1 == "ref:" && $3 == "HEAD" { print $2 }' <<<"$advertised")
    base_oid=$(awk '$2 == "HEAD" { print $1 }' <<<"$advertised")
    [[ $base_ref == refs/heads/* && -n $base_oid ]] ||
      { problem 'origin HEAD does not name a default branch'; return $?; }
    git -C "$repo" cat-file -e "$base_oid^{commit}" 2>/dev/null ||
      { problem 'origin HEAD commit not available locally; fetch origin and retry'; return $?; }
    local_default=$(git -C "$repo" symbolic-ref -q refs/remotes/origin/HEAD) || local_default=
    local_default=${local_default#refs/remotes/origin/}
    heads=$(git -C "$repo" for-each-ref --merged="$base_oid" \
      --format='%(objectname)%09%(refname)%09%(upstream:remotename)%09%(upstream:remoteref)' refs/heads) ||
      { problem 'could not list merged branches'; return $?; }
  else
    inverse='repo-sync.sh prune'
    heads=$(git -C "$repo" for-each-ref --format='%(objectname)%09%(refname)' "$namespace") ||
      { problem 'could not list saved branches'; return $?; }
  fi
  local tracking upstream
  # Ref names cannot contain spaces; trailing fields may be empty.
  while IFS=$'\t' read -r oid ref tracking upstream; do
    [[ -n $oid ]] || continue
    if [[ $action == prune ]]; then
      branch=$ref
      # Also protect local aliases of the default branch.
      if [[ $branch == "$base_ref" || $branch == "refs/heads/$local_default" ||
            $branch == refs/heads/main || $branch == refs/heads/master ||
            ( $tracking == origin && $upstream == "$base_ref" ) ]]; then continue; fi
      busy=$(git -C "$repo" worktree list --porcelain) ||
        { problem 'could not inspect linked worktrees'; return $?; }
      if grep -Fxq "branch $branch" <<<"$busy"; then continue; fi
      # Rebase and bisect can temporarily detach a worktree from its branch.
      for worktree_gitdir in "$common" "$common"/worktrees/*; do
        if grep -Fxq "$branch" "$worktree_gitdir/rebase-merge/head-name" \
          "$worktree_gitdir/rebase-apply/head-name" 2>/dev/null ||
          grep -Fxq "${branch#refs/heads/}" "$worktree_gitdir/BISECT_START" 2>/dev/null; then
          continue 2
        fi
      done
      backup=$namespace${branch#refs/heads/}
      before=$oid after=absent
    else
      backup=$ref
      branch=refs/heads/${backup#"$namespace"}
      if git -C "$repo" show-ref --verify --quiet "$branch"; then
        say '1;33' "SKIP  $name: ${branch#refs/heads/} already exists"
        skipped=1
        continue
      fi
      before=absent after=$oid
    fi
    changed=$((changed + 1))
    if (( dry_run )); then
      say '1;36' "PLAN  $name: ${branch#refs/heads/} (${oid:0:8})"
      continue
    fi
    id="$(date -u +%Y-%m-%dT%H:%M:%SZ)/$$/$seen/$changed"
    target=${branch//\"/\\\"}
    branch_key=$(printf '%s' "$branch" | git -C "$repo" hash-object --stdin) ||
      { problem 'could not identify branch for prune log'; return $?; }
    journal pending || { problem 'could not write prune log'; return $?; }
    # Atomically save the tip before deleting, or restore only into a free name.
    # Expected IDs prevent deleting a tip that changed after the ancestry check.
    if [[ $action == prune ]]; then
      printf 'create %s %s\ndelete %s %s\n' "$backup" "$oid" "$branch" "$oid"
    else
      printf 'create %s %s\ndelete %s %s\n' "$branch" "$oid" "$backup" "$oid"
    fi | git -C "$repo" update-ref --no-deref --stdin ||
      { problem "could not $action ${branch#refs/heads/}"; return $?; }
    journal done || { problem 'refs changed but could not complete prune log'; return $?; }
    say '1;32' "OK    $name: $action ${branch#refs/heads/}"
  done <<<"$heads"
  if (( changed == 0 && skipped == 0 )); then say '1;32' "OK    $name: no candidates"; fi
  return "$skipped"
}

configure_git() (
  local common path lock log backups backup record key value current i
  local id target=config branch_key=config before after inverse='repo-sync.sh undo --config'
  local keys=(user.name user.email commit.gpgsign tag.gpgsign)
  local values=("$author_name" "$author_email" false false)
  common=$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir) ||
    { problem 'could not locate local Git config'; return $?; }
  path=$common/config
  lock=$path.lock
  log=$common/repo-sync-config.jsonl
  backups=$common/repo-sync-config-backups
  if [[ ! -f $path || -L $path ]]; then problem 'local Git config is not a regular file'; return $?; fi
  if ! git config --file "$path" --no-includes --list >/dev/null; then
    problem 'could not read local Git config'; return $?
  fi

  if [[ $action == config ]]; then
    for i in "${!keys[@]}"; do
      key=${keys[$i]} value=${values[$i]}
      current=$(git config --file "$path" --no-includes --get-all "$key") || current=
      [[ $current == "$value" ]] || break
    done
    if [[ $current == "$value" ]]; then say '1;32' "OK    $name: config already set"; return 0; fi
  else
    if [[ ! -f $log ]]; then say '1;32' "OK    $name: no config backup"; return 0; fi
    current=$(git -C "$repo" hash-object --no-filters "$path") ||
      { problem 'could not fingerprint config'; return $?; }
    # Only these fixed-format journal fields are needed to locate a private backup.
    record=$(awk -F '"' -v current="$current" '
      function apply() {
        if (command == "repo-sync.sh config") stack[++n]=id " " tip
        else if (command == "repo-sync.sh undo" && n > 0) n--
      }
      $4 == "pending" { id=$8; command=$12; tip=$24; next }
      $4 == "done" && $8 == id {
        apply()
        id=""
      }
      END {
        if (id != "" && tip == current) apply()
        if (n > 0) print stack[n]
      }
    ' "$log") || { problem 'could not read config journal'; return $?; }
    if [[ -z $record ]]; then say '1;32' "OK    $name: no config backup"; return 0; fi
    read -r id after <<<"$record"
    [[ $id =~ ^config\.[[:alnum:]]+$ && -f $backups/$id/before ]] ||
      { problem 'config backup is missing or invalid'; return $?; }
    backup=$backups/$id
    if [[ $current != "$after" ]]; then skip 'config changed since the recorded update'; return $?; fi
  fi

  if (( dry_run )); then
    if [[ $action == config ]]; then
      say '1;36' "PLAN  $name: set ${keys[*]}"
    else
      say '1;36' "PLAN  $name: restore local Git config"
    fi
    return 0
  fi

  # Hold Git's config lock while staging the replacement and validating inputs.
  umask 077
  if ! (set -o noclobber; : >"$lock") 2>/dev/null; then
    problem 'could not lock Git config'; return $?
  fi
  trap 'rm -f -- "$lock"' EXIT
  before=$(git -C "$repo" hash-object --no-filters "$path") ||
    { problem 'could not fingerprint config'; return $?; }
  if [[ $action == config ]]; then
    mkdir -p -- "$backups" && backup=$(mktemp -d "$backups/config.XXXXXX") &&
      cat -- "$path" >"$backup/before" && cp -p -- "$path" "$lock" ||
      { problem 'could not back up Git config'; return $?; }
    id=${backup##*/}
    for i in "${!keys[@]}"; do
      git config --file "$lock" --no-includes --replace-all "${keys[$i]}" "${values[$i]}" ||
        { problem "could not set ${keys[$i]}"; return $?; }
    done
  else
    if [[ $before != "$after" ]]; then skip 'config changed during the operation'; return $?; fi
    cp -p -- "$path" "$lock" && cat -- "$backup/before" >"$lock" ||
      { problem 'could not stage config restoration'; return $?; }
    inverse='repo-sync.sh config'
  fi
  after=$(git -C "$repo" hash-object --no-filters "$lock") ||
    { problem 'could not fingerprint staged config'; return $?; }
  journal pending || { problem 'could not write config journal'; return $?; }
  mv -f -- "$lock" "$path" || { problem 'could not replace Git config'; return $?; }
  trap - EXIT
  journal done || { problem 'config changed but could not complete journal'; return $?; }
  say '1;32' "OK    $name: local Git config updated"
)

publish_branches() {
  local url advertised default_ref branches candidates ref oid errors=0
  url=$(git -C "$repo" remote get-url --push --all origin 2>/dev/null) ||
    { problem 'no origin push URL'; return $?; }
  if [[ -z $url || $url == *$'\n'* ]]; then
    problem 'expected exactly one origin push URL'; return $?
  fi
  advertised=$(git -C "$repo" ls-remote --symref -- "$url" HEAD 'refs/heads/*' </dev/null 2>/dev/null) ||
    { problem 'could not list branches on the origin push endpoint'; return $?; }
  default_ref=$(awk '$1 == "ref:" && $3 == "HEAD" { print $2 }' <<<"$advertised")
  branches=$(git -C "$repo" for-each-ref \
    --format='%(objectname)%09%(refname)%09%(upstream:remotename)%09%(upstream:remoteref)' refs/heads) ||
    { problem 'could not list local branches'; return $?; }
  candidates=$(printf '%s\n\n%s\n' "$advertised" "$branches" | awk -v protected_ref="$default_ref" '
    NF == 0 { in_local = 1; next }
    !in_local { if ($2 ~ /^refs\/heads\//) remote[$2]=1; next }
    $3 != "" && ($3 != "origin" || ($4 in remote)) { next }
    !($2 in remote) && $2 != protected_ref && $2 != "refs/heads/main" && $2 != "refs/heads/master" {
      print $1, $2
    }
  ') || { problem 'could not compare branch names'; return $?; }
  if [[ -z $candidates ]]; then say '1;32' "OK    $name: no local-only topic branches"; return 0; fi
  while read -r oid ref; do
    if (( dry_run )); then
      say '1;36' "PLAN  $name: push ${ref#refs/heads/} (${oid:0:8})"
      continue
    fi
    if [[ $(git -C "$repo" rev-parse --verify "$ref" 2>/dev/null) != "$oid" ]]; then
      say '1;31' "ERROR $name: ${ref#refs/heads/} changed after enumeration" >&2
      errors=1
      continue
    fi
    # An empty expected value is a creation-only lease. It can never authorize
    # an overwrite, even if someone creates the branch after the preview.
    if git -C "$repo" -c remote.origin.mirror=false push --quiet --no-follow-tags \
      --recurse-submodules=no "--force-with-lease=$ref:" origin "$oid:$ref" </dev/null; then
      say '1;32' "OK    $name: published ${ref#refs/heads/}"
    else
      say '1;31' "ERROR $name: could not publish ${ref#refs/heads/}" >&2
      errors=1
    fi
  done <<<"$candidates"
  if (( errors )); then return 2; fi
  return 0
}

seen=0 clean=0 different=0 errors=0
for repo in "$root"/*; do
  [[ -d $repo/.git || -f $repo/.git ]] || continue
  seen=$((seen + 1))
  name=${repo##*/}
  if (( dry_run && ! restore_pruned && ! restore_config )) &&
    [[ $action != prune && $action != config && $action != push ]]; then
    say '1;36' "PLAN  $name"
    continue
  fi
  if [[ $action != check ]]; then
    result=0
    if [[ $action == prune ]] || (( restore_pruned )); then
      prune_branches || result=$?
    elif [[ $action == config ]] || (( restore_config )); then
      configure_git || result=$?
    elif [[ $action == push ]]; then
      publish_branches || result=$?
    else
      change_branch || result=$?
    fi
    case $result in
      0) clean=$((clean + 1)) ;;
      1) different=$((different + 1)) ;;
      *) errors=$((errors + 1)) ;;
    esac
    continue
  fi
  if ! local_refs=$(git -C "$repo" for-each-ref --format='%(objectname)%09%(refname)' refs/heads refs/tags) ||
     ! dirty=$(git -C "$repo" status --porcelain=v1 --untracked-files=normal); then
    say '1;31' "ERROR $name: could not read local Git state" >&2
    errors=$((errors + 1))
    continue
  fi
  if ! git -C "$repo" remote get-url origin >/dev/null 2>&1; then
    say '1;31' "ERROR $name: no origin remote" >&2
    errors=$((errors + 1))
    continue
  fi
  if ! remote_refs=$(git -C "$repo" ls-remote --refs --heads --tags origin </dev/null 2>/dev/null); then
    say '1;31' "ERROR $name: could not query origin (check access and connectivity)" >&2
    errors=$((errors + 1))
    continue
  fi
  # Git ref names cannot contain whitespace; a blank line separates the two lists.
  changes=$(printf '%s\n\n%s\n' "$local_refs" "$remote_refs" | awk '
    NF == 0 { remote = 1; next }
    !remote { local_refs[$2] = $1; next }
    !($2 in local_refs) { print "  origin only: " $2; next }
    {
      if (local_refs[$2] != $1) print "  different:   " $2
      delete local_refs[$2]
    }
    END { for (ref in local_refs) print "  local only:  " ref }
  ' | LC_ALL=C sort)
  if [[ -z $changes && -z $dirty ]]; then
    say '1;32' "OK    $name"
    clean=$((clean + 1))
  else
    say '1;33' "DIFF  $name"
    if [[ -n $changes ]]; then
      while IFS= read -r detail; do
        case $detail in
          '  origin only:'*) say 36 "$detail" ;;
          '  local only:'*) say 35 "$detail" ;;
          *) say 33 "$detail" ;;
        esac
      done <<<"$changes"
    fi
    if [[ -n $dirty ]]; then say 33 '  working tree is dirty'; fi
    different=$((different + 1))
  fi
done

(( seen > 0 )) || fail "no Git checkouts under $root"
if (( dry_run && ! restore_pruned && ! restore_config )) &&
  [[ $action != prune && $action != config && $action != push ]]; then
  say '1;36' "$seen checkout(s); no origins queried."
  exit 0
fi
if [[ $action == check ]]; then
  say '1;36' "$seen checked: $clean matching, $different with differences, $errors errors."
else
  say '1;36' "$seen repositories: $clean OK, $different skipped, $errors errors."
fi
if (( errors )); then exit 2; fi
if (( different )); then exit 1; fi
exit 0
