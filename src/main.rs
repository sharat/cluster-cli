use anyhow::Result;
use clap::Parser;
use crossterm::{
    cursor::{SetCursorStyle, Show},
    event::{Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::{sync::mpsc, time::interval};

mod app;
mod config;
mod data;
mod events;
mod ui;
mod updater;

use app::AppState;
use config::Config;
use data::fetcher::Fetcher;
use data::models::{ConnectionIssue, ConnectionIssueKind};
use events::{AppEvent, FetchCommand};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Capacity of the fetcher -> UI event channel. Sized to absorb a burst of log
/// lines between renders (the UI drains one event per `select!` iteration).
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Capacity of the UI -> fetcher command channel. Commands are user-driven, so
/// this only needs to cover a burst of held keypresses.
const FETCH_CHANNEL_CAPACITY: usize = 64;

#[derive(Parser)]
#[command(name = "cluster")]
#[command(about = "A fast, interactive terminal UI for monitoring Kubernetes cluster health")]
#[command(version = VERSION)]
struct Cli {
    /// Upgrade cluster-cli in place using the method it was installed with
    #[arg(long)]
    upgrade: bool,

    /// Check whether a newer release is available, without installing it
    #[arg(long)]
    check_update: bool,

    /// Show version, detected install method, and config directory
    #[arg(long)]
    info: bool,
}

/// Handle `--info`, `--check-update` and `--upgrade`. Each runs to completion
/// and the process exits; none of them start the TUI.
async fn run_updater_flags(cli: &Cli) -> Result<()> {
    let up = updater::Updater::new(VERSION);

    if cli.info {
        up.show_info();
    }

    // `--upgrade` implies the check, so don't perform it twice.
    if cli.check_update && !cli.upgrade {
        match up.check_update().await? {
            Some(release) => println!("{}", up.get_update_notification(&release)),
            None => println!("cluster-cli {VERSION} is up to date."),
        }
    }

    if cli.upgrade {
        match up.check_update().await? {
            Some(release) => {
                println!("Updating {VERSION} → {}", release.tag_name);
                up.upgrade().await?;
            }
            None => println!("cluster-cli {VERSION} is already up to date — nothing to do."),
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // These subcommand-ish flags are terminal operations: handle them and exit
    // before any TUI/raw-mode setup, so their output goes to a normal terminal.
    if cli.upgrade || cli.check_update || cli.info {
        return run_updater_flags(&cli).await;
    }

    let mut config = Config::load()?;

    tracing_subscriber::fmt().with_writer(io::stderr).init();

    // Restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            SetCursorStyle::DefaultUserShape,
            Show,
            LeaveAlternateScreen
        );
        original_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Channels. Both are bounded so a slow UI can't let a fast producer (log
    // streaming in particular) grow the queue without limit. The fetcher awaits
    // on `event_tx`, so a backed-up UI applies backpressure to log streaming.
    // Command sends await available capacity so commands such as exports and
    // stopping log streams are never silently lost.
    let (event_tx, event_rx) = mpsc::channel::<AppEvent>(EVENT_CHANNEL_CAPACITY);
    let (fetch_tx, fetch_rx) = mpsc::channel::<FetchCommand>(FETCH_CHANNEL_CAPACITY);

    let mut initial_connection_issue = None;

    // Resolve namespace from kubectl context if not explicitly configured.
    // Contexts provisioned by cloud CLIs (az aks get-credentials, aws eks update-kubeconfig,
    // gcloud container clusters get-credentials) and local tools (minikube, colima,
    // docker-desktop, k3d, kind) rarely set a default namespace — fall back to "default".
    if config.namespace.is_empty() {
        match data::collector::fetch_current_namespace().await {
            Ok(ns) if !ns.trim().is_empty() => {
                config.namespace = ns;
            }
            Ok(_) => {
                // No namespace in context — use "default" so the app works out of the box.
                config.namespace = "default".to_string();
            }
            Err(err) => {
                // Only block on hard failures: kubectl not installed or no context configured.
                // Everything else falls back to "default" so we can still attempt a fetch.
                match data::collector::classify_kubectl_error(&err) {
                    Some(issue)
                        if matches!(
                            issue.kind,
                            ConnectionIssueKind::KubectlMissing | ConnectionIssueKind::NoContext
                        ) =>
                    {
                        initial_connection_issue = Some(issue);
                    }
                    _ => {
                        config.namespace = "default".to_string();
                    }
                }
            }
        }
    }

    // Start background fetcher
    let fetcher = Fetcher::new(config.clone(), event_tx);
    tokio::spawn(fetcher.run(fetch_rx));

    // Trigger initial data fetch only when the namespace is usable.
    if !config.namespace.is_empty() {
        fetch_tx
            .send(FetchCommand::RefreshAll {
                namespace: config.namespace.clone(),
            })
            .await?;
    }

    // Run app
    let result = run_app(
        &mut terminal,
        event_rx,
        fetch_tx,
        config,
        initial_connection_issue,
    )
    .await;

    // Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut event_rx: mpsc::Receiver<AppEvent>,
    fetch_tx: mpsc::Sender<FetchCommand>,
    config: Config,
    initial_connection_issue: Option<ConnectionIssue>,
) -> Result<()> {
    let mut app = AppState::new(config);
    app.connection_issue = initial_connection_issue;
    if app.connection_issue.is_some() {
        app.is_loading = false;
    }
    let mut crossterm_events = EventStream::new();
    let mut tick = interval(Duration::from_millis(250));
    let mut wait_cursor_active = false;

    loop {
        let should_show_wait_cursor = app.is_connecting_to_cluster();
        if should_show_wait_cursor != wait_cursor_active {
            if should_show_wait_cursor {
                terminal.show_cursor()?;
                execute!(terminal.backend_mut(), SetCursorStyle::BlinkingBlock)?;
            } else {
                terminal.hide_cursor()?;
                execute!(terminal.backend_mut(), SetCursorStyle::DefaultUserShape)?;
            }
            wait_cursor_active = should_show_wait_cursor;
        }

        terminal.draw(|f| ui::render(f, &mut app))?;

        tokio::select! {
            _ = tick.tick() => {
                // Periodic render (updates elapsed time display)
            }

            Some(Ok(event)) = crossterm_events.next() => {
                match event {
                    Event::Key(key) => {
                        if let Some(cmd) = events::handler::handle_key(&mut app, key) {
                            match cmd {
                                events::handler::AppCommand::Quit => return Ok(()),
                                events::handler::AppCommand::Fetch(fc) => {
                                    // Backpressure preserves one-shot commands
                                    // (export and log controls) instead of
                                    // silently discarding them when the queue is full.
                                    fetch_tx.send(fc).await?;
                                }
                            }
                        }
                    }
                    Event::Resize(_, _) => {
                        terminal.autoresize()?;
                    }
                    _ => {}
                }
            }

            Some(app_event) = event_rx.recv() => {
                match app_event {
                    AppEvent::Data(data_event) => {
                        events::handler::handle_data_event(&mut app, data_event);
                    }
                }
            }
        }
    }
}
