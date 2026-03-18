use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{AppState, AppView, PodDetailSection};
use crate::data::models::{ContainerInfo, EventType, PodInfo};
use crate::ui::{components, format, theme};
use std::collections::HashMap;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let pod_name = match &app.view {
        AppView::PodDetail { pod_name } => pod_name.clone(),
        _ => return,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(12), // overview
            Constraint::Length(8), // events
            Constraint::Fill(1),  // logs
            Constraint::Length(1), // status bar
        ])
        .split(area);

    render_header(f, chunks[0], app, &pod_name);
    render_overview(f, chunks[1], app);
    render_events(f, chunks[2], app, &pod_name);
    components::log_viewer::render(f, chunks[3], app);
    components::status_bar::render(f, chunks[4], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &AppState, pod_name: &str) {
    let status = app
        .current_pod()
        .map(|p| {
            let icon = theme::status_icon(&p.status);
            format!(" {}  {}", icon, p.phase)
        })
        .unwrap_or_default();

    let text = format!(" [Esc] Back  |  {}  |{}", pod_name, status);

    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        area,
    );
}

fn render_overview(f: &mut Frame, area: Rect, app: &AppState) {
    let is_focused = app.pod_detail_section == PodDetailSection::Overview;
    let border_style = if is_focused {
        theme::focused_border_style()
    } else {
        theme::normal_border_style()
    };

    let block = Block::default()
        .title(" Overview ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(pod) = app.current_pod() {
        let cpu_history = app.get_pod_cpu_history(pod);
        let memory_history = app.get_pod_memory_history(pod);
        let probe_failures = container_probe_failures(app, pod);
        let mut lines = vec![
            Line::from(format!(
                "  Status: {:10}  Node: {}  Age: {}  Restarts: {}",
                pod.phase,
                pod.node_name.as_deref().unwrap_or("unknown"),
                pod.age,
                pod.restarts
            )),
            Line::from(""),
            metric_line(
                "CPU",
                pod.cpu_pct,
                format!(
                    "use {}  req {}  lim {}",
                    format::cpu(pod.cpu_millicores),
                    format::cpu(pod.cpu_request_millicores),
                    format::cpu(pod.cpu_limit_millicores),
                ),
            ),
            history_line("CPU History", cpu_history),
            metric_line(
                "Mem",
                pod.memory_pct,
                format!(
                    "use {}  req {}  lim {}",
                    format::memory(pod.memory_mb),
                    format::memory(pod.memory_request_mb),
                    format::memory(pod.memory_limit_mb),
                ),
            ),
            history_line("Mem History", memory_history),
            Line::from(""),
            Line::styled(
                "  Containers:",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ];

        let available_lines = inner.height.saturating_sub(lines.len() as u16) as usize;
        let visible_container_count = available_lines.min(pod.containers.len());
        for container in pod.containers.iter().take(visible_container_count) {
            let (readiness_failures, liveness_failures) = probe_failures
                .get(&container.name)
                .copied()
                .unwrap_or((0, 0));
            lines.push(container_line(
                container,
                readiness_failures,
                liveness_failures,
            ));
        }
        if pod.containers.len() > visible_container_count {
            lines.push(Line::styled(
                format!(
                    "  ... {} more container(s) not shown",
                    pod.containers.len() - visible_container_count
                ),
                Style::default().fg(Color::DarkGray),
            ));
        }
        if pod.containers.is_empty() {
            lines.push(Line::styled(
                "  No container status data available.",
                Style::default().fg(Color::DarkGray),
            ));
        }

        f.render_widget(Paragraph::new(lines), inner);
    } else {
        f.render_widget(
            Paragraph::new("  Pod not found in current snapshot."),
            inner,
        );
    }
}

fn container_probe_failures(app: &AppState, pod: &PodInfo) -> HashMap<String, (u32, u32)> {
    let mut failures = HashMap::new();
    let Some(snapshot) = &app.snapshot else {
        return failures;
    };

    for event in snapshot
        .events
        .iter()
        .filter(|e| e.name == pod.name || pod.name.starts_with(&e.name))
    {
        if event.reason != "Unhealthy" {
            continue;
        }

        let message = event.message.to_lowercase();
        let is_readiness = message.contains("readiness probe failed");
        let is_liveness = message.contains("liveness probe failed");
        if !is_readiness && !is_liveness {
            continue;
        }

        let Some(container_name) = infer_container_name(&event.message, pod) else {
            continue;
        };

        let entry = failures.entry(container_name).or_insert((0, 0));
        let count = event.count.max(1);
        if is_readiness {
            entry.0 += count;
        }
        if is_liveness {
            entry.1 += count;
        }
    }

    failures
}

fn infer_container_name(message: &str, pod: &PodInfo) -> Option<String> {
    let mut matches = pod
        .containers
        .iter()
        .filter(|container| message.contains(&container.name))
        .map(|container| container.name.clone());

    let first = matches.next();
    if first.is_some() && matches.next().is_none() {
        return first;
    }

    if pod.containers.len() == 1 {
        return pod.containers.first().map(|container| container.name.clone());
    }

    None
}

fn container_line(
    container: &ContainerInfo,
    readiness_failures: u32,
    liveness_failures: u32,
) -> Line<'static> {
    let readiness_style = if container.ready {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let last_reason = container
        .last_termination_reason
        .clone()
        .unwrap_or_else(|| "-".to_string());
    let last_exit = container
        .last_exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "-".to_string());

    Line::from(vec![
        Span::raw(format!("  {:<16}", truncate_label(&container.name, 16))),
        Span::styled(
            format!("{:<6}  ", if container.ready { "ready" } else { "not" }),
            readiness_style.add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("{:<22}", truncate_label(&container.state, 22))),
        Span::styled(
            format!("r:{}  ", container.restart_count),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!("last:{}/{}  ", last_reason, last_exit),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!("probe R:{} L:{}", readiness_failures, liveness_failures),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn truncate_label(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }

    let mut truncated: String = value.chars().take(width.saturating_sub(1)).collect();
    truncated.push('~');
    truncated
}

fn render_events(f: &mut Frame, area: Rect, app: &AppState, pod_name: &str) {
    let is_focused = app.pod_detail_section == PodDetailSection::Events;
    let border_style = if is_focused {
        theme::focused_border_style()
    } else {
        theme::normal_border_style()
    };

    let block = Block::default()
        .title(" Events ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(snap) = &app.snapshot {
        let pod_events: Vec<_> = snap
            .events
            .iter()
            .filter(|e| e.name == pod_name || e.name.starts_with(pod_name))
            .collect();

        let lines: Vec<Line> = if pod_events.is_empty() {
            vec![Line::styled(
                "  No events for this pod.",
                Style::default().fg(Color::DarkGray),
            )]
        } else {
            pod_events
                .iter()
                .map(|e| {
                    let ts = if e.timestamp.len() >= 19 {
                        &e.timestamp[11..19]
                    } else {
                        e.timestamp.as_str()
                    };
                    let style = if e.event_type == EventType::Warning {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    Line::styled(
                        format!("  {}  {:12}  {}", ts, e.reason, e.message),
                        style,
                    )
                })
                .collect()
        };

        let scroll = if is_focused { app.detail_scroll as u16 } else { 0 };
        f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
    }
}

fn metric_line(label: &str, pct: u8, detail: String) -> Line<'static> {
    let mut spans = vec![
        Span::raw(format!("  {}:  ", label)),
        Span::styled(
            format!("{:>3}%  ", pct),
            Style::default()
                .fg(theme::heat_color(pct))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{}  ", detail), Style::default().fg(Color::White)),
    ];
    spans.extend(theme::gradient_bar(pct, 24).spans);
    Line::from(spans)
}

fn history_line(label: &str, history: Option<&Vec<u8>>) -> Line<'static> {
    let mut spans = vec![
        Span::raw(format!("  {}: ", label)),
        Span::styled("recent ", Style::default().fg(Color::DarkGray)),
    ];
    spans.extend(theme::sparkline(history, 24).spans);
    Line::from(spans)
}
