//! Palette and type from the "Cluster Desktop" design canvas.

use std::borrow::Cow;

use gpui::{rgb, rgba, App, Rgba};

pub const SANS: &str = "IBM Plex Sans";
pub const MONO: &str = "IBM Plex Mono";

pub fn load_fonts(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-SemiBold.ttf")),
    ];
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("cluster-desktop: failed to load bundled fonts: {err}");
    }
}

pub fn bg() -> Rgba {
    rgb(0x0e1014)
}
pub fn sidebar() -> Rgba {
    rgb(0x12151b)
}
pub fn surface() -> Rgba {
    rgb(0x161a21)
}
pub fn surface_raised() -> Rgba {
    rgb(0x1a1f28)
}
pub fn selected() -> Rgba {
    rgb(0x1e2430)
}
pub fn border() -> Rgba {
    rgb(0x262b35)
}
pub fn border_strong() -> Rgba {
    rgb(0x2b313d)
}
pub fn divider() -> Rgba {
    rgb(0x1f242d)
}
pub fn text() -> Rgba {
    rgb(0xe7e9ee)
}
pub fn text_secondary() -> Rgba {
    rgb(0xc9ced8)
}
pub fn muted() -> Rgba {
    rgb(0xa0a8b6)
}
pub fn faint() -> Rgba {
    rgb(0x7d8696)
}
pub fn accent() -> Rgba {
    rgb(0x8ab8ff)
}
pub fn accent_fill() -> Rgba {
    rgb(0x2f6fdf)
}
pub fn critical() -> Rgba {
    rgb(0xf26d6d)
}
pub fn warning() -> Rgba {
    rgb(0xf5b83d)
}
pub fn bar_normal() -> Rgba {
    rgb(0x5b8fd9)
}
pub fn backdrop() -> Rgba {
    rgba(0x05060899)
}

/// Foreground and tinted background for a grade chip. `?` is "unreachable".
pub fn grade_colors(grade: char) -> (Rgba, Rgba) {
    match grade {
        'A' => (rgb(0x3dd68c), rgba(0x3dd68c24)),
        'B' => (rgb(0xa3dc55), rgba(0xa3dc5524)),
        'C' => (rgb(0xf5b83d), rgba(0xf5b83d24)),
        'D' => (rgb(0xf08c3a), rgba(0xf08c3a29)),
        'F' => (rgb(0xf26d6d), rgba(0xf26d6d29)),
        _ => (rgb(0x7d8696), rgba(0x7d869624)),
    }
}

/// Bar colour for a utilisation percentage, matching the TUI's pressure line.
pub fn heat(pct: u8) -> Rgba {
    if pct >= cluster_core::data::models::RESOURCE_PRESSURE_PCT {
        critical()
    } else if pct >= 70 {
        warning()
    } else {
        bar_normal()
    }
}
