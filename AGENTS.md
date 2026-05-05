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
