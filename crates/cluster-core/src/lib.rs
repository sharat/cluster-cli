//! Front-end–agnostic core of cluster-cli: kubectl collection (read-only),
//! health scoring, incident ranking, config, and the fetcher task that feeds
//! snapshots to a UI over channels.

pub mod config;
pub mod data;
pub mod events;
