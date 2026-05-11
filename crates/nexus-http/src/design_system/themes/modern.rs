//! Modern theme — clean, minimal, professional.

use super::Theme;

/// A clean, minimal theme with blue primary, generous whitespace, and
/// Inter font family. Suitable for SaaS products, developer tools,
/// and business applications.
pub fn theme() -> Theme {
    Theme {
        id: "modern".into(),
        name: "Modern".into(),
        primary: "221 83% 53%".into(),
        secondary: "210 40% 96%".into(),
        background: "0 0% 100%".into(),
        foreground: "224 15% 12%".into(),
        muted: "220 14% 96%".into(),
        radius: "0.625rem".into(),
        font_class: "font-sans".into(),
        is_dark: false,
    }
}
