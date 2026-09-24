//! Bridges the tokio-based cluster-core fetcher into GPUI. One `Fetcher` runs
//! per kubectl context, each pinned to its context with
//! `collector::with_context`, and all of their events are funnelled into a
//! single channel tagged with the cluster's index.

use cluster_core::config::Config;
use cluster_core::data::collector;
use cluster_core::data::fetcher::Fetcher;
use cluster_core::events::{AppEvent, DataEvent, FetchCommand};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

const EVENT_CHANNEL_CAPACITY: usize = 256;
const COMMAND_CHANNEL_CAPACITY: usize = 16;
const STARTUP_STAGGER: std::time::Duration = std::time::Duration::from_millis(250);

pub type TaggedEvent = (usize, DataEvent);

pub struct Backend {
    runtime: Runtime,
    config: Config,
    events_tx: mpsc::UnboundedSender<TaggedEvent>,
}

impl Backend {
    pub fn start(config: Config) -> (Self, mpsc::UnboundedReceiver<TaggedEvent>) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("cluster-fetch")
            .build()
            .expect("failed to start the tokio runtime");
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        (
            Self {
                runtime,
                config,
                events_tx,
            },
            events_rx,
        )
    }

    /// Every context in the kubeconfig. Local and fast, so it blocks.
    pub fn context_names(&self) -> Result<Vec<String>, String> {
        self.runtime
            .block_on(collector::fetch_context_names())
            .map_err(|err| match collector::classify_kubectl_error(&err) {
                Some(issue) => issue.detail,
                None => err.to_string(),
            })
    }

    /// Shows a desktop notification off the UI thread (delivery can block).
    pub fn notify(&self, summary: String, body: String) {
        self.runtime.spawn_blocking(move || {
            if let Err(err) = notify_rust::Notification::new()
                .appname("cluster")
                .summary(&summary)
                .body(&body)
                .show()
            {
                eprintln!("cluster-desktop: notification failed: {err}");
            }
        });
    }

    /// Starts watching `context`; its events arrive tagged with `index`.
    pub fn watch(&self, index: usize, context: String) -> mpsc::Sender<FetchCommand> {
        let (fetch_tx, mut fetch_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);

        let fetcher = Fetcher::new(self.config.clone(), fetch_tx);
        self.runtime
            .spawn(collector::with_context(context, fetcher.run(cmd_rx)));

        let events_tx = self.events_tx.clone();
        self.runtime.spawn(async move {
            while let Some(AppEvent::Data(event)) = fetch_rx.recv().await {
                if events_tx.send((index, event)).is_err() {
                    break;
                }
            }
        });

        // The fetcher's timer only runs once a namespace is resolved, so kick
        // off the first fetch explicitly (empty = the context's default),
        // staggered so a large fleet doesn't start every cluster at once.
        let first_fetch = cmd_tx.clone();
        let namespace = self.config.namespace.clone();
        self.runtime.spawn(async move {
            tokio::time::sleep(STARTUP_STAGGER * index as u32).await;
            let _ = first_fetch
                .send(FetchCommand::RefreshAll { namespace })
                .await;
        });
        cmd_tx
    }
}
