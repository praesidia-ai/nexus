//! Design System — rich, production-quality CSS themes for generated apps.
//!
//! Instead of generating `@import 'tailwindcss';` and hoping the LLM
//! produces good styles, we inject a complete design system based on the
//! detected UI style (Luxurious, Playful, Corporate, Technical, Bold, Minimal).
//!
//! Each theme includes:
//! - Color palette (CSS custom properties)
//! - Typography scale
//! - Component styles (buttons, cards, inputs, navigation)
//! - Layout utilities
//! - Animations
//! - Dark/light mode support

use crate::intent_engine::UiStyle;

/// Generate a complete globals.css based on UI style.
pub fn generate_globals_css(style: &UiStyle, app_name: &str) -> String {
    let palette = color_palette(style);
    let typography = typography(style);
    let components = component_styles(style);
    let animations = animations(style);
    let style_name = format!("{:?}", style);

    format!(
        r#"@import 'tailwindcss';

/* ─── {app_name} Design System ─── */
/* Theme: {style_name} */

@layer base {{
  :root {{
    --radius: 0.625rem;
{palette}
{typography}
  }}

  * {{
    box-sizing: border-box;
    border-color: hsl(var(--border));
  }}

  html, body {{
    height: 100%;
    margin: 0;
  }}

  body {{
    background-color: hsl(var(--background));
    color: hsl(var(--foreground));
    font-family: var(--font-body);
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
    line-height: 1.6;
  }}

  h1, h2, h3, h4, h5, h6 {{
    font-family: var(--font-heading);
    line-height: 1.2;
    letter-spacing: var(--heading-tracking);
  }}

  h1 {{ font-size: 3.5rem; font-weight: 700; }}
  h2 {{ font-size: 2.25rem; font-weight: 600; }}
  h3 {{ font-size: 1.5rem; font-weight: 600; }}

  a {{ color: hsl(var(--primary)); text-decoration: none; }}
  a:hover {{ opacity: 0.85; }}
}}

@layer components {{
{components}
{animations}
}}

@layer utilities {{
  .scrollbar-thin {{
    scrollbar-width: thin;
    scrollbar-color: hsl(var(--muted)) transparent;
  }}
  .scrollbar-thin::-webkit-scrollbar {{ width: 4px; }}
  .scrollbar-thin::-webkit-scrollbar-track {{ background: transparent; }}
  .scrollbar-thin::-webkit-scrollbar-thumb {{
    background-color: hsl(var(--muted));
    border-radius: 2px;
  }}
}}
"#,
        app_name = app_name,
        style_name = style_name,
        palette = palette,
        typography = typography,
        components = components,
        animations = animations,
    )
}

