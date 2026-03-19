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

# Linting (warnings are errors in CI)
cargo clippy -- -D warnings        # lint with warnings-as-errors

# Update dependencies
cargo update                       # update to latest compatible versions
```

## Project Overview

cluster-cli is a read-only Kubernetes TUI built on ratatui + crossterm with a tokio async runtime. It communicates with kubectl via subprocesses (30s timeout, read-only whitelist enforced).

### Data Flow
```
kubectl subprocess
  └─ src/data/collector.rs        fetch_node_metrics / fetch_pod_info / fetch_events / fetch_workload_summaries
       └─ src/data/fetcher.rs     background task: tokio::join! 5 concurrent fetches, sends AppEvent over mpsc
            └─ src/events/mod.rs  AppEvent / DataEvent / FetchCommand enums
                 └─ src/app.rs    AppState — snapshot, cursors, overlays, pod history
                      └─ src/ui/  ratatui render pass (dashboard → node/pod detail views)
```

The fetcher runs on its own tokio task. The main loop uses `tokio::select!` across the terminal event stream, a tick timer, and the mpsc receiver. There is no shared mutable state between tasks — everything flows through channels.

### Key Modules
- **`src/data/collector.rs`** — All kubectl I/O. `ensure_readonly_kubectl_args()` enforces a whitelist (get, top, logs, config); mutation verbs are blocked.
- **`src/data/models.rs`** — All shared types. Threshold constants (`RESOURCE_PRESSURE_PCT`, `GRADE_*_THRESHOLD`) defined here.
- **`src/data/health.rs`** — Calculates 0–100 health score with weighted penalties, maps to A–F grade.
- **`src/data/incidents.rs`** — Buckets raw events/pod states into ranked `IncidentBucket` structs.
- **`src/app.rs`** — `AppState` owns `ClusterSnapshot`, cursors, `Overlay` enum, and `AppView`.
- **`src/events/handler.rs`** — Translates crossterm key events and `DataEvent`s into `AppState` mutations.
- **`src/ui/theme.rs`** — Color palette, `gradient_bar()`, `health_bar()`, `heat_color()` helpers.
- **`src/config.rs`** — CLI args (clap) → TOML file (`~/.config/cluster/config.toml`).
- **`src/updater.rs`** — Checks GitHub releases API via reqwest; read-only.

### Overlay State Machine
UI popups managed by single `Overlay` enum (not booleans): `None`, `WorkloadPopup`, `NamespaceList`, `NamespaceInput`, `RefreshInput`, `ExportInput`, `PodFilter`. Both dashboard rendering and key handler `match app.overlay`.

## Code Style Guidelines

### Imports
Standard library imports first, then external crates, then local modules. Blank line between groups:
```rust
use std::{io, time::Duration};           // std first
use anyhow::Result;                       // external crates
use tokio::{sync::mpsc, time::interval};  // more externals

mod app;                                  // local modules
mod config;
use app::AppState;                        // local imports
```

### Naming Conventions
- Types / traits / enums: `PascalCase`
- Struct fields / enum variants: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Variables / functions: `snake_case`

### Enum Derives
Always derive at minimum:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Critical,
    Warning,
    Elevated,
    Healthy,
}
```
Add `Copy` when the enum is cheap to copy (unit-like variants, or small enums with no heap allocations). Add `Eq` when appropriate (usually when `PartialEq` is `Eq`).

### Struct Naming
- `NodeMetric`, `PodInfo`, `ContainerInfo` — data models
- `ClusterSnapshot`, `HealthScore`, `ResourceUsageSummary` — composite/aggregate types
- `IncidentBucket`, `WorkloadSummary` — bucketed/aggregated data

### Error Handling
- Use `anyhow::Result` for application-level error handling
- Use `?` operator consistently
- Errors should propagate up channels when relevant (e.g., `ConnectionIssueKind` variants)

### Struct Initialization
When constructing structs with many fields, use field init syntax. For tests, helper builder functions are common:
```rust
fn create_test_node(memory_pct: u8, ready: bool, unhealthy_conditions: u8) -> NodeMetric {
    NodeMetric {
        name: "test-node".to_string(),
        memory_pct,
        ready,
        unhealthy_conditions,
        // ... other fields with defaults
    }
}
```

### Async / Concurrency
- Use `tokio::select!` for multiplexing multiple async operations
- Use `mpsc::unbounded_channel` for task communication
- No shared mutable state between tasks — everything flows through channels

### UI Rendering
- All UI code lives in `src/ui/`
- Render functions take `&mut Frame` and `&mut AppState`
- Theme helpers in `src/ui/theme.rs`: `gradient_bar()`, `health_bar()`, `heat_color()`

### Testing
- Tests live in `src/data/tests/` as a submodule
- Use `#[cfg(test)]` and `use crate::...` for internal imports
- Test names: `test_<description>` (snake_case)

### Code Review Checklist
- [ ] All new structs/enums have appropriate derives
- [ ] No mutation shared across task boundaries
- [ ] Error handling uses `?` operator
- [ ] Constants used instead of magic numbers
- [ ] Format: `cargo fmt -- --check` passes
- [ ] Lint: `cargo clippy -- -D warnings` passes
