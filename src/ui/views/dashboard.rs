use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{AppState, Overlay};
use crate::data::models::{EventType, HealthStatus, WorkloadSummary};
use crate::ui::{components, theme};

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    if app.has_blocking_connection_issue() {
        render_connection_blocker(f, area, app);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // header
            Constraint::Length(2),  // health + namespace resources
            Constraint::Length(10), // nodes + workloads + events
            Constraint::Fill(1),    // pods
            Constraint::Length(1),  // status bar
        ])
        .split(area);

    render_header(f, chunks[0], app);
    components::health_score::render(f, chunks[1], app);

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(chunks[2]);

    components::node_panel::render(f, middle[0], app);
    components::events_panel::render(f, middle[1], app);

    components::pod_table::render(f, chunks[3], app);
    components::status_bar::render(f, chunks[4], app);

    match app.overlay {
        Overlay::NamespaceInput => render_ns_popup(f, area, app),
        Overlay::NamespaceList => render_ns_list_popup(f, area, app),
        Overlay::RefreshInput => render_refresh_popup(f, area, app),
        Overlay::WorkloadPopup => render_workload_popup(f, area, app),
        Overlay::ExportInput => render_export_popup(f, area, app),
        Overlay::EventDetail => components::event_detail_overlay::render(f, area, app),
        _ => {}
    }
}

fn render_connection_blocker(f: &mut Frame, area: Rect, app: &AppState) {
    let popup_w = area.width.saturating_sub(10).clamp(60, 100);
    let popup_h = 9u16;
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w.min(area.width), popup_h.min(area.height));

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Connect to kubectl ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let message = app
        .blocking_connection_message()
        .unwrap_or_else(|| "Unable to connect to the cluster.".to_string());

    let lines = vec![
        Line::from(vec![Span::styled(
            "Connection required",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(message),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Actions: ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "n",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" choose namespace  "),
            Span::styled(
                "N",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" enter namespace  "),
            Span::styled(
                "r",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" retry"),
        ]),
    ];

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_workload_popup(f: &mut Frame, area: Rect, app: &AppState) {
    let popup_w = area.width.saturating_sub(12).clamp(72, 120);
    let popup_h = area.height.saturating_sub(6).clamp(14, 28);
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Workloads [j/k] navigate [w/Esc] close ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(inner);

    let workloads = app
        .snapshot
        .as_ref()
        .map(|s| s.workloads.as_slice())
        .unwrap_or(&[]);

    let items: Vec<ListItem> = workloads.iter().map(workload_list_item).collect();
    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if !workloads.is_empty() {
        let bounded_cursor = app.workload_cursor.min(workloads.len().saturating_sub(1));
        state.select(Some(bounded_cursor));
    }
    f.render_stateful_widget(list, columns[0], &mut state);

    let detail = workloads
        .get(app.workload_cursor.min(workloads.len().saturating_sub(1)))
        .map(workload_detail_lines)
        .unwrap_or_else(|| vec![Line::from("No workload data available.")]);

    let detail_block = Block::default().borders(Borders::LEFT);
    f.render_widget(
        Paragraph::new(detail)
            .block(detail_block)
            .wrap(Wrap { trim: true }),
        columns[1],
    );
}

fn workload_list_item(workload: &WorkloadSummary) -> ListItem<'static> {
    let status_style = health_style(workload.status.clone());
    ListItem::new(vec![
        Line::from(vec![
            Span::styled(
                format!("{:<4}", workload.kind.short_label()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(workload.name.clone(), status_style),
        ]),
        Line::from(vec![Span::styled(
            format!(
                "ready {}/{}  avail {}",
                workload.ready_replicas, workload.desired_replicas, workload.available_replicas
            ),
            Style::default().fg(Color::Gray),
        )]),
    ])
}

fn workload_detail_lines(workload: &WorkloadSummary) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", workload.kind.short_label()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                workload.name.clone(),
                health_style(workload.status.clone()).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![Span::styled(
            format!("namespace {}", workload.namespace),
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Replicas  ", Style::default().fg(Color::White)),
            Span::raw(format!(
                "ready {}/{}   available {}   unavailable {}",
                workload.ready_replicas,
                workload.desired_replicas,
                workload.available_replicas,
                workload.unavailable_pods
            )),
        ]),
    ];

    if let Some(updated) = workload.updated_replicas {
        lines.push(Line::from(vec![
            Span::styled("Updated   ", Style::default().fg(Color::White)),
            Span::raw(updated.to_string()),
        ]));
    }
    if let Some(current) = workload.current_replicas {
        lines.push(Line::from(vec![
            Span::styled("Current   ", Style::default().fg(Color::White)),
            Span::raw(current.to_string()),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Rollout   ", Style::default().fg(Color::White)),
        Span::styled(
            workload.rollout_status.clone(),
            Style::default().fg(Color::Gray),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Recent events",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]));

    if workload.recent_events.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No recent workload events.",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for event in workload.recent_events.iter().take(6) {
            let event_color = match event.event_type {
                EventType::Warning => Color::Yellow,
                EventType::Normal => Color::DarkGray,
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<10}", event.reason),
                    Style::default()
                        .fg(event_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" x{} ", event.count)),
                Span::styled(event.message.clone(), Style::default().fg(Color::White)),
            ]));
        }
    }

    lines
}

