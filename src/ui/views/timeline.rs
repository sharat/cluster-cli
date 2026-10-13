use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::AppState;
use crate::data::models::IncidentSeverity;
use crate::data::timeline::TimelineEntry;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Fill(1),   // timeline
            Constraint::Length(3), // status bar
        ])
        .split(area);

    render_header(f, chunks[0], app);
    render_timeline(f, chunks[1], app);
    render_status_bar(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &AppState) {
    let title = if let Some(incident) = &app.timeline_from_incident {
        format!(" Incident Timeline: {} ", incident)
    } else {
        " Incident Timeline ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let filter_info = build_filter_info(app);
    let sort_info = if app.timeline_newest_first {
        "Sort: Newest First"
    } else {
        "Sort: Oldest First"
    };

    let info = format!("{}  |  {}", filter_info, sort_info);
    let info_widget = Paragraph::new(info).style(Style::default().fg(Color::Gray));
    f.render_widget(info_widget, inner);
}

fn build_filter_info(app: &AppState) -> String {
    let mut filters = Vec::new();

    if let Some(kind) = app.timeline_kind_filter {
        filters.push(format!("Kind: {}", kind.label()));
    }

    if let Some(ns) = &app.timeline_namespace_filter {
        filters.push(format!("Namespace: {}", ns));
    }

    if let Some(severity) = app.timeline_severity_filter {
        filters.push(format!("Severity: {}", severity.label()));
    }

    if filters.is_empty() {
        "Filters: None".to_string()
    } else {
        format!("Filters: {}", filters.join(", "))
    }
}

fn render_timeline(f: &mut Frame, area: Rect, app: &AppState) {
    let entries = app.get_timeline_entries();

    if entries.is_empty() {
        let empty_msg = if app.snapshot.is_none() {
            "Loading cluster data..."
        } else {
            "No timeline entries found"
        };

        let block = Block::default()
            .title(" Timeline ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let msg = Paragraph::new(empty_msg)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true });
        f.render_widget(msg, inner);
        return;
    }

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| create_timeline_item(entry, idx == app.timeline_cursor))
        .collect();

    let block = Block::default()
        .title(format!(" Timeline ({} entries) ", entries.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let list = List::new(items).block(block);

    f.render_widget(list, area);
}

fn create_timeline_item(entry: &TimelineEntry, selected: bool) -> ListItem<'_> {
    let severity_color = match entry.severity {
        IncidentSeverity::Critical => Color::Red,
        IncidentSeverity::Warning => Color::Yellow,
        IncidentSeverity::Elevated => Color::Rgb(255, 165, 0), // Orange
    };

    let severity_label = format!("[{}]", entry.severity.label().to_uppercase());
    let kind_label = format!("[{}]", entry.kind.label());

    // Format timestamp (show relative time if recent, otherwise absolute)
    let timestamp_display = format_timestamp(&entry.timestamp);

    // Namespace display (if present)
    let namespace_display = entry
        .namespace
        .as_ref()
        .map(|ns| format!("({})", ns))
        .unwrap_or_default();

    // Build the display line
    let mut spans = vec![
        Span::styled(
            format!("{:<10} ", timestamp_display),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!("{:<10} ", severity_label),
            Style::default()
                .fg(severity_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<15} ", kind_label),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            format!("{:<25} ", entry.resource_name),
            Style::default().fg(Color::White),
        ),
    ];

    if !namespace_display.is_empty() {
        spans.push(Span::styled(
            format!("{:<15} ", namespace_display),
            Style::default().fg(Color::Magenta),
        ));
    }

    spans.push(Span::styled(
        &entry.message,
        Style::default().fg(Color::Gray),
    ));

    let mut line = Line::from(spans);

    if selected {
        line = line.style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    }

    ListItem::new(line)
}

fn format_timestamp(timestamp: &str) -> String {
    // Try to parse and format the timestamp
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(dt.with_timezone(&chrono::Utc));

        if duration.num_seconds() < 60 {
            format!("{}s ago", duration.num_seconds())
        } else if duration.num_minutes() < 60 {
            format!("{}m ago", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{}h ago", duration.num_hours())
        } else {
            dt.format("%m-%d %H:%M").to_string()
        }
    } else {
        timestamp.chars().take(16).collect()
    }
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &AppState) {
    let entries_count = app.get_timeline_entries().len();
    let cursor_display = if entries_count > 0 {
        format!(
            "{}/{}",
            app.timeline_cursor.min(entries_count.saturating_sub(1)) + 1,
            entries_count
        )
    } else {
        "0/0".to_string()
    };

    let help_text = vec![
        Span::styled(
            format!(" {} | ", cursor_display),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" Back | "),
        Span::styled("↑/k", Style::default().fg(Color::Yellow)),
        Span::raw("/"),
        Span::styled("↓/j", Style::default().fg(Color::Yellow)),
        Span::raw(" Scroll | "),
        Span::styled("s", Style::default().fg(Color::Yellow)),
        Span::raw(" Sort | "),
        Span::styled("f", Style::default().fg(Color::Yellow)),
        Span::raw(" Filter Kind | "),
        Span::styled("F", Style::default().fg(Color::Yellow)),
        Span::raw(" Filter Severity | "),
        Span::styled("c", Style::default().fg(Color::Yellow)),
        Span::raw(" Clear Filters "),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let paragraph = Paragraph::new(Line::from(help_text)).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}
