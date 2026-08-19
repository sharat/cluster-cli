# AGENTS.md

This file provides guidance for agents working in this repository.

## Build / Lint / Test Commands

```bash
# Build
cargo build                        # debug build
cargo build --release              # optimized build (lto, strip, panic=abort)

# Testing
cargo test --verbose               # run all tests
cargo test <test_name>             # run a single test by name (e.g., cargo test test_critical_node_penalty)

# Formatting (CI enforces this)
cargo fmt -- --check               # check formatting
cargo fmt                          # apply formatting

# Linting (warnings are errors in CI)
cargo clippy -- -D warnings       # lint with warnings-as-errors

# Update dependencies
cargo update                       # update to latest compatible versions
```

## Shared Agent Skills

Reusable repository skills live in `.agents/skills/`. Read the relevant `SKILL.md` before performing a matching task; `release-cluster-cli` governs release preparation, tagging, and release-status work.

## Changelog Maintenance

The repository maintains a `CHANGELOG.md` following [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) format. Update the **Unreleased** section when making significant changes:

### When to Update

**Always update for:**
- New features (`### Added`)
- Breaking changes (`### Changed` with note)
- Important bug fixes (`### Fixed`)
- Security patches (`### Security`)
- Deprecated features (`### Deprecated`)
- Removed features (`### Removed`)

**Skip for:**
- Routine dependency updates (batch these in releases)
- Minor typo fixes or formatting
- Internal refactoring with no user impact
- CI/build configuration changes
- Documentation-only updates to non-user-facing docs

### How to Update

1. Edit the `## [Unreleased]` section in `CHANGELOG.md`
2. Add your change under the appropriate category
3. Use present tense ("Add feature" not "Added feature")
4. Be concise but clear about the impact
5. Include the change in the same PR/commit

**Example:**
```markdown
## [Unreleased]

### Added
- Real-time log streaming for pod detail view
- Keyboard shortcut (Ctrl+R) to refresh data manually

### Fixed
- Crash when namespace contains pods with missing metrics
```

### During Release

When cutting a release, the unreleased entries are moved to a new version section with a date. The release skill (`release-cluster-cli`) handles this automatically.

## Pull Request Best Practices

### Visual Changes & Demos

For PRs that include user-facing changes, include screenshots or recordings to demonstrate the changes:

**Always include visuals for:**
- New UI features or components
- Changes to existing UI/TUI elements (layout, colors, animations)
- New keyboard shortcuts or interactions
- Bug fixes that resolve visual issues
- Performance improvements visible to users

**How to add visuals:**

1. **Screenshots** - For static UI changes:
   ```bash
   # Take screenshots during manual testing
   # Save to a descriptive filename (e.g., namespace-picker-pod-counts.png)
   ```

2. **Screen recordings** - For interactive features or animations:
   ```bash
   # Record a short demo (10-30 seconds) showing the feature in action
   # Use tools like asciinema for terminal recordings, or standard screen capture
   # Save as .mp4, .gif, or .cast (asciinema format)
   ```

3. **Include in PR description**:
   ```markdown
   ## Demo
   
   ### Before
   ![Before screenshot](path/to/before.png)
   
   ### After
   ![After screenshot](path/to/after.png)
   
   ### Demo Video
   ![Demo of new feature](path/to/demo.gif)
   ```

**Benefits:**
- Reviewers can quickly understand the impact
- Users can see what changed in release notes
- Serves as documentation of the feature
- Helps catch visual regressions

## Project Overview

cluster-cli is a read-only Kubernetes TUI built on ratatui + crossterm with a tokio async runtime. It communicates with kubectl via subprocesses (30s timeout, read-only whitelist enforced).

### Data Flow

[105 more lines in file. Use offset=31 to continue.]

## CI/CD

### Workflows
| Workflow | File | Purpose |
|----------|------|---------|
| CI | `.github/workflows/ci.yml` | Check, build, test on push/PR |
| Release | `.github/workflows/release.yml` | Multi-platform binary release on git tag |
| Dependabot | `.github/dependabot.yml` | Weekly Friday 09:00 IST dependency updates |

### Release Process

**Trigger:** Git tag push (e.g., `v0.1.3`)

```bash
# Bump version in Cargo.toml/Cargo.lock, commit, create tag, push
# Then tag push triggers release.yml
cd /Users/sarat/oss/cluster-cli
# Edit Cargo.toml version manually or use cargo-bump
git add Cargo.toml
git commit -m "chore(release): bump version to 0.1.3"
git tag v0.1.3
git push origin main --follow-tags
```

**What happens:**
1. Tag push triggers `.github/workflows/release.yml`
2. Check: format, clippy, tests
3. Build: binaries for Linux x86_64, macOS ARM64, Windows x86_64
4. Package: tar.gz (Unix), zip (Windows) + SHA256 checksums
5. Create GitHub Release with auto-generated changelog
6. Attach all binary artifacts

### crates.io
Not currently published to crates.io. Distribution is via GitHub Release binaries, `install.sh`, and the Homebrew formula.

### Requirements
- `GITHUB_TOKEN` (auto-provided)

## Notes
- Multi-platform releases: Linux, macOS (ARM64), Windows
- Uses `Swatinem/rust-cache@v2` for faster builds
- Minimum Rust version: 1.70
