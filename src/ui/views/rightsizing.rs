use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::app::AppState;
use crate::data::rightsizing::{
    HEADROOM_PCT, OVERPROVISIONED_PCT, ResourceTotals, SizingVerdict, WorkloadSizing, summarize,
};
use crate::ui::{components, format, theme};

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let workloads = app.rightsizing();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(5), // summary
            Constraint::Fill(1),   // table
            Constraint::Length(1), // status bar
        ])
        .split(area);

    let header = format!(
        " [Esc] Back  |  Right-sizing  |  ns:{}  |  usage is the peak seen this session",
        app.config.namespace
    );
    f.render_widget(
        Paragraph::new(header)
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        chunks[0],
    );
    render_summary(f, chunks[1], &workloads);
    render_table(f, chunks[2], app, &workloads);
    components::status_bar::render(f, chunks[3], app);
}

fn render_summary(f: &mut Frame, area: Rect, workloads: &[WorkloadSizing]) {
    let block = Block::default()
        .title(" Summary ")
        .borders(Borders::ALL)
        .border_style(theme::normal_border_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = if workloads.is_empty() {
        vec![Line::styled(
            "  No running pods in this namespace.",
            Style::default().fg(Color::DarkGray),
        )]
    } else if workloads.iter().all(|w| w.verdict == SizingVerdict::NoMetrics) {
        vec![
            Line::styled(
                "  No usage metrics: `kubectl top pods` returned nothing.",
                Style::default().fg(Color::Yellow),
            ),
            Line::styled(
                "  Right-sizing needs metrics-server in the cluster (kind and some local clusters lack it).",
                Style::default().fg(Color::DarkGray),
            ),
        ]
    } else {
        let summary = summarize(workloads);
        vec![
            Line::from(vec![
                Span::raw("  Requested  "),
                Span::styled(
                    format!(
                        "CPU {}  Mem {}",
                        format::cpu(summary.cpu.request),
                        format::memory(summary.memory.request)
                    ),
                    Style::default().fg(Color::White),
                ),
                Span::raw("    Using  "),
                Span::styled(
                    format!(
                        "CPU {}{}  Mem {}{}",
                        format::cpu(summary.cpu.usage),
                        pct_suffix(summary.cpu),
                        format::memory(summary.memory.usage),
                        pct_suffix(summary.memory)
                    ),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::raw("  Reclaimable by following the suggestions  "),
                Span::styled(
                    format!(
                        "CPU {}  Mem {}",
                        format::cpu(summary.reclaimable_cpu),
                        format::memory(summary.reclaimable_memory)
                    ),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("    {} workload(s) flagged", summary.workloads_flagged),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::styled(
                format!(
                    "  over = using under {OVERPROVISIONED_PCT}% of the request; suggestions are usage + {HEADROOM_PCT}% headroom, per pod; \"at limit\" = raise the limit too"
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]
    };
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_table(f: &mut Frame, area: Rect, app: &AppState, workloads: &[WorkloadSizing]) {
    let block = Block::default()
        .title(format!(" Workloads ({}) ", workloads.len()))
        .borders(Borders::ALL)
        .border_style(theme::focused_border_style());

    let header = Row::new([
        "Verdict",
        "Workload",
        "Pods",
        "CPU use / req",
        "Mem use / req",
        "Suggested / pod",
    ])
    .style(theme::header_style());

    let visible_rows = area.height.saturating_sub(3) as usize;
    let max_scroll = workloads.len().saturating_sub(visible_rows.max(1));
    app.sizing_scroll_max.set(max_scroll);
    let scroll = app.sizing_scroll.min(max_scroll);
    let rows = workloads.iter().skip(scroll).map(|workload| {
        let (color, label) = verdict_style(workload.verdict);
        Row::new(vec![
            Cell::from(Span::styled(
                format!("[{label}]"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )),
            Cell::from(format!("{}/{}", short_kind(&workload.owner.kind), workload.owner.name)),
            Cell::from(if workload.measured_pods == workload.pods {
                workload.pods.to_string()
            } else {
                format!("{}/{}", workload.measured_pods, workload.pods)
            }),
            Cell::from(usage_cell(workload.cpu, format::cpu)),
            Cell::from(usage_cell(workload.memory, format::memory)),
            Cell::from(suggestion(workload)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Fill(2),
            // Fits "measured/total" up to three digits each.
            Constraint::Length(8),
            Constraint::Length(20),
            Constraint::Length(22),
            Constraint::Fill(2),
        ],
    )
    .header(header)
    .block(block);
    f.render_widget(table, area);
}

fn verdict_style(verdict: SizingVerdict) -> (Color, &'static str) {
    let color = match verdict {
        SizingVerdict::UnderProvisioned => Color::Red,
        SizingVerdict::OverProvisioned => Color::Yellow,
        SizingVerdict::NoRequests => Color::Magenta,
        SizingVerdict::NoMetrics => Color::DarkGray,
        SizingVerdict::Ok => Color::Green,
    };
    (color, verdict.label())
}

fn short_kind(kind: &str) -> &str {
    match kind {
        "Deployment" => "deploy",
        "StatefulSet" => "sts",
        "DaemonSet" => "ds",
        "ReplicaSet" => "rs",
        "Job" => "job",
        "Pod" => "pod",
        other => other,
    }
}

fn pct_suffix(totals: ResourceTotals) -> String {
    totals.usage_pct().map(|pct| format!(" ({pct}%)")).unwrap_or_default()
}

fn usage_cell(totals: ResourceTotals, fmt: fn(u64) -> String) -> String {
    let usage = if totals.usage == 0 { "0".to_string() } else { fmt(totals.usage) };
    format!("{usage} / {}{}", fmt(totals.request), pct_suffix(totals))
}

fn suggestion(workload: &WorkloadSizing) -> String {
    let parts: Vec<String> = [
        workload.suggested_cpu.map(|cpu| {
            let note = if workload.cpu_at_limit { " (at limit)" } else { "" };
            format!("cpu {}{note}", format::cpu(cpu))
        }),
        workload.suggested_memory.map(|mem| {
            let note = if workload.memory_at_limit { " (at limit)" } else { "" };
            format!("mem {}{note}", format::memory(mem))
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() { "-".to_string() } else { parts.join(", ") }
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::app::{AppState, AppView};
    use crate::config::Config;
    use crate::data::models::{ClusterSnapshot, HealthScore, HealthStatus, PodInfo, PodOwner};
    use ratatui::{Terminal, backend::TestBackend};
    use std::time::Instant;

    fn pod(name: &str, request: (u64, u64), usage: (u64, u64)) -> PodInfo {
        PodInfo {
            uid: format!("uid-{name}"),
            name: format!("{name}-abc"),
            namespace: "default".to_string(),
            phase: "Running".to_string(),
            restarts: 0,
            age: "1h".to_string(),
            cpu_millicores: usage.0,
            cpu_request_millicores: request.0,
            cpu_limit_millicores: 0,
            memory_mb: usage.1,
            memory_request_mb: request.1,
            memory_limit_mb: 0,
            memory_request_pct: 0,
            memory_pct: 0,
            cpu_request_pct: 0,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
            ready_containers: 1,
            total_containers: 1,
            is_ready: true,
            crash_looping: false,
            oom_killed: false,
            init_containers: vec![],
            sidecar_containers: vec![],
            owner: Some(PodOwner { kind: "Deployment".to_string(), name: name.to_string() }),
            metrics_available: true,
            running_limits: Default::default(),
            node_name: None,
            containers: vec![],
        }
    }

    fn snapshot(pods: Vec<PodInfo>) -> ClusterSnapshot {
        ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods,
            events: vec![],
            incident_buckets: vec![],
            health: HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: Instant::now(),
            error: None,
            context_name: None,
        }
    }

    fn rendered(pods: Vec<PodInfo>) -> String {
        let mut app = AppState::new(Config::default());
        app.snapshot = Some(snapshot(pods));
        app.view = AppView::RightSizing;
        let mut terminal = Terminal::new(TestBackend::new(140, 20)).unwrap();
        terminal.draw(|f| render(f, f.area(), &app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn shows_verdicts_suggestions_and_reclaimable_total() {
        let text = rendered(vec![
            pod("idle-api", (500, 512), (20, 60)),
            pod("hot-worker", (50, 64), (90, 40)),
        ]);

        assert!(text.contains("[under]"), "{text}");
        assert!(text.contains("deploy/hot-worker"));
        assert!(text.contains("[over]"));
        assert!(text.contains("deploy/idle-api"));
        assert!(text.contains("20m / 500m (4%)"));
        assert!(text.contains("cpu 30m, mem 80Mi"));
        assert!(text.contains("Reclaimable by following the suggestions  CPU 470m  Mem 432Mi"));
    }

    #[test]
    fn explains_missing_metrics() {
        let mut unmeasured = pod("api", (100, 128), (0, 0));
        unmeasured.metrics_available = false;
        let text = rendered(vec![unmeasured]);

        assert!(text.contains("No usage metrics"), "{text}");
        assert!(text.contains("[no metrics]"));
    }

    #[test]
    fn scrolling_stops_at_the_last_page() {
        use crate::events::handler::handle_key;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let pods: Vec<PodInfo> =
            (0..30).map(|i| pod(&format!("app-{i:02}"), (100, 128), (60, 90))).collect();
        let mut app = AppState::new(Config::default());
        app.snapshot = Some(snapshot(pods));
        app.view = AppView::RightSizing;
        let mut terminal = Terminal::new(TestBackend::new(140, 20)).unwrap();
        let mut draw = |app: &AppState| {
            terminal.draw(|f| render(f, f.area(), app)).unwrap();
        };
        let key = |app: &mut AppState, code| {
            let _ = handle_key(app, KeyEvent::new(code, KeyModifiers::NONE));
        };

        draw(&app);
        for _ in 0..50 {
            key(&mut app, KeyCode::Char('j'));
        }
        let max = app.sizing_scroll_max.get();
        assert!(max > 0);
        assert_eq!(app.sizing_scroll, max, "j stops at the last full page");

        key(&mut app, KeyCode::Char('k'));
        assert_eq!(app.sizing_scroll, max - 1, "k moves the list right away");
    }

    #[test]
    fn partial_metrics_show_the_full_measured_ratio() {
        let mut pods: Vec<PodInfo> = (0..120).map(|_| pod("web", (100, 128), (60, 90))).collect();
        for pod in pods.iter_mut().skip(12) {
            pod.metrics_available = false;
        }

        let text = rendered(pods);

        assert!(text.contains(" 12/120 "), "{text}");
    }
}
