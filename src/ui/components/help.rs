//! Help screen (`?`): every key binding and what each on-screen marker means.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::AppState;
use crate::ui::theme;

const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "Dashboard",
        &[
            ("1 2 3, Tab", "Focus Nodes, Incident Queue, Pods; Tab cycles"),
            ("j k, ↓ ↑", "Move the selection"),
            ("Enter", "Open the pod or node, or drill into the incident"),
            ("Esc", "Clear the incident focus"),
            ("t", "Incident timeline"),
            ("T", "Timeline for the selected incident (Incident Queue)"),
            ("u", "Right-sizing"),
            ("w", "Workloads"),
            ("/", "Filter pods by name"),
            ("s", "Cycle pod sort order"),
            ("E", "Export pods to CSV"),
            ("n, N", "Pick a namespace, or type one"),
            ("r, R", "Refresh now, or set the refresh interval"),
            ("q, Ctrl+C", "Quit"),
        ],
    ),
    (
        "Pod detail",
        &[
            ("Tab", "Cycle Overview, Events, Logs"),
            ("j k", "Scroll the focused section"),
            ("f c p", "Logs: follow, next container, previous logs"),
            ("/ w t E", "Logs: search, wrap, timestamps, export"),
            ("Esc, q", "Back"),
        ],
    ),
    ("Node detail", &[("j k", "Scroll"), ("Esc, q", "Back")]),
    (
        "Timeline",
        &[
            ("j k", "Move the selection"),
            ("s", "Newest or oldest first"),
            ("f, F", "Cycle the kind or severity filter"),
            ("c", "Clear filters"),
            ("Esc, q", "Back"),
        ],
    ),
    ("Right-sizing", &[("j k", "Scroll"), ("r", "Refresh"), ("Esc, q", "Back")]),
    (
        "Markers",
        &[
            ("!! ! ~ ✓", "Critical, warning, elevated, healthy"),
            ("▶", "Focused panel"),
            (">", "Selected row"),
            ("⚡", "Timeline: likely trigger of a later incident"),
            ("*", "Right-sizing: suggestion capped at the limit"),
        ],
    ),
];

/// Rows of help text, shared by the renderer and the scroll limit.
pub fn lines() -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, (section, keys)) in SECTIONS.iter().enumerate() {
        if index > 0 {
            lines.push(Line::raw(""));
        }
        lines
            .push(Line::styled(*section, theme::header_style().add_modifier(Modifier::UNDERLINED)));
        for (key, action) in keys.iter() {
            lines.push(Line::raw(format!("  {key:<12} {action}")));
        }
    }
    lines
}

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let width = area.width.saturating_sub(4).min(76);
    let height = area.height.saturating_sub(2);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Help  [j/k] scroll  [?/Esc/q] close ")
        .borders(Borders::ALL)
        .border_style(theme::focused_border_style());
    let inner = block.inner(popup);
    let lines = lines();
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    app.help_scroll_max.set(max_scroll);
    let scroll = app.help_scroll.min(max_scroll) as u16;
    f.render_widget(
        Paragraph::new(lines).style(Style::default()).block(block).scroll((scroll, 0)),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::render;
    use crate::app::AppState;
    use crate::config::Config;

    fn rendered(width: u16, height: u16, app: &AppState) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(f, f.area(), app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn lists_every_view_and_the_markers() {
        let app = AppState::new(Config::default());
        let text = rendered(80, 70, &app);
        for section in ["Dashboard", "Pod detail", "Node detail", "Timeline", "Right-sizing"] {
            assert!(text.contains(section), "{section} missing:\n{text}");
        }
        assert!(text.contains("Critical, warning, elevated, healthy"), "{text}");
        assert_eq!(app.help_scroll_max.get(), 0, "everything fits at 70 rows");
    }

    #[test]
    fn short_terminals_scroll_to_the_end() {
        let mut app = AppState::new(Config::default());
        let _ = rendered(80, 24, &app);
        let max = app.help_scroll_max.get();
        assert!(max > 0);

        app.help_scroll = max;
        let text = rendered(80, 24, &app);
        assert!(text.contains("suggestion capped at the limit"), "{text}");
    }
}