fn color_palette(style: &UiStyle) -> String {
    match style {
        UiStyle::Luxurious => r#"
    /* Luxurious — deep burgundy, gold, cream */
    --background: 20 10% 4%;
    --foreground: 40 30% 92%;
    --card: 20 10% 7%;
    --card-foreground: 40 30% 92%;
    --primary: 38 92% 50%;
    --primary-foreground: 20 10% 4%;
    --secondary: 350 60% 40%;
    --secondary-foreground: 40 30% 95%;
    --muted: 20 10% 12%;
    --muted-foreground: 30 15% 55%;
    --accent: 38 92% 50%;
    --accent-foreground: 20 10% 4%;
    --destructive: 0 84% 60%;
    --destructive-foreground: 0 0% 100%;
    --border: 30 10% 15%;
    --input: 20 10% 12%;
    --ring: 38 92% 50%;"#.into(),

        UiStyle::Playful => r#"
    /* Playful — vibrant purple, pink, teal */
    --background: 260 30% 5%;
    --foreground: 0 0% 95%;
    --card: 260 30% 8%;
    --card-foreground: 0 0% 95%;
    --primary: 280 85% 60%;
    --primary-foreground: 0 0% 100%;
    --secondary: 170 80% 45%;
    --secondary-foreground: 0 0% 100%;
    --muted: 260 20% 14%;
    --muted-foreground: 260 10% 55%;
    --accent: 330 85% 60%;
    --accent-foreground: 0 0% 100%;
    --destructive: 0 84% 60%;
    --destructive-foreground: 0 0% 100%;
    --border: 260 20% 16%;
    --input: 260 20% 14%;
    --ring: 280 85% 60%;"#.into(),

        UiStyle::Corporate => r#"
    /* Corporate — navy, slate, clean blue */
    --background: 222 47% 97%;
    --foreground: 222 47% 11%;
    --card: 0 0% 100%;
    --card-foreground: 222 47% 11%;
    --primary: 221 83% 53%;
    --primary-foreground: 0 0% 100%;
    --secondary: 210 40% 96%;
    --secondary-foreground: 222 47% 11%;
    --muted: 210 40% 96%;
    --muted-foreground: 215 20% 50%;
    --accent: 210 40% 96%;
    --accent-foreground: 222 47% 11%;
    --destructive: 0 84% 60%;
    --destructive-foreground: 0 0% 100%;
    --border: 214 32% 91%;
    --input: 214 32% 91%;
    --ring: 221 83% 53%;"#.into(),

        UiStyle::Technical => r#"
    /* Technical — terminal green, dark gray */
    --background: 220 15% 6%;
    --foreground: 120 20% 85%;
    --card: 220 15% 9%;
    --card-foreground: 120 20% 85%;
    --primary: 142 70% 45%;
    --primary-foreground: 220 15% 4%;
    --secondary: 200 60% 40%;
    --secondary-foreground: 0 0% 100%;
    --muted: 220 15% 14%;
    --muted-foreground: 220 10% 50%;
    --accent: 142 70% 45%;
    --accent-foreground: 220 15% 4%;
    --destructive: 0 84% 60%;
    --destructive-foreground: 0 0% 100%;
    --border: 220 15% 16%;
    --input: 220 15% 12%;
    --ring: 142 70% 45%;"#.into(),

        UiStyle::Bold => r#"
    /* Bold — black, white, hot accent */
    --background: 0 0% 2%;
    --foreground: 0 0% 98%;
    --card: 0 0% 5%;
    --card-foreground: 0 0% 98%;
    --primary: 350 90% 55%;
    --primary-foreground: 0 0% 100%;
    --secondary: 45 100% 50%;
    --secondary-foreground: 0 0% 4%;
    --muted: 0 0% 10%;
    --muted-foreground: 0 0% 55%;
    --accent: 350 90% 55%;
    --accent-foreground: 0 0% 100%;
    --destructive: 0 84% 60%;
    --destructive-foreground: 0 0% 100%;
    --border: 0 0% 14%;
    --input: 0 0% 10%;
    --ring: 350 90% 55%;"#.into(),

        UiStyle::Minimal => r#"
    /* Minimal — neutral, clean, spacious */
    --background: 0 0% 100%;
    --foreground: 224 15% 12%;
    --card: 0 0% 100%;
    --card-foreground: 224 15% 12%;
    --primary: 224 70% 45%;
    --primary-foreground: 0 0% 100%;
    --secondary: 220 14% 96%;
    --secondary-foreground: 224 15% 12%;
    --muted: 220 14% 96%;
    --muted-foreground: 220 10% 46%;
    --accent: 220 14% 96%;
    --accent-foreground: 224 15% 12%;
    --destructive: 0 84% 60%;
    --destructive-foreground: 0 0% 100%;
    --border: 220 13% 91%;
    --input: 220 13% 91%;
    --ring: 224 70% 45%;"#.into(),
    }
}

fn typography(style: &UiStyle) -> String {
    match style {
        UiStyle::Luxurious => r#"
    --font-heading: 'Playfair Display', Georgia, serif;
    --font-body: 'Inter', system-ui, sans-serif;
    --heading-tracking: -0.02em;"#.into(),

        UiStyle::Playful => r#"
    --font-heading: 'Nunito', 'Segoe UI', sans-serif;
    --font-body: 'Nunito', 'Segoe UI', sans-serif;
    --heading-tracking: -0.01em;"#.into(),

        UiStyle::Corporate => r#"
    --font-heading: 'Inter', system-ui, sans-serif;
    --font-body: 'Inter', system-ui, sans-serif;
    --heading-tracking: -0.025em;"#.into(),

        UiStyle::Technical => r#"
    --font-heading: 'JetBrains Mono', 'Fira Code', monospace;
    --font-body: 'Inter', system-ui, sans-serif;
    --heading-tracking: -0.03em;"#.into(),

        UiStyle::Bold => r#"
    --font-heading: 'Inter', system-ui, sans-serif;
    --font-body: 'Inter', system-ui, sans-serif;
    --heading-tracking: -0.04em;"#.into(),

        UiStyle::Minimal => r#"
    --font-heading: 'Inter', system-ui, sans-serif;
    --font-body: 'Inter', system-ui, sans-serif;
    --heading-tracking: -0.025em;"#.into(),
    }
}

