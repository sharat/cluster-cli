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

use app::AppState;
use config::Config;
use data::fetcher::Fetcher;
use data::models::{ConnectionIssue, ConnectionIssueKind};
use events::{AppEvent, FetchCommand};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "cluster")]
#[command(about = "A fast, interactive terminal UI for monitoring Kubernetes cluster health")]
#[command(version = VERSION)]
struct Cli {}

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();
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

    // Channels
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let (fetch_tx, fetch_rx) = mpsc::unbounded_channel::<FetchCommand>();

    let mut initial_connection_issue = None;

    // Resolve namespace from kubectl context if not explicitly configured
    if config.namespace.is_empty() {
        match data::collector::fetch_current_namespace().await {
            Ok(ns) if !ns.trim().is_empty() => {
                config.namespace = ns;
            }
            Ok(_) => {
                initial_connection_issue = Some(ConnectionIssue {
                    kind: ConnectionIssueKind::NamespaceUnavailable,
                    namespace: None,
                    detail: "No namespace is configured in the current kubectl context."
                        .to_string(),
                });
            }
            Err(err) => {
                initial_connection_issue =
                    data::collector::classify_kubectl_error(&err).or(Some(ConnectionIssue {
                        kind: ConnectionIssueKind::NamespaceUnavailable,
                        namespace: None,
                        detail: "No namespace is configured in the current kubectl context."
                            .to_string(),
                    }));
            }
        }
    }

    // Start background fetcher
    let fetcher = Fetcher::new(config.clone(), event_tx);
    tokio::spawn(fetcher.run(fetch_rx));

    // Trigger initial data fetch only when the namespace is usable.
    if !config.namespace.is_empty() {
        let _ = fetch_tx.send(FetchCommand::RefreshAll {
            namespace: config.namespace.clone(),
        });
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
    mut event_rx: mpsc::UnboundedReceiver<AppEvent>,
    fetch_tx: mpsc::UnboundedSender<FetchCommand>,
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
                                    let _ = fetch_tx.send(fc);
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
