use ratatui::{
    style::{Color, Style},
    text::Span,
};

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const GRADIENT: [Color; 4] = [
    Color::Rgb(94, 234, 212),
    Color::Rgb(56, 189, 248),
    Color::Rgb(129, 140, 248),
    Color::Rgb(192, 132, 252),
];

/// Returns a four-glyph Braille spinner with a teal-to-violet gradient.
pub fn spans(frame: usize) -> Vec<Span<'static>> {
    GRADIENT
        .iter()
        .enumerate()
        .map(|(offset, color)| {
            let glyph = FRAMES[(frame + offset) % FRAMES.len()];
            Span::styled(glyph, Style::default().fg(*color))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_returns_a_four_glyph_gradient() {
        let spinner = spans(0);

        assert_eq!(spinner.len(), GRADIENT.len());
    }
}