fn component_styles(style: &UiStyle) -> String {
    let btn = match style {
        UiStyle::Luxurious => r#"
  .btn-primary {
    background: linear-gradient(135deg, hsl(38 92% 50%), hsl(38 80% 40%));
    color: hsl(20 10% 4%);
    padding: 0.75rem 2rem;
    border-radius: 0;
    font-weight: 500;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    font-size: 0.8rem;
    transition: all 0.3s ease;
    border: 1px solid hsl(38 92% 50% / 0.3);
  }
  .btn-primary:hover {
    box-shadow: 0 0 30px hsl(38 92% 50% / 0.2);
    transform: translateY(-1px);
  }"#,
        UiStyle::Playful => r#"
  .btn-primary {
    background: linear-gradient(135deg, hsl(280 85% 60%), hsl(330 85% 60%));
    color: white;
    padding: 0.75rem 1.75rem;
    border-radius: 9999px;
    font-weight: 700;
    font-size: 0.95rem;
    transition: all 0.2s ease;
    box-shadow: 0 4px 15px hsl(280 85% 60% / 0.3);
  }
  .btn-primary:hover {
    transform: translateY(-2px) scale(1.02);
    box-shadow: 0 8px 25px hsl(280 85% 60% / 0.4);
  }"#,
        UiStyle::Corporate => r#"
  .btn-primary {
    background: hsl(221 83% 53%);
    color: white;
    padding: 0.625rem 1.5rem;
    border-radius: 0.375rem;
    font-weight: 500;
    font-size: 0.875rem;
    transition: background 0.15s ease;
  }
  .btn-primary:hover {
    background: hsl(221 83% 46%);
  }"#,
        UiStyle::Bold => r#"
  .btn-primary {
    background: hsl(350 90% 55%);
    color: white;
    padding: 1rem 2.5rem;
    border-radius: 0;
    font-weight: 800;
    font-size: 1rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    transition: all 0.15s ease;
  }
  .btn-primary:hover {
    background: white;
    color: hsl(0 0% 2%);
  }"#,
        _ => r#"
  .btn-primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    padding: 0.625rem 1.25rem;
    border-radius: var(--radius);
    font-weight: 500;
    font-size: 0.875rem;
    transition: opacity 0.15s ease;
  }
  .btn-primary:hover {
    opacity: 0.9;
  }"#,
    };

    let card = match style {
        UiStyle::Luxurious => r#"
  .card {
    background: hsl(var(--card));
    border: 1px solid hsl(38 92% 50% / 0.1);
    border-radius: 0;
    padding: 2rem;
    transition: border-color 0.3s ease;
  }
  .card:hover {
    border-color: hsl(38 92% 50% / 0.25);
  }"#,
        UiStyle::Playful => r#"
  .card {
    background: hsl(var(--card));
    border: 2px solid hsl(var(--border));
    border-radius: 1.25rem;
    padding: 1.5rem;
    transition: all 0.2s ease;
  }
  .card:hover {
    transform: translateY(-4px);
    box-shadow: 0 12px 40px hsl(280 85% 60% / 0.15);
  }"#,
        UiStyle::Corporate => r#"
  .card {
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    padding: 1.5rem;
    box-shadow: 0 1px 3px hsl(0 0% 0% / 0.08);
  }
  .card:hover {
    box-shadow: 0 4px 12px hsl(0 0% 0% / 0.12);
  }"#,
        _ => r#"
  .card {
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius);
    padding: 1.5rem;
    transition: box-shadow 0.15s ease;
  }
  .card:hover {
    box-shadow: 0 2px 8px hsl(0 0% 0% / 0.08);
  }"#,
    };

    let input = r#"
  .input {
    background: hsl(var(--input));
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius);
    padding: 0.625rem 0.875rem;
    font-size: 0.875rem;
    color: hsl(var(--foreground));
    transition: border-color 0.15s ease;
    width: 100%;
  }
  .input:focus {
    outline: none;
    border-color: hsl(var(--ring));
    box-shadow: 0 0 0 3px hsl(var(--ring) / 0.1);
  }
  .input::placeholder {
    color: hsl(var(--muted-foreground));
  }"#;

    let nav = match style {
        UiStyle::Luxurious => r#"
  .nav {
    border-bottom: 1px solid hsl(38 92% 50% / 0.1);
    padding: 1.25rem 2rem;
    backdrop-filter: blur(20px);
  }
  .nav-link {
    color: hsl(var(--muted-foreground));
    font-size: 0.75rem;
    letter-spacing: 0.15em;
    text-transform: uppercase;
    transition: color 0.2s;
  }
  .nav-link:hover, .nav-link.active {
    color: hsl(var(--primary));
  }"#,
        UiStyle::Corporate => r#"
  .nav {
    border-bottom: 1px solid hsl(var(--border));
    padding: 0 1.5rem;
    height: 3.5rem;
    display: flex;
    align-items: center;
    background: hsl(var(--card));
  }
  .nav-link {
    color: hsl(var(--muted-foreground));
    font-size: 0.875rem;
    font-weight: 500;
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    transition: all 0.15s;
  }
  .nav-link:hover, .nav-link.active {
    color: hsl(var(--foreground));
    background: hsl(var(--muted));
  }"#,
        _ => r#"
  .nav {
    border-bottom: 1px solid hsl(var(--border));
    padding: 0.75rem 1.5rem;
    display: flex;
    align-items: center;
    gap: 1.5rem;
  }
  .nav-link {
    color: hsl(var(--muted-foreground));
    font-size: 0.875rem;
    transition: color 0.15s;
  }
  .nav-link:hover, .nav-link.active {
    color: hsl(var(--foreground));
  }"#,
    };

    // Hero section styles
    let hero = match style {
        UiStyle::Luxurious => r#"
  .hero {
    min-height: 80vh;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    text-align: center;
    padding: 4rem 2rem;
    background: radial-gradient(ellipse at 50% 0%, hsl(38 92% 50% / 0.06) 0%, transparent 70%);
  }
  .hero h1 {
    font-size: 4.5rem;
    background: linear-gradient(135deg, hsl(40 30% 92%), hsl(38 92% 50%));
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }
  .hero p {
    color: hsl(var(--muted-foreground));
    font-size: 1.125rem;
    max-width: 36rem;
    margin-top: 1.5rem;
  }"#,
        UiStyle::Bold => r#"
  .hero {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 4rem 3rem;
  }
  .hero h1 {
    font-size: 6rem;
    line-height: 0.95;
    text-transform: uppercase;
    letter-spacing: -0.04em;
  }
  .hero p {
    font-size: 1.25rem;
    margin-top: 2rem;
    max-width: 32rem;
    color: hsl(var(--muted-foreground));
  }"#,
        _ => r#"
  .hero {
    min-height: 70vh;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    text-align: center;
    padding: 4rem 2rem;
  }
  .hero h1 {
    font-size: 3.5rem;
  }
  .hero p {
    color: hsl(var(--muted-foreground));
    font-size: 1.125rem;
    max-width: 36rem;
    margin-top: 1rem;
  }"#,
    };

    let section = r#"
  .section {
    padding: 5rem 2rem;
    max-width: 72rem;
    margin: 0 auto;
  }
  .section-title {
    font-size: 2rem;
    font-weight: 700;
    margin-bottom: 0.75rem;
  }
  .section-subtitle {
    color: hsl(var(--muted-foreground));
    font-size: 1.1rem;
    margin-bottom: 3rem;
  }
  .grid-3 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
    gap: 1.5rem;
  }"#;

    format!("{btn}\n{card}\n{input}\n{nav}\n{hero}\n{section}")
}

