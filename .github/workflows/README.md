# Workflows

## dependabot-release.yml
**Purpose:** Automated Dependabot PR review, merge, and release pipeline  
**Type:** GitHub Actions Function  
**Schedule:** Every Friday at 04:30 UTC (10:00 AM IST)  
**Triggers:**
- Schedule (weekly Friday)
- Manual (workflow_dispatch with dry-run option)

### Pipeline Steps:
1. **review-and-merge**
   - Lists open Dependabot PRs
   - Checks CI status for each PR
   - Skips major version bumps (safety)
   - Merges qualifying PRs (squash + delete branch)

2. **release** (only if PRs were merged)
   - Waits for main branch CI to pass
   - Bumps patch version in `Cargo.toml`
   - Updates `Cargo.lock`
   - Runs `cargo publish --dry-run`
   - Commits the version bump
   - Publishes to crates.io
   - Creates and pushes git tag (`vX.Y.Z`) — **triggers existing release.yml** to build binaries and create the GitHub Release

### Requirements:
- `GITHUB_TOKEN` (auto-provided)
- `CARGO_REGISTRY_TOKEN` (secret for crates.io)
- Repository permissions: contents:write, pull-requests:write, checks:read, statuses:read, actions:read, id-token:write

## Triggering Manually
```bash
# Via GitHub CLI
gh workflow run dependabot-release.yml --repo sharat/cluster-cli

# Or visit: https://github.com/sharat/cluster-cli/actions
```

## Dry Run Mode
Use dry run to test the workflow without actual merges or publishes:
```bash
gh workflow run dependabot-release.yml --repo sharat/cluster-cli -f dry_run=true
```

## Difference from swizzy
- Uses **Cargo** instead of npm (Rust project)
- Publishes to **crates.io** instead of npm
- Version bump in `Cargo.toml` instead of `package.json`
- Same tag format (`vX.Y.Z`) triggers existing `release.yml`
