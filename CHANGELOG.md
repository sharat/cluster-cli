# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Cloud Agent development environment configuration (`.cursor/environment.json` and `.cursor/install.sh`)
- Select each regular container in a multi-container pod with `c`
- Toggle between live current logs and the selected container's previous logs with `p`
- Search, timestamp, wrap, follow, and export controls for buffered pod logs
- Node-pool filtering across common AKS, EKS, GKE, Karpenter, Cluster API, and kOps labels
- Jobs, CronJobs, HPAs, PDBs, Services, Ingresses, and PVCs in the workload popup

### Changed
- Updated 82 transitive dependencies to their latest compatible versions
- Log streams request Kubernetes timestamps and surface `kubectl logs` stderr failures
- Workload details adapt to each resource's rollout, schedule, scaling, network, or storage state
- Documented changelog and visual evidence requirements for pull requests

## [0.2.5] - 2026-08-19

### Changed
- Updated clap dependency in the cargo-dependencies group

## [0.2.4] - 2026-08-19

### Changed
- Updated cargo-dependencies group with 2 packages

## [0.2.3] - 2026-08-19

### Changed
- Updated cargo-dependencies group with 7 packages

## [0.2.2] - 2026-08-19

### Fixed
- Updater now verifies bundled install script

## [0.2.1] - 2026-08-19

### Added
- Gradient braille loading animations in UI

### Changed
- Improved documentation for namespace pod totals and updates

## [0.2.0] - 2026-08-19

### Added
- Pod counts display in namespace picker
- Secure update commands and bounded event flow
- Agent skill for cluster-cli release management

## [0.1.8] - 2026-08-19

### Changed
- Updated cargo-dependencies group with 3 packages

## [0.1.7] - 2026-08-19

### Changed
- Updated compatible dependencies

### Fixed
- Resolved linting errors

## [0.1.6] - 2026-08-18

### Changed
- Formatted entire codebase with rustfmt to pass CI formatting checks
- Centralized config directory and fixed cluster-rs naming mismatch

### Fixed
- Added `.antigravitycli/` to gitignore

## [0.1.5] - 2026-08-18

### Changed
- Updated serde_json in the cargo-dependencies group
- Updated tokio in the cargo-dependencies group

### Fixed
- Release pipeline now uses direct verification instead of CI polling
- Removed crates.io publishing from release flow (distribution via GitHub releases only)
- Hardened dependency release automation

## [0.1.4] - 2026-08-18

### Changed
- Updated reqwest in the cargo-dependencies group

### Added
- Comprehensive release process and CI/CD documentation in AGENTS.md

### Fixed
- YAML syntax issues in dependabot-release workflow

## [0.1.3] - 2026-08-18

### Added
- Dependabot review and release pipeline
- Pinned Rust toolchain to 1.94.1 for reproducible builds

### Changed
- Updated multiple cargo dependencies across several groups

## [0.1.2] - 2026-08-17

### Changed
- Updated cargo-dependencies group with 4 packages

## [0.1.1] - 2026-08-17

### Added
- Tests for EventDetail overlay key bindings

## [0.1.0] - 2026-08-17

### Added
- Initial release of cluster-cli
- Interactive TUI for monitoring Kubernetes cluster health
- Real-time node and pod metrics via `kubectl top`
- Health scoring system (A-F grades)
- Incident tracking and categorization
- Dashboard view with cluster overview
- Node detail view with resource usage
- Pod detail view with logs and events
- Namespace filtering and selection
- Workload summaries (Deployments, StatefulSets, DaemonSets)
- Configuration file support (`~/.config/cluster/config.toml`)
- Auto-update checking
- Multi-platform support (Linux, macOS, Windows)

[Unreleased]: https://github.com/sharat/cluster-cli/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/sharat/cluster-cli/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/sharat/cluster-cli/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/sharat/cluster-cli/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/sharat/cluster-cli/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/sharat/cluster-cli/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/sharat/cluster-cli/compare/v0.1.8...v0.2.0
[0.1.8]: https://github.com/sharat/cluster-cli/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/sharat/cluster-cli/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/sharat/cluster-cli/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/sharat/cluster-cli/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/sharat/cluster-cli/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/sharat/cluster-cli/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/sharat/cluster-cli/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/sharat/cluster-cli/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/sharat/cluster-cli/releases/tag/v0.1.0
