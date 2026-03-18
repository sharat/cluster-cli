use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};

use crate::app::{AppState, Panel};
use crate::ui::{format, theme};

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let is_focused = app.focused_panel == Panel::Pods;

    let border_style = if is_focused {
        theme::focused_border_style()
    } else {
        theme::normal_border_style()
    };

    let filter_hint = if app.filter_active {
        format!(" [/{}█] ", app.pod_filter)
    } else if !app.pod_filter.is_empty() {
        format!(" [filter: {}] ", app.pod_filter)
    } else {
        " [/] filter ".to_string()
    };

    let incident_hint = app
        .incident_focus
        .as_ref()
        .map(|focus| format!(" [incident: {}] ", focus.reason))
        .unwrap_or_default();

    let pods = app.filtered_pods();
    let pod_count = pods.len();

    let block = Block::default()
        .title(format!(
            " Pods ({})  [sort: {}]  {}{} ",
            pod_count,
            app.pod_sort_mode.label(),
            filter_hint.trim(),
            incident_hint
        ))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let header = Row::new(vec![
        Cell::from("  Status").style(theme::header_style()),
        Cell::from("Name").style(theme::header_style()),
        Cell::from("CPU").style(theme::header_style()),
        Cell::from("CUse").style(theme::header_style()),
        Cell::from("CReq").style(theme::header_style()),
        Cell::from("CLim").style(theme::header_style()),
        Cell::from("Mem").style(theme::header_style()),
        Cell::from("Use").style(theme::header_style()),
        Cell::from("Req").style(theme::header_style()),
        Cell::from("Lim").style(theme::header_style()),
        Cell::from("Rst").style(theme::header_style()),
        Cell::from("Age").style(theme::header_style()),
    ])
    .bottom_margin(1);

    let rows: Vec<Row> = pods
        .iter()
        .map(|pod| {
            let icon = theme::status_icon(&pod.status);
            let style = theme::status_style(&pod.status);
            let phase_short: String = pod.phase.chars().take(7).collect();

            let cpu_line = resource_triplet_line(
                pod.cpu_pct,
                String::new(),
            );
            let mem_line = resource_triplet_line(
                pod.memory_pct,
                String::new(),
            );

            Row::new(vec![
                Cell::from(format!("{} {}", icon, phase_short)),
                Cell::from(pod.name.clone()),
                Cell::from(cpu_line),
                Cell::from(format::cpu(pod.cpu_millicores)),
                Cell::from(format::cpu(pod.cpu_request_millicores)),
                Cell::from(format::cpu(pod.cpu_limit_millicores)),
                Cell::from(mem_line),
                Cell::from(format::memory(pod.memory_mb)),
                Cell::from(format::memory(pod.memory_request_mb)),
                Cell::from(format::memory(pod.memory_limit_mb)),
                Cell::from(pod.restarts.to_string()),
                Cell::from(pod.age.clone()),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Fill(1),
        Constraint::Length(12),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(4),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(theme::selected_style());

    let mut state = TableState::default();
    if is_focused && pod_count > 0 {
        state.select(Some(app.pod_cursor));
    }

    f.render_stateful_widget(table, inner, &mut state);
}

fn resource_triplet_line(pct: u8, summary: String) -> Line<'static> {
    let mut spans = theme::gradient_bar(pct, 6).spans;
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("{:>3}%", pct),
        Style::default()
            .fg(theme::heat_color(pct))
            .add_modifier(Modifier::BOLD),
    ));
    if !summary.is_empty() {
        spans.push(Span::styled(
            format!(" {}", summary),
            Style::default().fg(Color::White),
        ));
    }
    Line::from(spans)
}
