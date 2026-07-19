use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::Paragraph,
    Frame,
};

use crate::app::{AppState, AppView};
use crate::ui::components::loading_spinner;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let dashboard_help =
        "[1] Nodes  [2] Incidents  [3] Pods  [Tab] Panel  [j/k] Nav  [Enter] Drill  [w] Workload Popup  [/] Filter  [s] Sort  [E] Export  [n/N] Namespace  [r] Refresh  [R] Rate  [q] Quit";
    let detail_help = "[Tab] Section  [j/k] Scroll  [f] Follow  [Esc/q] Back";
    let node_help = "[j/k] Scroll  [Esc/q] Back";

    // Check if status message is still fresh (< 5s)
    let fresh_error = app
        .status_message
        .as_ref()
        .filter(|(_, t)| t.elapsed().as_secs() < 5);

    let (line, style) = if let Some((msg, _)) = fresh_error {
        (
            Line::from(format!(" ⚠  {msg} ")),
            Style::default().fg(Color::Yellow),
        )
    } else if app.is_loading {
        let mut spans = loading_spinner::spans(app.loading_animation_frame);
        spans.push(" Loading...".into());
        (Line::from(spans), Style::default().fg(Color::DarkGray))
    } else {
        let help = match &app.view {
            AppView::Dashboard => dashboard_help,
            AppView::PodDetail { .. } => detail_help,
            AppView::NodeDetail { .. } => node_help,
        };
        (
            Line::from(format!(" {help} ")),
            Style::default().fg(Color::DarkGray),
        )
    };

    f.render_widget(Paragraph::new(line).style(style), area);
}
