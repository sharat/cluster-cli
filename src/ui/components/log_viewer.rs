use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{AppState, LogSource, PodDetailSection};
use crate::ui::theme;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let follow_indicator = if app.log_follow { " ▮ following" } else { "" };
    let container = app.log_container.as_deref().unwrap_or("default");
    let wrap = if app.log_wrap { "on" } else { "off" };
    let timestamps = if app.log_timestamps { "on" } else { "off" };
    let filtered_lines = app.filtered_log_lines();
    let search = if app.log_search.is_empty() {
        String::new()
    } else {
        format!(
            " search:\"{}\" {}/{}",
            app.log_search,
            filtered_lines.len(),
            app.log_buffer.len()
        )
    };
    let title = format!(
        " Logs ({}) c:{container} [c]ontainer [p]revious [/]search [w]rap:{wrap} [t]ime:{timestamps} [E]xport [f]ollow{follow_indicator}{search} ",
        app.log_source.label()
    );
    let border_style = if app.pod_detail_section == PodDetailSection::Logs {
        theme::focused_border_style()
    } else {
        theme::normal_border_style()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;

    let lines: Vec<Line> = if filtered_lines.is_empty() {
        let message = if app.log_buffer.is_empty() {
            if app.log_source == LogSource::Previous {
                "  No previous logs available."
            } else {
                "  Waiting for log output..."
            }
        } else {
            "  No log lines match the current search."
        };
        vec![Line::styled(message, Style::default().fg(Color::DarkGray))]
    } else {
        filtered_lines
            .iter()
            .map(|raw_line| {
                let line = if app.log_timestamps {
                    *raw_line
                } else {
                    strip_kubectl_timestamp(raw_line)
                };
                let mut style = theme::log_level_style(line);
                if !app.log_search.is_empty() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                Line::styled(line.to_string(), style)
            })
            .collect()
    };

    let visual_line_count = if app.log_wrap {
        wrapped_line_count(&lines, inner.width as usize)
    } else {
        lines.len()
    };
    let scroll_offset = if app.log_follow {
        visual_line_count.saturating_sub(height) as u16
    } else {
        app.detail_scroll.min(visual_line_count.saturating_sub(1)) as u16
    };

    let mut para = Paragraph::new(lines).scroll((scroll_offset, 0));
    if app.log_wrap {
        para = para.wrap(Wrap { trim: false });
    }
    f.render_widget(para, inner);
}

fn strip_kubectl_timestamp(line: &str) -> &str {
    let Some((timestamp, content)) = line.split_once(' ') else {
        return line;
    };
    if chrono::DateTime::parse_from_rfc3339(timestamp).is_ok() {
        content
    } else {
        line
    }
}

fn wrapped_line_count(lines: &[Line<'_>], available_width: usize) -> usize {
    if available_width == 0 {
        return lines.len();
    }
    lines
        .iter()
        .map(|line| {
            let content_width = line.width().max(1);
            (content_width + available_width.saturating_sub(1)) / available_width
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::strip_kubectl_timestamp;

    #[test]
    fn timestamp_toggle_removes_kubectl_rfc3339_prefix() {
        assert_eq!(
            strip_kubectl_timestamp("2026-08-19T20:40:12.123456789Z server ready"),
            "server ready"
        );
    }

    #[test]
    fn timestamp_toggle_preserves_plain_log_line() {
        assert_eq!(strip_kubectl_timestamp("server ready"), "server ready");
    }
}
