use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{AppState, AppView, PodDetailSection};
use crate::data::diagnosis::{PodFinding, diagnose_pod};
use crate::data::models::{ContainerInfo, EventType, IncidentSeverity, PodInfo};
use crate::ui::{components, format, theme};
use std::collections::HashMap;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let pod_name = match &app.view {
        AppView::PodDetail { pod_name } => pod_name.clone(),
        _ => return,
    };

    let findings = match (app.current_pod(), &app.snapshot) {
        (Some(pod), Some(snapshot)) => diagnose_pod(pod, snapshot),
        _ => Vec::new(),
    };

    let plan = DiagnosisPlan::new(&findings, area.height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),             // header
            Constraint::Length(OVERVIEW_ROWS), // overview
            Constraint::Length(plan.height()), // diagnosis
            Constraint::Length(EVENTS_ROWS),   // events
            Constraint::Fill(1),               // logs
            Constraint::Length(1),             // status bar
        ])
        .split(area);

    render_header(f, chunks[0], app, &pod_name);
    render_overview(f, chunks[1], app);
    render_diagnosis(f, chunks[2], &findings, plan);
    render_events(f, chunks[3], app, &pod_name);
    components::log_viewer::render(f, chunks[4], app);
    components::status_bar::render(f, chunks[5], app);
}

const OVERVIEW_ROWS: u16 = 12;
const EVENTS_ROWS: u16 = 8;
/// Rows the logs panel keeps before the Diagnosis panel may grow into them.
const MIN_LOG_ROWS: u16 = 8;
/// Findings shown in full before the rest are summarized as "... N more".
const MAX_DETAILED_FINDINGS: usize = 3;

/// How many findings the Diagnosis panel shows and whether each gets its
/// cause and command lines. Detail is dropped before the logs panel shrinks
/// below `MIN_LOG_ROWS`, so short terminals get one line per finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiagnosisPlan {
    total: usize,
    visible: usize,
    detailed: bool,
}

impl DiagnosisPlan {
    fn new(findings: &[PodFinding], screen_height: u16) -> Self {
        let total = findings.len();
        let fixed = 1 + OVERVIEW_ROWS + EVENTS_ROWS + 1 + MIN_LOG_ROWS;
        let budget = screen_height.saturating_sub(fixed) as usize;
        let rows = |visible: usize, per_finding: usize| {
            2 + visible * per_finding + usize::from(total > visible)
        };

        let detailed = (1..=total.min(MAX_DETAILED_FINDINGS))
            .rev()
            .find(|&visible| rows(visible, 3) <= budget);
        if let Some(visible) = detailed {
            return Self { total, visible, detailed: true };
        }
        // Compact: one line per finding, but always at least the worst one.
        let visible = (1..=total).rev().find(|&v| rows(v, 1) <= budget).unwrap_or(1).min(total);
        Self { total, visible, detailed: false }
    }

    fn height(self) -> u16 {
        if self.total == 0 {
            return 3;
        }
        let per_finding = if self.detailed { 3 } else { 1 };
        (2 + self.visible * per_finding + usize::from(self.total > self.visible)) as u16
    }
}

