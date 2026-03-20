use chrono::{DateTime, Local};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::AppState;
use crate::data::models::IncidentSeverity;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let Some(bucket) = app.selected_incident() else {
        return;
    };

    let popup_w = area.width.saturating_sub(12).clamp(60, 90);
    let popup_h = area.height.saturating_sub(6).clamp(14, 28);
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w.min(area.width), popup_h.min(area.height));

    f.render_widget(Clear, popup_area);

    let (border_color, severity_label) = match bucket.severity {
        IncidentSeverity::Critical => (Color::Red, "CRITICAL"),
        IncidentSeverity::Warning => (Color::Yellow, "WARNING"),
        IncidentSeverity::Elevated => (Color::LightBlue, "ELEVATED"),
    };

    let block = Block::default()
        .title(" Event Detail  [Enter] drill-down  [Esc] close ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let mut lines: Vec<Line> = vec![];

    // Reason + severity badge
    lines.push(Line::from(vec![
        Span::styled(
            bucket.reason.clone(),
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            severity_label,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(""));

    // Occurrences + last seen
    let timestamp_formatted = if bucket.latest_timestamp.is_empty() {
        "--".to_string()
    } else {
        DateTime::parse_from_rfc3339(&bucket.latest_timestamp)
            .map(|ts| {
                ts.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|_| bucket.latest_timestamp.clone())
    };

    lines.push(Line::from(vec![
        Span::styled("Occurrences  ", Style::default().fg(Color::White)),
        Span::raw(bucket.occurrences.to_string()),
        Span::styled("     Last seen  ", Style::default().fg(Color::White)),
        Span::raw(timestamp_formatted),
    ]));

    lines.push(Line::from(""));

    // Full message
    lines.push(Line::from(vec![Span::styled(
        "Message",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]));
    match bucket.sample_message.as_deref().filter(|m| !m.is_empty()) {
        Some(msg) => lines.push(Line::from(vec![Span::styled(
            format!("  {msg}"),
            Style::default().fg(Color::Gray),
        )])),
        None => lines.push(Line::from(vec![Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )])),
    }

    lines.push(Line::from(""));

    // Affected targets
    let target_labels: Vec<String> = if bucket.targets.is_empty() {
        bucket.affected_resources.clone()
    } else {
        bucket
            .targets
            .iter()
            .map(|t| t.display_label())
            .collect()
    };

    lines.push(Line::from(vec![Span::styled(
        format!("Affected  ({})", target_labels.len()),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]));

    if target_labels.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for label in &target_labels {
            lines.push(Line::from(vec![Span::styled(
                format!("  {label}"),
                Style::default().fg(Color::Cyan),
            )]));
        }
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}
