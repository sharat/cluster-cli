use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::AppState;
use crate::ui::{format, theme};

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    if let Some(snap) = &app.snapshot {
        let h = &snap.health;
        let mut summary_spans = theme::health_bar(h.score, 28).spans;
        summary_spans.push(Span::raw(" "));
        summary_spans.push(Span::styled(
            format!("{:>3}/100", h.score),
            Style::default()
                .fg(theme::score_bar_color(h.score))
                .add_modifier(Modifier::BOLD),
        ));
        summary_spans.push(Span::styled(
            format!(" ({})", h.grade),
            theme::grade_style(h.grade),
        ));
        summary_spans.push(Span::styled(
            format!(
                "   {} critical nodes   {} critical pods   {} total restarts",
                h.critical_nodes, h.critical_pods, h.total_restarts
            ),
            Style::default().fg(Color::White),
        ));

        let resources = app.namespace_resource_summary();
        let resource_line = Line::from(vec![
            Span::styled(" ns resources  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "CPU {} / {} / {}  ",
                    format::cpu(resources.cpu_usage_millicores),
                    format::cpu(resources.cpu_request_millicores),
                    format::cpu(resources.cpu_limit_millicores),
                ),
                Style::default().fg(Color::Rgb(126, 214, 223)),
            ),
            Span::styled(
                format!(
                    "({:>3}% req {:>3}% lim)  ",
                    pct(
                        resources.cpu_usage_millicores,
                        resources.cpu_request_millicores
                    ),
                    pct(
                        resources.cpu_usage_millicores,
                        resources.cpu_limit_millicores
                    ),
                ),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!(
                    "Mem {} / {} / {}  ",
                    format::memory(resources.memory_usage_mb),
                    format::memory(resources.memory_request_mb),
                    format::memory(resources.memory_limit_mb),
                ),
                Style::default().fg(theme::heat_color(pct(
                    resources.memory_usage_mb,
                    resources.memory_limit_mb,
                ))),
            ),
            Span::styled(
                format!(
                    "({:>3}% req {:>3}% lim)  {} pods",
                    pct(resources.memory_usage_mb, resources.memory_request_mb),
                    pct(resources.memory_usage_mb, resources.memory_limit_mb),
                    resources.pod_count,
                ),
                Style::default().fg(Color::White),
            ),
        ]);

        f.render_widget(
            Paragraph::new(vec![Line::from(summary_spans), resource_line]),
            area,
        );
    } else if app.is_loading {
        let p =
            Paragraph::new(" Connecting to cluster...").style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
    }
}

fn pct(used: u64, total: u64) -> u8 {
    if total == 0 {
        0
    } else {
        ((used.saturating_mul(100)) / total).min(100) as u8
    }
}
