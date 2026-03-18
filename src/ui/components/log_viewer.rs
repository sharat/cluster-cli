use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::AppState;
use crate::ui::theme;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let follow_indicator = if app.log_follow { " ▮ following" } else { "" };
    let title = format!(" Logs (live) [f]ollow{} ", follow_indicator);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme::normal_border_style());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;

    let lines: Vec<Line> = app
        .log_buffer
        .iter()
        .map(|line| Line::styled(line.clone(), theme::log_level_style(line)))
        .collect();

    let scroll_offset = if app.log_follow {
        lines.len().saturating_sub(height) as u16
    } else {
        app.detail_scroll.min(lines.len().saturating_sub(1)) as u16
    };

    let para = Paragraph::new(lines).scroll((scroll_offset, 0));
    f.render_widget(para, inner);
}
