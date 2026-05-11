//! Elegant theme — serif typography, muted tones, refined spacing.

use super::Theme;

/// A refined theme with serif headings, warm neutral palette, and
/// generous line height. Suitable for editorial sites, luxury brands,
/// and content-heavy applications.
pub fn theme() -> Theme {
    Theme {
        id: "elegant".into(),
        name: "Elegant".into(),
        primary: "38 92% 50%".into(),
        secondary: "350 60% 40%".into(),
        background: "40 20% 98%".into(),
        foreground: "20 15% 15%".into(),
        muted: "30 10% 92%".into(),
        radius: "0.25rem".into(),
        font_class: "font-serif".into(),
        is_dark: false,
    }
}
