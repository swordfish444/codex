use crate::color::blend;
use crate::color::is_light;
use crate::color::perceptual_distance;
use crate::terminal_palette::terminal_palette;
use ratatui::style::Color;
use ratatui::style::Style;

/// Returns the style for a user-authored message using the provided terminal background.
pub fn user_message_style(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    match terminal_bg {
        Some(bg) => Style::default().bg(user_message_bg(bg)),
        None => Style::default(),
    }
}

#[allow(clippy::disallowed_methods)]
/// Derives a background color for the user input field that contrasts with the terminal.
///
/// The goal is to blend the user's default terminal background with either black or
/// white (depending on whether the background is light/dark) so the composer area feels
/// consistent with the host theme. The function progressively falls back to palettes of
/// decreasing fidelity depending on what the runtime reports about color support.
pub fn user_message_bg(terminal_bg: (u8, u8, u8)) -> Color {
    // Determine a reference "top" color to blend toward. For dark backgrounds we lighten
    // the mix with white, and for light backgrounds we darken it with black.
    let top = if is_light(terminal_bg) {
        (0, 0, 0)
    } else {
        (255, 255, 255)
    };
    let bottom = terminal_bg;
    let Some(color_level) = supports_color::on_cached(supports_color::Stream::Stdout) else {
        return Color::default();
    };

    // Blend 10% toward the contrasting top color to create a subtle shading effect that
    // keeps the composer distinct without overwhelming the terminal theme.
    let target = blend(top, bottom, 0.1);
    if color_level.has_16m {
        // In truecolor terminals we can use the exact RGB value.
        let (r, g, b) = target;
        Color::Rgb(r, g, b)
    } else if color_level.has_256
        && let Some(palette) = terminal_palette()
        && let Some((i, _)) = palette.into_iter().enumerate().min_by(|(_, a), (_, b)| {
            perceptual_distance(*a, target)
                .partial_cmp(&perceptual_distance(*b, target))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        // If we have a captured 256-color palette, pick whichever indexed color is
        // perceptually closest to the blended target.
        Color::Indexed(i as u8)
    } else if color_level.has_basic {
        // Finally, degrade to the basic 16 ANSI colors using a perceptual distance match.
        closest_basic_color(target)
    } else {
        // If the runtime reports no color support at all, keep the default background to
        // avoid rendering garbage escape sequences.
        Color::default()
    }
}

fn closest_basic_color(target: (u8, u8, u8)) -> Color {
    // Iterate through the baked-in ANSI colors and return whichever one is closest to the
    // desired RGB shade. This mirrors the logic used for the 256-color lookup but avoids
    // allocating an array for each call.
    BASIC_TERMINAL_COLORS
        .iter()
        .min_by(|(_, a), (_, b)| {
            perceptual_distance(*a, target)
                .partial_cmp(&perceptual_distance(*b, target))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(color, _)| *color)
        .unwrap_or(Color::default())
}

// Mapping of ANSI color indices to approximate RGB tuples. These values mirror Windows'
// default console palette so the fallback path stays visually consistent across platforms.
const BASIC_TERMINAL_COLORS: [(Color, (u8, u8, u8)); 16] = [
    (Color::Black, (0, 0, 0)),
    (Color::Blue, (0, 0, 128)),
    (Color::Green, (0, 128, 0)),
    (Color::Cyan, (0, 128, 128)),
    (Color::Red, (128, 0, 0)),
    (Color::Magenta, (128, 0, 128)),
    (Color::Yellow, (128, 128, 0)),
    (Color::Gray, (192, 192, 192)),
    (Color::DarkGray, (128, 128, 128)),
    (Color::LightBlue, (0, 0, 255)),
    (Color::LightGreen, (0, 255, 0)),
    (Color::LightCyan, (0, 255, 255)),
    (Color::LightRed, (255, 0, 0)),
    (Color::LightMagenta, (255, 0, 255)),
    (Color::LightYellow, (255, 255, 0)),
    (Color::White, (255, 255, 255)),
];
