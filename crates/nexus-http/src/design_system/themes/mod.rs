//! Theme definitions for the design system.
//!
//! Themes control color tokens, typography, border radius, and spacing
//! overrides applied to design blocks at composition time.

pub mod bold;
pub mod elegant;
pub mod modern;

use serde::{Deserialize, Serialize};

/// A design system theme — controls the visual identity of generated pages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    /// Theme identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Primary color HSL value (e.g., "221 83% 53%").
    pub primary: String,
    /// Secondary color HSL value.
    pub secondary: String,
    /// Background HSL value.
    pub background: String,
    /// Foreground HSL value.
    pub foreground: String,
    /// Muted color HSL value.
    pub muted: String,
    /// Border radius token (e.g., "0.5rem", "0.75rem").
    pub radius: String,
    /// Font family class for the page body.
    pub font_class: String,
    /// Whether this is a dark theme.
    pub is_dark: bool,
}

impl Theme {
    /// Create the default Modern theme.
    pub fn modern() -> Self {
        modern::theme()
    }

    /// Create the Bold theme.
    pub fn bold() -> Self {
        bold::theme()
    }

    /// Create the Elegant theme.
    pub fn elegant() -> Self {
        elegant::theme()
    }

    /// List all built-in themes.
    pub fn all() -> Vec<Self> {
        vec![Self::modern(), Self::bold(), Self::elegant()]
    }
}

/// Apply a theme's color overrides to component source code.
///
/// Replaces generic color tokens with theme-specific values where applicable.
/// This is a light-touch operation — most theming happens via CSS custom
/// properties at runtime, but some components embed color values directly.
pub fn apply_theme(component_code: &str, theme: &Theme) -> String {
    let mut code = component_code.to_string();

    // For dark themes, swap light-mode-only classes to dark variants
    if theme.is_dark {
        code = code.replace("bg-background", "bg-zinc-950");
    }

    // Inject a theme annotation comment at the top
    if code.starts_with('"') {
        // "use client"; directive — insert after it
        if let Some(pos) = code.find('\n') {
            let after = pos + 1;
            code.insert_str(
                after,
                &format!("\n// Theme: {} ({})\n", theme.name, theme.id),
            );
        }
    }

    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_themes_have_unique_ids() {
        let themes = Theme::all();
        let ids: Vec<&str> = themes.iter().map(|t| t.id.as_str()).collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len(), "Theme IDs must be unique");
    }

    #[test]
    fn apply_theme_adds_annotation() {
        let code = r#""use client";

export function Foo() { return <div />; }
"#;
        let themed = apply_theme(code, &Theme::modern());
        assert!(themed.contains("// Theme: Modern"));
    }
}
