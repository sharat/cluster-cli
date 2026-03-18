#![allow(dead_code)]

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::data::models::HealthStatus;

pub fn status_style(status: &HealthStatus) -> Style {
    match status {
        HealthStatus::Critical => Style::default()
            .fg(Color::Rgb(220, 38, 38))
            .add_modifier(Modifier::BOLD),
        HealthStatus::Warning => Style::default().fg(Color::Rgb(249, 115, 22)),
        HealthStatus::Elevated => Style::default().fg(Color::Rgb(234, 179, 8)),
        HealthStatus::Healthy => Style::default()
            .fg(Color::Rgb(22, 163, 74))
            .add_modifier(Modifier::BOLD),
    }
}

pub fn status_icon(status: &HealthStatus) -> &'static str {
    match status {
        HealthStatus::Critical => "🔥",
        HealthStatus::Warning => "⚠ ",
        HealthStatus::Elevated => "⚡",
        HealthStatus::Healthy => "✓ ",
    }
}

pub fn grade_style(grade: char) -> Style {
    match grade {
        'A' => Style::default()
            .fg(Color::Rgb(22, 163, 74))
            .add_modifier(Modifier::BOLD),
        'B' => Style::default().fg(Color::Rgb(132, 204, 22)),
        'C' => Style::default().fg(Color::Rgb(234, 179, 8)),
        'D' => Style::default().fg(Color::Rgb(249, 115, 22)),
        _ => Style::default()
            .fg(Color::Rgb(220, 38, 38))
            .add_modifier(Modifier::BOLD),
    }
}

pub fn score_bar_color(score: u8) -> Color {
    health_color(score)
}

pub fn focused_border_style() -> Style {
    Style::default().fg(Color::Cyan)
}

pub fn normal_border_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn header_style() -> Style {
    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD)
}

/// Color a resource-usage cell by raw percentage (used in pod table CPU/Mem columns)
pub fn pct_style(pct: u8) -> Style {
    if pct >= 85 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if pct >= 75 {
        Style::default().fg(Color::Yellow)
    } else if pct >= 60 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Green)
    }
}

pub fn heat_color(pct: u8) -> Color {
    match pct {
        0..=30 => Color::Rgb(61, 214, 208),
        31..=55 => Color::Rgb(115, 226, 122),
        56..=70 => Color::Rgb(244, 208, 63),
        71..=84 => Color::Rgb(255, 159, 67),
        _ => Color::Rgb(255, 99, 72),
    }
}

pub fn health_color(score: u8) -> Color {
    match score {
        90..=100 => Color::Rgb(22, 163, 74),
        70..=89 => Color::Rgb(132, 204, 22),
        50..=69 => Color::Rgb(234, 179, 8),
        30..=49 => Color::Rgb(249, 115, 22),
        _ => Color::Rgb(220, 38, 38),
    }
}

/// Pre-computed utilization gradient colors for common widths (up to 50)
/// This avoids recalculating colors on every render
const MAX_CACHED_WIDTH: usize = 50;

fn precompute_utilization_colors() -> [[Color; MAX_CACHED_WIDTH]; MAX_CACHED_WIDTH] {
    let mut cache = [[Color::Rgb(0, 0, 0); MAX_CACHED_WIDTH]; MAX_CACHED_WIDTH];
    
    for width in 1..=MAX_CACHED_WIDTH {
        for step in 0..width {
            cache[width - 1][step] = compute_utilization_gradient_color(step, width);
        }
    }
    
    cache
}

fn compute_utilization_gradient_color(step: usize, total_steps: usize) -> Color {
    if total_steps <= 1 {
        return Color::Rgb(61, 214, 208);
    }

    let ramp = [
        (61u8, 214u8, 208u8),
        (115, 226, 122),
        (244, 208, 63),
        (255, 159, 67),
        (255, 99, 72),
    ];
    let scaled = step as f32 / (total_steps - 1) as f32;
    let segment = (scaled * (ramp.len() - 1) as f32).floor() as usize;
    let segment = segment.min(ramp.len() - 2);
    let local = scaled * (ramp.len() - 1) as f32 - segment as f32;
    let (r1, g1, b1) = ramp[segment];
    let (r2, g2, b2) = ramp[segment + 1];

    Color::Rgb(
        (r1 as f32 + (r2 as f32 - r1 as f32) * local).round() as u8,
        (g1 as f32 + (g2 as f32 - g1 as f32) * local).round() as u8,
        (b1 as f32 + (b2 as f32 - b1 as f32) * local).round() as u8,
    )
}

pub fn utilization_gradient_color(step: usize, total_steps: usize) -> Color {
    // Use cached values for common widths
    if total_steps <= MAX_CACHED_WIDTH && step < total_steps {
        static CACHE: std::sync::OnceLock<[[Color; MAX_CACHED_WIDTH]; MAX_CACHED_WIDTH]> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(precompute_utilization_colors);
        return cache[total_steps - 1][step];
    }
    
    compute_utilization_gradient_color(step, total_steps)
}

