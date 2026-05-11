//! Bold theme — strong colors, dark mode, high contrast.

use super::Theme;

/// A high-contrast dark theme with vibrant accent colors, tight spacing,
/// and uppercase headings. Suitable for creative portfolios, landing
/// pages, and marketing sites.
pub fn theme() -> Theme {
    Theme {
        id: "bold".into(),
        name: "Bold".into(),
        primary: "350 90% 55%".into(),
        secondary: "45 100% 50%".into(),
        background: "0 0% 2%".into(),
        foreground: "0 0% 98%".into(),
        muted: "0 0% 10%".into(),
        radius: "0rem".into(),
        font_class: "font-sans".into(),
        is_dark: true,
    }
}
