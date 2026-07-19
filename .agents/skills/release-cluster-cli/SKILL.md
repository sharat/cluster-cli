---
name: release-cluster-cli
description: Prepare, tag, and monitor safe GitHub releases for the cluster-cli Rust repository. Use when asked to prepare a release, check whether the repository is releasable, compare releases or tags, recommend a semantic version, draft concise human-readable release notes, create a confirmed release tag, watch the release build and publish status, or roll back a failed tag.
---

# Cluster CLI releases

Prepare a release before making any remote change. Treat tagging and pushing as explicit confirmation steps.

## Preconditions

Run these checks before proposing a version or touching release files:

```bash
git status --short --branch
git branch --show-current
git fetch --tags origin
git tag --sort=-version:refname | head -n 1
```

Require all of the following for an actual release:

- Current branch is exactly `main`; refuse on every other branch.
- Working tree is clean.
- `main` is synchronized with `origin/main`; do not release an unpushed or divergent commit.
- Proposed `vX.Y.Z` is a valid, unused semver tag and matches `version` in `Cargo.toml`.

For a comparison or dry run, report failed preconditions but continue read-only where possible.

## Preflight check

For `release check`, answer one question: is this repository in a releasable state right now? This command is strictly read-only. It never commits, stages, tags, pushes, fetches into a branch, or touches any GitHub resource, so it is safe to run at any time, on any branch, without confirmation.

Run only inspecting commands:

```bash
git branch --show-current
git status --short --branch
git fetch --tags origin
git rev-list --left-right --count origin/main...HEAD
git tag --list "v$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test --verbose
```

`git fetch` is the only remote call and it updates remote-tracking refs only; never `pull`, `merge`, or `rebase` from this command.

Evaluate each gate independently and keep going after a failure so the user sees the full picture:

- **Branch** — current branch is exactly `main`.
- **Worktree** — no staged, unstaged, or untracked changes.
- **Sync** — `main` and `origin/main` point at the same commit, with no ahead or behind counts.
- **Version** — `version` in `Cargo.toml` has no matching existing `vX.Y.Z` tag.
- **Format** — `cargo fmt -- --check` exits clean.
- **Lint** — `cargo clippy -- -D warnings` exits clean.
- **Tests** — `cargo test --verbose` passes.

Report a compact checklist, one line per gate, then a single verdict:

```text
✔ Branch      main
✔ Worktree    clean
✘ Sync        2 commits ahead of origin/main
✔ Version     0.1.9 has no existing tag
✔ Format      clean
✘ Lint        1 warning in src/app.rs
✔ Tests       48 passed

NOT READY — 2 gates failed
```

When any gate fails, give the specific remediation for that gate and nothing broader: switch to `main`; commit or stash the pending changes; push or reconcile with `origin/main` yourself; bump `version` in `Cargo.toml` because the current one is already released; run `cargo fmt`; fix the reported lint; fix the failing test. Do not perform any of these remediations as part of `release check` — name them and stop.

A `READY` verdict means the preconditions and quality gates pass; it is not itself permission to release. Releasing still requires the explicit confirmation described below.

## Prepare

Use the latest semver tag as `PREV_TAG`; compare `PREV_TAG..HEAD`. For a first release, compare the full relevant history and label it Initial release.

Inspect both commit subjects and changed paths:

```bash
git log --reverse --format='%h%x09%s' "$PREV_TAG..HEAD"
git diff --stat "$PREV_TAG..HEAD"
git diff --name-only "$PREV_TAG..HEAD"
```

Classify the recommended bump:

- Recommend **major** for a documented incompatible CLI, configuration, or behavioral change.
- Recommend **minor** for a user-visible backward-compatible feature.
- Recommend **patch** for fixes, dependency changes, documentation, CI, lint, or internal maintenance.
- If evidence conflicts or a breaking change is uncertain, state the ambiguity and ask the user to choose. Otherwise propose the exact next version without asking.

Draft short release notes based on user impact, not commit-by-commit history:

- Combine related commits into one outcome-focused bullet.
- Collapse dependency-only changes to `Updated dependencies.`
- Omit formatting, lint-only, CI-only, and refactoring-only changes unless users notice an effect.
- Prefer one sentence per item and normally no more than five bullets.
- Include a `Breaking changes` section only when needed.

Use this compact shape:

```md
## What's new

- Added …
- Fixed …

## Maintenance

- Updated dependencies.
```

Show the previous tag, commit range, recommendation with reason, notes draft, and all preflight results. Ask for one explicit confirmation before making a commit, tag, push, GitHub release, or other remote mutation. Offer `edit`, `choose version`, and `cancel`.

## Release after confirmation

1. Update `Cargo.toml` and `Cargo.lock` to the approved version, then show the diff.
2. Run `cargo fmt -- --check`, `cargo clippy -- -D warnings`, and `cargo test --verbose`.
3. Stop on a failure; do not tag or push.
4. Commit the approved version change with `chore(release): bump version to X.Y.Z` only after showing its exact staged diff.
5. Create annotated tag `vX.Y.Z` with the approved Markdown notes. Use `git tag --cleanup=verbatim` so Markdown headings are preserved. Push `main` and the tag, then report the remote URLs. The repository Release workflow uses this tag message as the GitHub Release body.
6. Monitor the tag-triggered GitHub Actions Release workflow. Do not claim a release is published until check, every platform build, and the release job succeed.
7. If the repository workflow cannot accept the curated notes, explain that it will publish its generated notes; propose a separate workflow update rather than silently overwriting a published GitHub release.

Never force-push a branch, rewrite shared history, reset the worktree, publish on a failed workflow, or continue after a failed required check. Never delete or move a tag as part of the normal release flow; the single narrow exception is the Rollback procedure below, which applies only to a failed tag nobody has consumed and only with explicit per-step user confirmation.

## Rollback

Use this only when a release has already pushed a tag and then failed — the workflow errored, the wrong version was tagged, or the notes were wrong — and the tag has not been consumed. Treat it as an explicitly confirmed exception to the tag prohibition, not as routine practice.

Confirm all of the following before proposing anything destructive. If any is false or unknown, stop and recommend a forward fix such as a new patch version instead:

- The tag is `vX.Y.Z` from the failed attempt and no later tag depends on it.
- No published GitHub Release for it has been announced, downloaded, or otherwise consumed.
- No downstream package, install script, or user is known to reference it.

Ask for a separate explicit confirmation immediately before each step. Show the exact command, run it only after the user agrees, and stop entirely on any refusal.

1. Remove the GitHub Release first, so the tag never outlives a dangling release:

```bash
gh release view vX.Y.Z
gh release delete vX.Y.Z
```

2. Delete the remote tag, then the local one:

```bash
git push origin :refs/tags/vX.Y.Z
git tag -d vX.Y.Z
```

3. Handle the `chore(release): bump version to X.Y.Z` commit forward, never by rewriting history:

```bash
git revert <commit>
```

Push the revert as an ordinary commit on `main`. Do not use `git reset`, `git rebase`, `git commit --amend`, or any force-push to remove it, even though it is the tip commit — the blanket prohibition on rewriting shared history is unchanged by this section.

Afterwards, restate the state plainly: which tag and release were removed, that `Cargo.toml` is back on the previous version, and that the next attempt needs a fresh version number. Re-run `release check` before retrying.

## Status and comparison

For `release status <tag>`, inspect the GitHub Actions workflow and GitHub Release for that tag using `gh` when authenticated; otherwise give the URLs and the exact manual checks needed.

For `compare releases <from> <to>`, remain read-only and return the same concise notes format plus the raw commit count. Do not assume a version bump or create a tag.
