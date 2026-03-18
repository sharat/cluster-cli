use chrono::{DateTime, Local};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{AppState, Panel};
use crate::data::models::IncidentSeverity;
use crate::ui::{format, theme};

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let is_focused = app.focused_panel == Panel::Events;

    let border_style = if is_focused {
        theme::focused_border_style()
    } else {
        theme::normal_border_style()
    };

    let incident_count = app
        .snapshot
        .as_ref()
        .map(|s| s.incident_buckets.len())
        .unwrap_or(0);

    let block = Block::default()
        .title(format!(" Incident Queue ({}) ", incident_count))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(snap) = &app.snapshot {
        let items: Vec<ListItem> = snap
            .incident_buckets
            .iter()
            .map(|bucket| {
                let (icon, style) = match bucket.severity {
                    IncidentSeverity::Critical => ("!!", Style::default().fg(Color::Red)),
                    IncidentSeverity::Warning => ("! ", Style::default().fg(Color::Yellow)),
                    IncidentSeverity::Elevated => ("~ ", Style::default().fg(Color::LightBlue)),
                };
                let target_labels: Vec<String> = if bucket.targets.is_empty() {
                    bucket.affected_resources.clone()
                } else {
                    bucket
                        .targets
                        .iter()
                        .map(|target| target.display_label())
                        .collect()
                };
                let target_hint = if target_labels.is_empty() {
                    bucket
                        .sample_message
                        .as_deref()
                        .map(|message| format::truncate_no_ellipsis(message, 20))
                        .unwrap_or_else(|| "no targets".to_string())
                } else {
                    format::truncate_no_ellipsis(&target_labels.join(", "), 20)
                };
                let target_count = if bucket.targets.is_empty() {
                    bucket.affected_resources.len()
                } else {
                    bucket.targets.len()
                };

                let text = format!(
                    "{} {:>5} {:>3} {:>8} {} {}",
                    icon,
                    bucket.occurrences,
                    target_count,
                    format_incident_timestamp(&bucket.latest_timestamp),
                    format::truncate_no_ellipsis(&bucket.reason, 18),
                    target_hint,
                );

                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");

        let mut state = ListState::default();
        if is_focused && incident_count > 0 {
            state.select(Some(app.event_cursor));
        }

        f.render_stateful_widget(list, inner, &mut state);
    }
}

fn format_incident_timestamp(timestamp: &str) -> String {
    if timestamp.is_empty() {
        return "--".to_string();
    }

    DateTime::parse_from_rfc3339(timestamp)
        .map(|ts| ts.with_timezone(&Local).format("%H:%M:%S").to_string())
        .unwrap_or_else(|_| {
            // On parse failure, show truncated timestamp with ellipsis indicator
            let truncated = format::truncate_no_ellipsis(timestamp, 7);
            if timestamp.len() > 7 {
                format!("{}...", truncated)
            } else {
                truncated
            }
        })
}

#[cfg(test)]
mod tests {
    use super::format_incident_timestamp;

    #[test]
    fn formats_rfc3339_incident_timestamps() {
        assert_eq!(format_incident_timestamp("2026-03-09T10:01:00Z").len(), 8);
        assert_eq!(format_incident_timestamp(""), "--");
        assert_eq!(format_incident_timestamp("bad-value"), "bad-val...");
    }
}
