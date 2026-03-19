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

## Architecture

cluster-cli is a read-only Kubernetes TUI built on ratatui + crossterm with a tokio async runtime.

### Data flow

```
kubectl subprocess (30s timeout, read-only whitelist enforced)
  └─ src/data/collector.rs        fetch_node_metrics / fetch_pod_info / fetch_events / fetch_workload_summaries
       └─ src/data/fetcher.rs     background task: tokio::join! 5 concurrent fetches, sends AppEvent over mpsc
            └─ src/events/mod.rs  AppEvent / DataEvent / FetchCommand enums
                 └─ src/app.rs    AppState — snapshot, cursors, overlays, pod history
                      └─ src/ui/  ratatui render pass (dashboard → node/pod detail views)
```

The fetcher runs on its own tokio task. The main loop uses `tokio::select!` across the terminal event stream, a tick timer, and the mpsc receiver. There is no shared mutable state between tasks — everything flows through channels.

### Key modules

- **`src/data/collector.rs`** — All kubectl I/O lives here. `ensure_readonly_kubectl_args()` enforces a whitelist (get, top, logs, config); mutation verbs are blocked. Errors are classified into `ConnectionIssueKind` variants (KubectlMissing, NoContext, NamespaceUnavailable, Generic).

- **`src/data/models.rs`** — All shared types. Threshold constants (`RESOURCE_PRESSURE_PCT`, `GRADE_*_THRESHOLD`) are defined here and imported by both the data and UI layers to stay in sync.

- **`src/data/health.rs`** — Calculates a 0–100 health score with weighted penalties (crash loop −20, OOM −15, node memory pressure −15, etc.) and maps it to an A–F grade.

- **`src/data/incidents.rs`** — Buckets raw events/pod states into ranked `IncidentBucket` structs by reason and severity.

- **`src/app.rs`** — `AppState` owns the current `ClusterSnapshot`, cursor positions, pod sort mode, the `Overlay` enum (which popup is open), and the active view (`AppView::Dashboard` | `PodDetail` | `NodeDetail`).

- **`src/events/handler.rs`** — Translates crossterm key events and incoming `DataEvent`s into mutations on `AppState`. Overlay transitions and cursor clamping live here.

- **`src/ui/theme.rs`** — Color palette, `gradient_bar()` and `health_bar()` sparkline helpers, `heat_color()` for percentage cells.

- **`src/config.rs`** — CLI args (clap) → TOML file (`~/.config/cluster/config.toml`). Fields: namespace, refresh_interval_secs, node_pool_filter, cluster_name, resource_group.

- **`src/updater.rs`** — Checks GitHub releases API via reqwest; read-only, no auto-update.

### Overlay state machine

UI popups are managed by a single `Overlay` enum on `AppState` (not booleans). Valid values: `None`, `WorkloadPopup`, `NamespaceList`, `NamespaceInput`, `RefreshInput`, `ExportInput`, `PodFilter`. Dashboard rendering and key handler both `match app.overlay`.

### Release builds

The release profile uses `lto=true`, `strip=true`, `codegen-units=1`, `panic=abort`, `opt-level="z"`. Cross-platform binaries (Linux x86_64, macOS ARM64, Windows x86_64) are built and published via `.github/workflows/release.yml` when a `v*.*.*` tag is pushed.
