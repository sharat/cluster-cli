use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
    Frame,
};

use crate::app::AppState;
use crate::data::models::{ConditionStatus, NodeMetric};
use crate::ui::theme;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let popup_area = centered_rect(area, 82, 75);
    f.render_widget(Clear, popup_area);

    let node = match app.current_node() {
        Some(n) => n,
        None => {
            let msg = Paragraph::new("No node selected").style(Style::default().fg(Color::Red));
            f.render_widget(msg, popup_area);
            return;
        }
    };

    let block = Block::default()
        .title(" Node Details ")
        .title_bottom(" Esc/q to close ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(0),
        ])
        .split(inner);

    render_header(f, chunks[0], node);
    render_resources(f, chunks[1], node);
    render_info(f, chunks[2], node);
}

fn render_header(f: &mut Frame, area: Rect, node: &crate::data::models::NodeMetric) {
    let icon = theme::status_icon(&node.status);
    let style = theme::status_style(&node.status);

    let title = format!(" {} {} Node Details ", icon, node.name);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style);

    f.render_widget(block, area);
}

fn centered_rect(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1])[1]
}

fn render_resources(f: &mut Frame, area: Rect, node: &crate::data::models::NodeMetric) {
    let rows = vec![
        Row::new(vec![
            Cell::from("CPU Usage").style(Style::default().fg(Color::Cyan)),
            Cell::from(format!(
                "{}m / {}m",
                node.cpu_millicores,
                node.cpu_capacity * 1000
            )),
            Cell::from(usage_meter(node.cpu_pct)),
        ]),
        Row::new(vec![
            Cell::from("Memory Usage").style(Style::default().fg(Color::Cyan)),
            Cell::from(format!("{}Mi / {}Mi", node.memory_mb, node.memory_total_mb)),
            Cell::from(usage_meter(node.memory_pct)),
        ]),
        Row::new(vec![
            Cell::from("Allocatable CPU").style(Style::default().fg(Color::DarkGray)),
            Cell::from(format!("{} cores", node.cpu_capacity)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from("Allocatable Memory").style(Style::default().fg(Color::DarkGray)),
            Cell::from(format!("{}Mi", node.memory_total_mb)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from("Capacity Memory").style(Style::default().fg(Color::DarkGray)),
            Cell::from(format!("{}Mi", node.memory_capacity_mb)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from("Status").style(Style::default().fg(Color::Cyan)),
            Cell::from(format!("{:?}", node.status)),
            Cell::from(""),
        ]),
    ];

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Fill(1),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec!["Resource", "Value", "Usage"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().title(" Resources ").borders(Borders::ALL));

    f.render_widget(table, area);
}

fn usage_meter(pct: u8) -> Line<'static> {
    let mut spans = theme::gradient_bar(pct, 12).spans;
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("{:>3}%", pct),
        Style::default()
            .fg(theme::heat_color(pct))
            .add_modifier(Modifier::BOLD),
    ));
    Line::from(spans)
}

fn render_info(f: &mut Frame, area: Rect, node: &crate::data::models::NodeMetric) {
    let info_block = Block::default()
        .title(" Node Information ")
        .borders(Borders::ALL);

    let inner = info_block.inner(area);
    f.render_widget(info_block, area);

    if let Some(node_info) = &node.node_info {
        let rows = vec![
            condition_row("Ready", node.conditions.ready, true),
            condition_row("MemoryPressure", node.conditions.memory_pressure, false),
            condition_row("DiskPressure", node.conditions.disk_pressure, false),
            condition_row("PIDPressure", node.conditions.pid_pressure, false),
            condition_row(
                "NetworkUnavailable",
                node.conditions.network_unavailable,
                false,
            ),
            Row::new(vec![
                Cell::from("Scheduling"),
                Cell::from(scheduling_line(node)),
            ])
            .style(Style::default().fg(Color::Cyan)),
            Row::new(vec![
                Cell::from("Kernel Version"),
                Cell::from(node_info.kernel_version.clone()),
            ])
            .style(Style::default().fg(Color::Cyan)),
            Row::new(vec![
                Cell::from("OS Image"),
                Cell::from(node_info.os_image.clone()),
            ])
            .style(Style::default().fg(Color::Cyan)),
            Row::new(vec![
                Cell::from("Container Runtime"),
                Cell::from(node_info.container_runtime.clone()),
            ])
            .style(Style::default().fg(Color::Cyan)),
            Row::new(vec![
                Cell::from("Kubelet Version"),
                Cell::from(node_info.kubelet_version.clone()),
            ])
            .style(Style::default().fg(Color::Cyan)),
            Row::new(vec![
                Cell::from("Architecture"),
                Cell::from(node_info.architecture.clone()),
            ])
            .style(Style::default().fg(Color::Cyan)),
            Row::new(vec![
                Cell::from("Operating System"),
                Cell::from(node_info.operating_system.clone()),
            ])
            .style(Style::default().fg(Color::Cyan)),
        ];

        let table = Table::new(rows, [Constraint::Length(20), Constraint::Fill(1)]).header(
            Row::new(vec!["Property", "Value"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        );

        f.render_widget(table, inner);
    } else {
        let rows = vec![
            condition_row("Ready", node.conditions.ready, true),
            condition_row("MemoryPressure", node.conditions.memory_pressure, false),
            condition_row("DiskPressure", node.conditions.disk_pressure, false),
            condition_row("PIDPressure", node.conditions.pid_pressure, false),
            condition_row(
                "NetworkUnavailable",
                node.conditions.network_unavailable,
                false,
            ),
            Row::new(vec![
                Cell::from("Scheduling"),
                Cell::from(scheduling_line(node)),
            ])
            .style(Style::default().fg(Color::Cyan)),
            Row::new(vec![Cell::from("Node Info"), Cell::from("Not available")])
                .style(Style::default().fg(Color::Yellow)),
        ];

        let table = Table::new(rows, [Constraint::Length(20), Constraint::Fill(1)]).header(
            Row::new(vec!["Property", "Value"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        );

        f.render_widget(table, inner);
    }
}

fn condition_row(
    label: &'static str,
    status: ConditionStatus,
    healthy_when_true: bool,
) -> Row<'static> {
    Row::new(vec![
        Cell::from(label),
        Cell::from(condition_line(status, healthy_when_true)),
    ])
    .style(Style::default().fg(Color::Cyan))
}

fn condition_line(status: ConditionStatus, healthy_when_true: bool) -> Line<'static> {
    let (icon, color) = match status {
        ConditionStatus::True if healthy_when_true => ("Healthy", Color::Green),
        ConditionStatus::False if !healthy_when_true => ("Healthy", Color::Green),
        ConditionStatus::Unknown => ("Unknown", Color::Yellow),
        ConditionStatus::True => ("Active", Color::Red),
        ConditionStatus::False => ("Inactive", Color::DarkGray),
    };

    Line::from(vec![
        Span::styled(
            icon,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("({})", status.as_str()),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn scheduling_line(node: &NodeMetric) -> Line<'static> {
    let (label, color) = if node.cordoned || node.draining {
        ("Cordoned", Color::Yellow)
    } else {
        ("Schedulable", Color::Green)
    };

    Line::from(vec![
        Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            if node.cordoned || node.draining {
                "(cordoned/draining)"
            } else {
                "(enabled)"
            },
            Style::default().fg(Color::DarkGray),
        ),
    ])
}