fn animations(style: &UiStyle) -> String {
    let base = r#"
  @keyframes fade-in {
    from { opacity: 0; transform: translateY(10px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .animate-fade-in {
    animation: fade-in 0.5s ease forwards;
  }"#;

    let extra = match style {
        UiStyle::Luxurious => r#"
  @keyframes shimmer {
    0% { background-position: -200% 0; }
    100% { background-position: 200% 0; }
  }
  .shimmer {
    background: linear-gradient(90deg, transparent, hsl(38 92% 50% / 0.1), transparent);
    background-size: 200% 100%;
    animation: shimmer 3s ease infinite;
  }"#,
        UiStyle::Playful => r#"
  @keyframes bounce-in {
    0% { transform: scale(0.9); opacity: 0; }
    60% { transform: scale(1.02); }
    100% { transform: scale(1); opacity: 1; }
  }
  .animate-bounce-in {
    animation: bounce-in 0.4s ease forwards;
  }"#,
        _ => "",
    };

    format!("{base}\n{extra}")
}

/// Generate the layout.tsx with proper font imports based on style.
pub fn font_imports(style: &UiStyle) -> &'static str {
    match style {
        UiStyle::Luxurious => r#"<link href="https://fonts.googleapis.com/css2?family=Playfair+Display:wght@400;500;600;700&family=Inter:wght@300;400;500;600&display=swap" rel="stylesheet" />"#,
        UiStyle::Playful => r#"<link href="https://fonts.googleapis.com/css2?family=Nunito:wght@400;600;700;800&display=swap" rel="stylesheet" />"#,
        UiStyle::Technical => r#"<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600;700&family=Inter:wght@300;400;500;600&display=swap" rel="stylesheet" />"#,
        _ => r#"<link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700;800&display=swap" rel="stylesheet" />"#,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_different_palettes() {
        let luxurious = generate_globals_css(&UiStyle::Luxurious, "Wine App");
        let corporate = generate_globals_css(&UiStyle::Corporate, "Corp App");
        let playful = generate_globals_css(&UiStyle::Playful, "Kids App");

        // Each should have different primary colors
        assert!(luxurious.contains("38 92% 50%")); // Gold
        assert!(corporate.contains("221 83% 53%")); // Blue
        assert!(playful.contains("280 85% 60%")); // Purple

        // Luxurious should have serif heading font
        assert!(luxurious.contains("Playfair Display"));
        // Corporate should have sans-serif
        assert!(corporate.contains("Inter"));
        // Playful should have rounded font
        assert!(playful.contains("Nunito"));
    }

    #[test]
    fn luxurious_has_gold_buttons() {
        let css = generate_globals_css(&UiStyle::Luxurious, "Test");
        assert!(css.contains("text-transform: uppercase"));
        assert!(css.contains("letter-spacing: 0.1em"));
    }

    #[test]
    fn playful_has_rounded_elements() {
        let css = generate_globals_css(&UiStyle::Playful, "Test");
        assert!(css.contains("border-radius: 9999px")); // pill buttons
        assert!(css.contains("border-radius: 1.25rem")); // rounded cards
    }

    #[test]
    fn corporate_is_light_mode() {
        let css = generate_globals_css(&UiStyle::Corporate, "Test");
        assert!(css.contains("--background: 222 47% 97%")); // light bg
    }

    #[test]
    fn all_styles_have_required_components() {
        for style in &[
            UiStyle::Luxurious,
            UiStyle::Playful,
            UiStyle::Corporate,
            UiStyle::Technical,
            UiStyle::Bold,
            UiStyle::Minimal,
        ] {
            let css = generate_globals_css(style, "Test");
            assert!(css.contains(".btn-primary"), "Missing btn-primary for {:?}", style);
            assert!(css.contains(".card"), "Missing card for {:?}", style);
            assert!(css.contains(".input"), "Missing input for {:?}", style);
            assert!(css.contains(".nav"), "Missing nav for {:?}", style);
            assert!(css.contains(".hero"), "Missing hero for {:?}", style);
            assert!(css.contains("--background"), "Missing background for {:?}", style);
            assert!(css.contains("--font-heading"), "Missing font-heading for {:?}", style);
        }
    }
}