fn render_diagnosis(f: &mut Frame, area: Rect, findings: &[PodFinding], plan: DiagnosisPlan) {
    let block = Block::default()
        .title(" Diagnosis ")
        .borders(Borders::ALL)
        .border_style(theme::normal_border_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if findings.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled(
                "  ✓ No problems detected",
                Style::default().fg(Color::Green),
            )),
            inner,
        );
        return;
    }

    let width = inner.width as usize;
    let mut lines = Vec::new();
    for finding in findings.iter().take(plan.visible) {
        let color = match finding.severity {
            IncidentSeverity::Critical => Color::Red,
            IncidentSeverity::Warning => Color::Yellow,
            IncidentSeverity::Elevated => Color::Rgb(255, 165, 0),
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  [{}] ", finding.severity.label().to_uppercase()),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                finding.issue.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]));
        if !plan.detailed {
            continue;
        }
        lines.push(Line::styled(
            fit_width(&format!("      Likely cause: {}", finding.likely_cause), width),
            Style::default().fg(Color::Gray),
        ));
        lines.push(Line::styled(
            fit_width(&format!("      $ {}", finding.next_command), width),
            Style::default().fg(Color::Cyan),
        ));
    }
    if findings.len() > plan.visible {
        lines.push(Line::styled(
            format!("  ... {} more finding(s)", findings.len() - plan.visible),
            Style::default().fg(Color::DarkGray),
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_header(f: &mut Frame, area: Rect, app: &AppState, pod_name: &str) {
    let status = app
        .current_pod()
        .map(|p| {
            let icon = theme::status_icon(&p.status);
            format!(" {icon}  {}", p.phase)
        })
        .unwrap_or_default();

    let text = format!(" [Esc] Back  |  {pod_name}  |{status}");

    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        area,
    );
}

fn render_overview(f: &mut Frame, area: Rect, app: &AppState) {
    let is_focused = app.pod_detail_section == PodDetailSection::Overview;
    let border_style =
        if is_focused { theme::focused_border_style() } else { theme::normal_border_style() };

    let block =
        Block::default().title(" Overview ").borders(Borders::ALL).border_style(border_style);

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
            let (readiness_failures, liveness_failures) =
                probe_failures.get(&container.name).copied().unwrap_or((0, 0));
            lines.push(container_line(container, readiness_failures, liveness_failures));
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
        f.render_widget(Paragraph::new("  Pod not found in current snapshot."), inner);
    }
}

/// Cut a line that would run past the panel edge, marking the cut with "…".
fn fit_width(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn container_probe_failures(app: &AppState, pod: &PodInfo) -> HashMap<String, (u32, u32)> {
    let mut failures = HashMap::new();
    let Some(snapshot) = &app.snapshot else {
        return failures;
    };

    for event in snapshot.events.iter().filter(|e| e.kind == "Pod" && e.name == pod.name) {
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
    let last_reason = container.last_termination_reason.clone().unwrap_or_else(|| "-".to_string());
    let last_exit =
        container.last_exit_code.map(|code| code.to_string()).unwrap_or_else(|| "-".to_string());

    Line::from(vec![
        Span::raw(format!("  {:<16}", truncate_label(&container.name, 16))),
        Span::styled(
            format!("{:<6}  ", if container.ready { "ready" } else { "not" }),
            readiness_style.add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("{:<22}", truncate_label(&container.state, 22))),
        Span::styled(format!("r:{}  ", container.restart_count), Style::default().fg(Color::White)),
        Span::styled(format!("last:{last_reason}/{last_exit}  "), Style::default().fg(Color::Gray)),
        Span::styled(
            format!("probe R:{readiness_failures} L:{liveness_failures}"),
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
    let border_style =
        if is_focused { theme::focused_border_style() } else { theme::normal_border_style() };

    let block = Block::default().title(" Events ").borders(Borders::ALL).border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(snap) = &app.snapshot {
        let pod_events: Vec<_> = snap
            .events
            .iter()
            .filter(|e| e.name == pod_name || e.name.starts_with(pod_name))
            .collect();

        let lines: Vec<Line> = if pod_events.is_empty() {
            vec![Line::styled("  No events for this pod.", Style::default().fg(Color::DarkGray))]
        } else {
            pod_events
                .iter()
                .map(|e| {
                    let ts = chrono::DateTime::parse_from_rfc3339(&e.timestamp)
                        .map(|timestamp| timestamp.format("%H:%M:%S").to_string())
                        .unwrap_or_else(|_| e.timestamp.clone());
                    let style = if e.event_type == EventType::Warning {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    Line::styled(format!("  {}  {:12}  {}", ts, e.reason, e.message), style)
                })
                .collect()
        };

        if is_focused {
            app.detail_scroll_max.set(lines.len().saturating_sub(1));
        }
        let scroll = if is_focused {
            app.detail_scroll.min(lines.len().saturating_sub(1)) as u16
        } else {
            0
        };
        f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
    }
}

fn metric_line(label: &str, pct: u8, detail: String) -> Line<'static> {
    let mut spans = vec![
        Span::raw(format!("  {label}:  ")),
        Span::styled(
            format!("{pct:>3}%  "),
            Style::default().fg(theme::heat_color(pct)).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{detail}  "), Style::default().fg(Color::White)),
    ];
    spans.extend(theme::gradient_bar(pct, 24).spans);
    Line::from(spans)
}

fn history_line(label: &str, history: Option<&Vec<u8>>) -> Line<'static> {
    let mut spans = vec![
        Span::raw(format!("  {label}: ")),
        Span::styled("recent ", Style::default().fg(Color::DarkGray)),
    ];
    spans.extend(theme::sparkline(history, 24).spans);
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::{DiagnosisPlan, render_diagnosis};
    use crate::data::diagnosis::PodFinding;
    use crate::data::models::IncidentSeverity;
    use ratatui::{Terminal, backend::TestBackend};

    /// Render the Diagnosis panel as it would appear on a screen of the given height.
    fn rendered_text_on(findings: &[PodFinding], screen_height: u16) -> String {
        let plan = DiagnosisPlan::new(findings, screen_height);
        let mut terminal = Terminal::new(TestBackend::new(90, plan.height())).unwrap();
        terminal.draw(|f| render_diagnosis(f, f.area(), findings, plan)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn rendered_text(findings: &[PodFinding]) -> String {
        rendered_text_on(findings, 60)
    }

    fn finding(issue: &str) -> PodFinding {
        PodFinding {
            severity: IncidentSeverity::Critical,
            issue: issue.to_string(),
            likely_cause: "The command was not found (exit 127)".to_string(),
            next_command: "kubectl logs api -n prod --previous".to_string(),
        }
    }

    #[test]
    fn diagnosis_panel_shows_issue_cause_and_command() {
        let text = rendered_text(&[finding("Container app is crash-looping (7 restarts)")]);
        assert!(text.contains("[CRITICAL] Container app is crash-looping (7 restarts)"));
        assert!(text.contains("Likely cause: The command was not found (exit 127)"));
        assert!(text.contains("$ kubectl logs api -n prod --previous"));
    }

    #[test]
    fn diagnosis_panel_cuts_long_causes_at_the_panel_edge() {
        let mut long = finding("Container app cannot pull its image");
        long.likely_cause = format!("Failed to pull image \"nginx:typo\": {}", "x".repeat(200));
        let text = rendered_text(&[long]);
        assert!(text.contains("Likely cause: Failed to pull image \"nginx:typo\""));
        assert!(text.contains("x…│"), "cause should end with an ellipsis at the border:\n{text}");
    }

    #[test]
    fn diagnosis_panel_reports_healthy_pod_and_caps_long_lists() {
        assert!(rendered_text(&[]).contains("No problems detected"));

        let findings: Vec<_> = (0..5).map(|i| finding(&format!("issue {i}"))).collect();
        let text = rendered_text(&findings);
        assert!(text.contains("issue 2"));
        assert!(!text.contains("issue 3"));
        assert!(text.contains("... 2 more finding(s)"));
    }

    #[test]
    fn diagnosis_panel_drops_detail_on_short_terminals_to_keep_logs_visible() {
        let findings: Vec<_> = (0..4).map(|i| finding(&format!("issue {i}"))).collect();

        // 24 rows: no room for detail, so one line per finding (at least the worst).
        let plan = DiagnosisPlan::new(&findings, 24);
        assert!(!plan.detailed);
        assert_eq!(plan.height(), 4);
        let text = rendered_text_on(&findings, 24);
        assert!(text.contains("issue 0") && text.contains("... 3 more finding(s)"));
        assert!(!text.contains("Likely cause"));

        // 40 rows: room for two detailed findings, logs keep their minimum.
        let plan = DiagnosisPlan::new(&findings, 40);
        assert!(plan.detailed);
        assert_eq!(plan.visible, 2);
        assert!(plan.height() <= 40 - (1 + 12 + 8 + 1 + 8));

        // Tall screens show the full three.
        assert_eq!(DiagnosisPlan::new(&findings, 60).visible, 3);
    }
}