fn precompute_health_colors() -> [[Color; MAX_CACHED_WIDTH]; MAX_CACHED_WIDTH] {
    let mut cache = [[Color::Rgb(0, 0, 0); MAX_CACHED_WIDTH]; MAX_CACHED_WIDTH];
    
    for width in 1..=MAX_CACHED_WIDTH {
        for step in 0..width {
            cache[width - 1][step] = compute_health_gradient_color(step, width);
        }
    }
    
    cache
}

fn compute_health_gradient_color(step: usize, total_steps: usize) -> Color {
    if total_steps <= 1 {
        return health_color(100);
    }

    let ramp = [
        (220u8, 38u8, 38u8),
        (249, 115, 22),
        (234, 179, 8),
        (132, 204, 22),
        (22, 163, 74),
    ];
    let scaled = step as f32 / (total_steps - 1) as f32;
    let segment = (scaled * (ramp.len() - 1) as f32).floor() as usize;
    let segment = segment.min(ramp.len() - 2);
    let local = scaled * (ramp.len() - 1) as f32 - segment as f32;
    let (r1, g1, b1) = ramp[segment];
    let (r2, g2, b2) = ramp[segment + 1];

    Color::Rgb(
        (r1 as f32 + (r2 as f32 - r1 as f32) * local).round() as u8,
        (g1 as f32 + (g2 as f32 - g1 as f32) * local).round() as u8,
        (b1 as f32 + (b2 as f32 - b1 as f32) * local).round() as u8,
    )
}

pub fn health_gradient_color(step: usize, total_steps: usize) -> Color {
    // Use cached values for common widths
    if total_steps <= MAX_CACHED_WIDTH && step < total_steps {
        static CACHE: std::sync::OnceLock<[[Color; MAX_CACHED_WIDTH]; MAX_CACHED_WIDTH]> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(precompute_health_colors);
        return cache[total_steps - 1][step];
    }
    
    compute_health_gradient_color(step, total_steps)
}

pub fn gradient_bar(pct: u8, width: usize) -> Line<'static> {
    // Use saturating multiplication to prevent overflow
    let filled = ((pct as usize).saturating_mul(width) / 100).min(width);
    let mut spans = Vec::with_capacity(width + 2);
    spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));

    for idx in 0..width {
        let span = if idx < filled {
            Span::styled("█", Style::default().fg(utilization_gradient_color(idx, width)))
        } else {
            Span::styled("░", Style::default().fg(Color::DarkGray))
        };
        spans.push(span);
    }

    spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
    Line::from(spans)
}

pub fn health_bar(score: u8, width: usize) -> Line<'static> {
    let filled = (score as usize * width / 100).min(width);
    let mut spans = Vec::with_capacity(width + 2);
    spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));

    for idx in 0..width {
        let span = if idx < filled {
            Span::styled("█", Style::default().fg(health_gradient_color(idx, width)))
        } else {
            Span::styled("░", Style::default().fg(Color::DarkGray))
        };
        spans.push(span);
    }

    spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
    Line::from(spans)
}

pub fn sparkline(history: Option<&Vec<u8>>, width: usize) -> Line<'static> {
    let Some(samples) = history else {
        return placeholder_sparkline(width);
    };

    if samples.is_empty() {
        return placeholder_sparkline(width);
    }

    let glyphs = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let start = samples.len().saturating_sub(width);
    let visible = &samples[start..];
    let mut spans = Vec::with_capacity(width);

    // Pre-calculate padding needed for O(n) instead of O(n²)
    let padding_needed = width.saturating_sub(visible.len());
    for _ in 0..padding_needed {
        spans.push(Span::styled("·", Style::default().fg(Color::DarkGray)));
    }

    for &pct in visible {
        let glyph_index = ((pct as usize * (glyphs.len() - 1)) / 100).min(glyphs.len() - 1);
        spans.push(Span::styled(
            glyphs[glyph_index].to_string(),
            Style::default().fg(heat_color(pct)),
        ));
    }

    Line::from(spans)
}

fn placeholder_sparkline(width: usize) -> Line<'static> {
    Line::from(vec![Span::styled(
        "·".repeat(width),
        Style::default().fg(Color::DarkGray),
    )])
}

pub fn log_level_style(line: &str) -> Style {
    let line_upper = line.to_uppercase();
    if line_upper.contains("ERROR") || line_upper.contains("FATAL") || line_upper.contains("CRITICAL") {
        Style::default().fg(Color::Red)
    } else if line_upper.contains("WARN") {
        Style::default().fg(Color::Yellow)
    } else if line_upper.contains("INFO") {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    }
}
