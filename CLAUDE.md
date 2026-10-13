# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                        # debug build
cargo build --release              # optimized build (lto, strip, panic=abort)
cargo test --verbose               # run all tests
cargo test <test_name>             # run a single test by name
cargo fmt -- --check               # check formatting (CI enforces this)
cargo clippy -- -D warnings        # lint (warnings are errors in CI)
cargo update                       # update dependencies to latest compatible versions
```

## Shared agent skills

Reusable repository skills live in `.agents/skills/`. When a task matches a skill's description, read and follow its `SKILL.md`. The `release-cluster-cli` skill governs release preparation, tagging, and release-status work.

## Changelog maintenance

Update `CHANGELOG.md` (Unreleased section) for significant changes: new features, breaking changes, important fixes, security patches. Skip routine dependency updates, typos, internal refactoring, and CI changes. Use present tense and categorize under Added/Changed/Fixed/Security/Deprecated/Removed. Include the update in the same PR.

## Pull request visuals

For PRs with user-facing UI/TUI changes, include screenshots or recordings in the PR description. Show before/after comparisons for changes, or demos for new features. This helps reviewers understand impact and serves as documentation. Use standard image formats (png, jpg) or recordings (mp4, gif, asciinema .cast files).

## Architecture

cluster-cli is a read-only Kubernetes TUI built on ratatui + crossterm with a tokio async runtime.

The repo is a Cargo workspace. The root package is the TUI; `crates/cluster-core` holds everything UI-agnostic (`config`, `events`, `data/*`); `crates/cluster-desktop` is an experimental GPUI desktop app. `default-members` excludes the desktop crate, so plain `cargo build`/`cargo test` (and CI) cover only the TUI and core — use `-p cluster-desktop` for the GUI. Paths below under `src/data/`, `src/config.rs` and `src/events/mod.rs` now live in `crates/cluster-core/src/`.

The desktop app runs one `Fetcher` per kubeconfig context on a tokio runtime thread, each wrapped in `collector::with_context`, a task-local override that makes `run_cmd` pass `--context`. Events are tagged with the cluster index and drained into GPUI via `cx.spawn`.

### Data flow

```
kubectl subprocess (30s timeout, read-only whitelist enforced)
  └─ src/data/collector.rs        list_* / top_* / fetch_workload_summaries; build_pods / build_node_metrics / build_events parse items
  └─ src/data/checks.rs           cluster-scoped + crd_checks health checks → ResourceProblem
  └─ src/data/watch.rs            long-lived `kubectl get --watch-only --output-watch-events` streams (pods, nodes, events)
       └─ src/data/store.rs       ClusterStore: raw objects replaced by each poll, patched by watches; snapshot() derives ClusterSnapshot
            └─ src/data/fetcher.rs background task: poll on the interval, apply watch updates, debounce (500ms) → AppEvent over mpsc
            └─ src/events/mod.rs  AppEvent / DataEvent / FetchCommand enums
                 └─ src/app.rs    AppState — snapshot, cursors, overlays, pod history
                      └─ src/ui/  ratatui render pass (dashboard → node/pod detail views)
```

Polls set `ClusterSnapshot::metrics_sampled`; watch-driven snapshots reuse the last poll's `kubectl top` output and errors, so per-sample history (pod sparklines, desktop score trend) only advances when it is set. Watches are tagged with a generation so updates from a previous namespace are dropped, and live in a `JoinSet` whose drop kills the kubectl processes (`kill_on_drop`). The desktop backend forces `watch = false`.

The fetcher runs on its own tokio task. The main loop uses `tokio::select!` across the terminal event stream, a tick timer, and the mpsc receiver. There is no shared mutable state between tasks — everything flows through channels.

### Key modules

- **`src/data/collector.rs`** — All kubectl I/O lives here. `ensure_readonly_kubectl_args()` enforces a whitelist (get, top, logs, config); mutation verbs are blocked. `run_cmd` is the single choke point: it applies the `with_context` override, expands `-n *` (`ALL_NAMESPACES`) to `--all-namespaces`, and holds a global semaphore capping concurrent kubectl processes at 48. Errors are classified into `ConnectionIssueKind` variants (KubectlMissing, NoContext, NamespaceUnavailable, Generic).

- **`src/data/models.rs`** — All shared types. Threshold constants (`RESOURCE_PRESSURE_PCT`, `GRADE_*_THRESHOLD`) are defined here and imported by both the data and UI layers to stay in sync.

- **`src/data/health.rs`** / **`incidents.rs`** take a `ClusterSignals` (nodes, pods, events, resource problems); add new signal sources there rather than new parameters.

- **`src/data/health.rs`** — Calculates a 0–100 health score from capped, share-based penalties (e.g. failing pods: up to −30 at 10% of pods; unhealthy nodes: up to −30 at 25% of nodes; each category has a small floor so single failures stay visible) and maps it to an A–F grade. Completed and evicted pods are ignored.

- **`src/data/incidents.rs`** — Buckets raw events/pod states into ranked `IncidentBucket` structs by reason and severity.

- **`src/app.rs`** — `AppState` owns the current `ClusterSnapshot`, cursor positions, pod sort mode, the `Overlay` enum (which popup is open), and the active view (`AppView::Dashboard` | `PodDetail` | `NodeDetail`).

- **`src/events/handler.rs`** — Translates crossterm key events and incoming `DataEvent`s into mutations on `AppState`. Overlay transitions and cursor clamping live here.

- **`src/ui/theme.rs`** — Color palette, `gradient_bar()` and `health_bar()` sparkline helpers, `heat_color()` for percentage cells.

- **`src/config.rs`** — CLI args (clap) → TOML file (`~/.config/cluster/config.toml`). Fields: namespace, refresh_interval_secs, node_pool_filter, cluster_name, resource_group, watch (default true, `--no-watch`), crd_checks. New fields need `#[serde(default)]`: a config file that fails to parse silently falls back to defaults.

- **`src/updater.rs`** — Checks GitHub releases API via reqwest; read-only, no auto-update.

### Overlay state machine

UI popups are managed by a single `Overlay` enum on `AppState` (not booleans). Valid values: `None`, `WorkloadPopup`, `NamespaceList`, `NamespaceInput`, `RefreshInput`, `ExportInput`, `PodFilter`. Dashboard rendering and key handler both `match app.overlay`.

### Release builds

The release profile uses `lto=true`, `strip=true`, `codegen-units=1`, `panic=abort`, `opt-level="z"`. Cross-platform binaries (Linux x86_64, macOS ARM64, Windows x86_64) are built and published via `.github/workflows/release.yml` when a `v*.*.*` tag is pushed.
