use anyhow::Result;
use crossterm::{
    cursor::{SetCursorStyle, Show},
    event::{Event, EventStream, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::{sync::mpsc, time::interval};

mod app;
mod cli;
mod config;
mod data;
mod events;
mod headless;
mod json_output;
mod mcp;
mod ui;
mod updater;

use app::AppState;
use cli::{Cli, Commands};
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

/// Restores the terminal when dropped, so early `?` returns after raw mode is
/// enabled don't leave the user's shell in raw mode on the alternate screen.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
    }
}

/// Handle installer metadata, `--info`, `--check-update` and `--upgrade`.
/// Each runs to completion and exits without starting the TUI.
async fn run_updater_flags(cli: &Cli) -> Result<()> {
    if let Some(source) = cli.record_install_method.as_deref() {
        let method = updater::InstallMethod::from_str(source)
            .filter(|method| *method != updater::InstallMethod::Unknown)
            .ok_or_else(|| anyhow::anyhow!("Unsupported installation method: {source}"))?;
        updater::Updater::store_install_method(method)?;
        println!("Installation method recorded: {}", method.as_str());
    }

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
    use clap::Parser;
    let cli = Cli::parse();

    // Handle non-TUI commands first
    if cli.upgrade || cli.check_update || cli.info || cli.record_install_method.is_some() {
        return run_updater_flags(&cli).await;
    }

    // Handle JSON/headless subcommands
    if let Some(ref command) = cli.command {
        let config = Config::load(cli.config.clone())?;

        match command {
            Commands::Status { json, verbose, kubectl_opts } => {
                return headless::handle_status(kubectl_opts, &config, *json, *verbose).await;
            }
            Commands::Incidents { json, verbose, kubectl_opts } => {
                return headless::handle_incidents(kubectl_opts, &config, *json, *verbose).await;
            }
            Commands::Pods { json, verbose, kubectl_opts } => {
                return headless::handle_pods(kubectl_opts, &config, *json, *verbose).await;
            }
            Commands::Nodes { json, verbose, kubectl_opts } => {
                return headless::handle_nodes(kubectl_opts, &config, *json, *verbose).await;
            }
            Commands::Logs { pod, container, tail, follow, timestamps, kubectl_opts } => {
                return headless::handle_logs(
                    kubectl_opts,
                    &config,
                    pod,
                    container.as_deref(),
                    *tail,
                    *follow,
                    *timestamps,
                )
                .await;
            }
            Commands::Mcp => {
                return mcp::run_mcp_server(config).await;
            }
        }
    }

    // Everything below is the TUI path
    let mut config = Config::load(cli.config)?;

    // Logging is opt-in via RUST_LOG. stderr shares the terminal with the TUI,
    // so unconditional logging would draw over the alternate screen.
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(filter).with_writer(io::stderr).init();
    }

    // Restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ =
            execute!(io::stdout(), SetCursorStyle::DefaultUserShape, Show, LeaveAlternateScreen);
        original_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let _terminal_guard = TerminalGuard;
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
        fetch_tx.send(FetchCommand::RefreshAll { namespace: config.namespace.clone() }).await?;
    }

    // Run app
    let result = run_app(&mut terminal, event_rx, fetch_tx, config, initial_connection_issue).await;

    // Terminal is restored by `_terminal_guard` on drop.
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
                // Periodic render updates elapsed time and the loading animation.
                if app.is_loading || app.is_loading_namespaces {
                    app.loading_animation_frame = app.loading_animation_frame.wrapping_add(1);
                }
            }

            Some(Ok(event)) = crossterm_events.next() => {
                match event {
                    // Windows reports both press and release; act on presses only.
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
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
