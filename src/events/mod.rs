pub mod handler;

use crate::data::models::{ClusterSnapshot, ConnectionIssue};

#[derive(Debug)]
pub enum AppEvent {
    Data(DataEvent),
}

#[derive(Debug)]
pub enum DataEvent {
    Refreshed(ClusterSnapshot),
    LogLine(String),
    Error(String),
    ConnectionState(Option<ConnectionIssue>),
    Namespaces(Vec<String>),
    ExportResult { message: String },
}

#[derive(Debug)]
pub enum FetchCommand {
    RefreshAll {
        namespace: String,
    },
    UpdateRefreshInterval {
        namespace: String,
        interval_secs: u64,
    },
    StartLogStream {
        pod: String,
        namespace: String,
    },
    StopLogStream,
    FetchNamespaces,
    ExportPods {
        cluster_name: Option<String>,
        namespace: String,
        path: String,
    },
}