fn health_style(status: HealthStatus) -> Style {
    match status {
        HealthStatus::Critical => Style::default().fg(Color::Red),
        HealthStatus::Warning => Style::default().fg(Color::Yellow),
        HealthStatus::Elevated => Style::default().fg(Color::LightYellow),
        HealthStatus::Healthy => Style::default().fg(Color::Green),
    }
}

fn render_ns_popup(f: &mut Frame, area: Rect, app: &AppState) {
    let popup_w = 52u16;
    let popup_h = 3u16;
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w.min(area.width), popup_h.min(area.height));

    f.render_widget(Clear, popup_area);
    f.render_widget(
        Block::default()
            .title(" Switch Namespace (Enter to apply, Esc to cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
        popup_area,
    );

    let inner = Rect::new(popup_area.x + 1, popup_area.y + 1, popup_area.width - 2, 1);
    let text = format!("{}_", app.ns_input);
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::White)),
        inner,
    );
}

fn render_ns_list_popup(f: &mut Frame, area: Rect, app: &AppState) {
    let popup_w = 60u16;
    let popup_h = (app.ns_list.len() as u16 + 2).clamp(5, 20);
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w.min(area.width), popup_h.min(area.height));

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Select Namespace (↑/↓ to navigate, Enter to select, Esc to cancel) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    f.render_widget(block.clone(), popup_area);

    let inner = Rect::new(
        popup_area.x + 1,
        popup_area.y + 1,
        popup_area.width - 2,
        popup_area.height - 2,
    );

    let items: Vec<ListItem> = app
        .ns_list
        .iter()
        .enumerate()
        .map(|(i, ns)| {
            let (style, prefix) = if i == app.ns_list_cursor {
                if ns == &app.config.namespace {
                    (
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                        "● ",
                    )
                } else {
                    (
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                        "  ",
                    )
                }
            } else if ns == &app.config.namespace {
                (
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                    "● ",
                )
            } else {
                (Style::default().fg(Color::Gray), "  ")
            };
            ListItem::new(format!("{prefix}{ns}")).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default());

    let mut state = ListState::default();
    if !app.ns_list.is_empty() {
        state.select(Some(app.ns_list_cursor));
    }

    f.render_stateful_widget(list, inner, &mut state);
}

fn render_refresh_popup(f: &mut Frame, area: Rect, app: &AppState) {
    let popup_w = 54u16;
    let popup_h = 3u16;
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w.min(area.width), popup_h.min(area.height));

    f.render_widget(Clear, popup_area);
    f.render_widget(
        Block::default()
            .title(" Refresh Interval in Seconds (Enter to apply, Esc to cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
        popup_area,
    );

    let inner = Rect::new(popup_area.x + 1, popup_area.y + 1, popup_area.width - 2, 1);
    let text = format!("{}_", app.refresh_input);
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::White)),
        inner,
    );
}

fn render_export_popup(f: &mut Frame, area: Rect, app: &AppState) {
    let popup_w = 70u16;
    let popup_h = 3u16;
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w.min(area.width), popup_h.min(area.height));

    f.render_widget(Clear, popup_area);
    f.render_widget(
        Block::default()
            .title(" Export Pod Status to CSV (Enter to save, Esc to cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
        popup_area,
    );

    let inner = Rect::new(popup_area.x + 1, popup_area.y + 1, popup_area.width - 2, 1);
    let text = format!("{}_", app.export_input);
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::White)),
        inner,
    );
}

fn render_header(f: &mut Frame, area: Rect, app: &AppState) {
    let now = chrono::Local::now();
    let time = now.format("%H:%M:%S").to_string();

    let refresh_info = if let Some(snap) = &app.snapshot {
        let elapsed = snap.fetched_at.elapsed().as_secs();
        format!(
            "  Last refresh: {}s ago  Every: {}s  [r]efresh [R]ate",
            elapsed, app.config.refresh_interval_secs
        )
    } else {
        format!(
            "  Connecting...  Every: {}s  [R]ate",
            app.config.refresh_interval_secs
        )
    };

    let ns = &app.config.namespace;

    let mut spans = vec![Span::styled(
        " cluster-rs  ",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )];

    let cluster_label = app
        .config
        .cluster_name
        .as_deref()
        .or_else(|| app.snapshot.as_ref()?.context_name.as_deref());
    if let Some(cluster) = cluster_label {
        spans.push(Span::styled(
            format!("{cluster}  "),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::styled(
        format!("ns:{ns}"),
        Style::default().fg(Color::Cyan),
    ));

    spans.push(Span::styled(
        format!("  {time}{refresh_info}  "),
        Style::default().fg(Color::White),
    ));

    let header = Paragraph::new(Line::from(spans));
    f.render_widget(header, area);
}
