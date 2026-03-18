use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{AppState, Panel};
use crate::ui::{format, theme};

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let is_focused = app.focused_panel == Panel::Nodes;

    let border_style = if is_focused {
        theme::focused_border_style()
    } else {
        theme::normal_border_style()
    };

    let node_count = app.snapshot.as_ref().map(|s| s.nodes.len()).unwrap_or(0);

    let block = Block::default()
        .title(format!(" Nodes ({}) ", node_count))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(snap) = &app.snapshot {
        let items: Vec<ListItem> = snap
            .nodes
            .iter()
            .map(|node| {
                let icon = theme::status_icon(&node.status);
                let style = theme::status_style(&node.status);
                let mut spans = vec![
                    Span::styled(format!("{} ", icon), style),
                    Span::styled(
                        format!("{:<20}", format::truncate_no_ellipsis(&node.name, 20)),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(
                        format!(" CPU:{:>3}%  ", node.cpu_pct),
                        Style::default().fg(Color::Rgb(126, 214, 223)),
                    ),
                    Span::styled(
                        format!("Mem:{:>3}% ", node.memory_pct),
                        Style::default()
                            .fg(theme::heat_color(node.memory_pct))
                            .add_modifier(Modifier::BOLD),
                    ),
                ];
                spans.extend(theme::gradient_bar(node.memory_pct, 8).spans);

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");

        let mut state = ListState::default();
        if node_count > 0 {
            state.select(Some(app.node_cursor.min(node_count.saturating_sub(1))));
        }

        f.render_stateful_widget(list, inner, &mut state);
    }
}


