# Repository sync checker contract

- **S1 — Scope.** `repo-sync.sh check` includes only direct, non-hidden children
  with a `.git` directory or file. Root precedence is `--root`,
  `TYPEDNOTES_REPOS_ROOT`, then the parent of the app checkout.
- **S2 — Read-only operation.** Help and `--dry-run` never query origin,
  except `prune --dry-run` and `push --dry-run`, which read live refs.
  Non-preview operations require confirmation or `--yes`. `check` does not
  fetch, push, update refs, or modify working trees.
- **S3 — Comparison.** Local branches and tags are compared to live `origin`
  refs by name and exact object ID. Local-only, origin-only, and mismatched
  refs and uncommitted changes are reported separately. An annotation-only
  tag change counts as a mismatch. The checker does not calculate ancestry.
- **S4 — Results.** Exit 0 for a clean match or successful help/preview;
  1 for differences; 2 for an incomplete check or usage error, taking
  precedence over differences. Reports go to stdout, diagnostics to stderr.
  Output is plain when piped or when `NO_COLOR` is nonempty.
- **S5 — Pull.** Fast-forward only a clean, checked-out branch to its configured
  `origin` upstream, or the same-named origin branch if no upstream is set.
  Skip divergent, ahead, detached, dirty, missing upstream, and other-remote
  branches. No tag, remote-tracking ref, or inactive branch updates.
- **S6 — Undo.** Record branch changes before and after mutation in
  `repo-sync-undo.jsonl` under each checkout's Git directory. Undo the latest
  recorded pull on the current branch only when clean and still at the recorded
  tip. Undo is local and journaled; downloaded objects are not removed.
- **S7 — Prune.** Delete only local branch refs whose exact tips are reachable
  from the advertised default branch of `origin`. A missing remote tip object
  prevents pruning that repo; no fetch, patch equivalence, or remote deletion.
  Protect `main`, `master`, default-branch names and upstream aliases, and
  branches checked out in any linked worktree. Do not run alongside checkout
  or other branch-management operations.
- **S8 — Recover pruned tips.** Atomically save each deleted tip in
  `refs/repo-sync/pruned/`. Record changes in `repo-sync-prune.jsonl` in the
  common Git directory. `undo --pruned` restores saved tips only into unused
  branch names and consumes the corresponding backups. Branch config is left
  untouched; this restores the tip, not the old branch reflog.
- **S9 — Configure.** `config` sets `user.name`, `user.email`,
  `commit.gpgsign`, and `tag.gpgsign` to the values shown by `config --help`
  in the common Git config. Replace duplicates for those keys only. No
  remote access, global config changes, or writes in preview mode. Do not
  follow config includes or edit worktree-specific overrides.
- **S10 — Restore config.** Save a private copy and journal fingerprints
  before atomically replacing local config under Git's config lock.
  `undo --config` restores the last backup only if the current file still
  exactly matches the recorded result.
- **S11 — Publish.** `push` creates local-only topics on the single
  configured `origin` push endpoint. Skip `main`, `master`, the advertised
  default branch, and branches tracking another remote or an existing
  `origin` branch. The server must see an absent destination; never
  update an existing branch or publish tags. `--dry-run` shows candidates
  without publishing; an explicit confirmation or `--yes` is required.
  No automatic remote deletion or rollback.
