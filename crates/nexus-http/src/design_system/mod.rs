//! Design System — block-based page composition for generated websites.
//!
//! The codegen engine uses this module to compose production-quality websites
//! from pre-built, customizable blocks. Each block is a complete React/Next.js
//! component using shadcn/ui + Tailwind CSS + lucide-react icons.
//!
//! # Architecture
//!
//! ```text
//!   BlockRegistry          Theme            PageComposer
//!       |                    |                   |
//!       +--------+-----------+-------------------+
//!                |
//!         compose_page(blocks, theme) -> Vec<GeneratedFile>
//! ```

pub mod animations;
pub mod blocks;
pub mod css_themes;
pub mod themes;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// Re-export CSS theme functions so existing callers continue to work.
pub use css_themes::{font_imports, generate_globals_css};

// ── Core types ──────────────────────────────────────────────────────────

/// Category of a design block.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockCategory {
    Hero,
    Features,
    Pricing,
    Cta,
    Navbar,
    Footer,
    Auth,
    Dashboard,
    Table,
}

impl std::fmt::Display for BlockCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hero => write!(f, "hero"),
            Self::Features => write!(f, "features"),
            Self::Pricing => write!(f, "pricing"),
            Self::Cta => write!(f, "cta"),
            Self::Navbar => write!(f, "navbar"),
            Self::Footer => write!(f, "footer"),
            Self::Auth => write!(f, "auth"),
            Self::Dashboard => write!(f, "dashboard"),
            Self::Table => write!(f, "table"),
        }
    }
}

/// A customization point that the LLM can adjust when using a block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomizationPoint {
    /// Variable name in the component (e.g., "title", "ctaText").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Default value.
    pub default_value: String,
    /// Type hint for the LLM.
    pub value_type: CustomizationType,
}

/// Type of a customization point value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomizationType {
    Text,
    LongText,
    Color,
    ImageUrl,
    Number,
    Boolean,
    Enum(Vec<String>),
}

/// A pre-built design block — a complete React component with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignBlock {
    /// Unique block identifier (e.g., "hero-gradient-centered").
    pub id: String,
    /// Block category.
    pub category: BlockCategory,
    /// Variant name within the category (e.g., "gradient-centered").
    pub variant: String,
    /// Full React/Next.js component source code.
    pub component_code: String,
    /// npm packages this block requires (e.g., "lucide-react").
    pub required_packages: Vec<String>,
    /// shadcn/ui components this block uses (e.g., "button", "card").
    pub required_components: Vec<String>,
    /// Points the LLM can customize when instantiating the block.
    pub customization_points: Vec<CustomizationPoint>,
}

/// A file to be written to the generated project.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// Relative path within the project (e.g., "src/components/hero.tsx").
    pub path: String,
    /// File content.
    pub content: String,
}

// ── Block Registry ──────────────────────────────────────────────────────

/// Registry of all available design blocks.
pub struct BlockRegistry {
    blocks: Vec<DesignBlock>,
    index: HashMap<String, usize>,
}

impl BlockRegistry {
    /// Create a registry pre-loaded with all built-in blocks.
    pub fn new() -> Self {
        let blocks = Self::builtin_blocks();
        let index = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id.clone(), i))
            .collect();
        Self { blocks, index }
    }

    /// Get a block by its unique ID.
    pub fn get(&self, id: &str) -> Option<&DesignBlock> {
        self.index.get(id).map(|&i| &self.blocks[i])
    }

    /// List all blocks in a given category.
    pub fn by_category(&self, category: &BlockCategory) -> Vec<&DesignBlock> {
        self.blocks
            .iter()
            .filter(|b| &b.category == category)
            .collect()
    }

    /// List all available blocks.
    pub fn all(&self) -> &[DesignBlock] {
        &self.blocks
    }

    /// Collect all required packages across a set of blocks.
    pub fn required_packages(block_ids: &[&str], registry: &BlockRegistry) -> Vec<String> {
        let mut pkgs: Vec<String> = block_ids
            .iter()
            .filter_map(|id| registry.get(id))
            .flat_map(|b| b.required_packages.clone())
            .collect();
        pkgs.sort();
        pkgs.dedup();
        pkgs
    }

    /// Collect all required shadcn components across a set of blocks.
    pub fn required_components(block_ids: &[&str], registry: &BlockRegistry) -> Vec<String> {
        let mut comps: Vec<String> = block_ids
            .iter()
            .filter_map(|id| registry.get(id))
            .flat_map(|b| b.required_components.clone())
            .collect();
        comps.sort();
        comps.dedup();
        comps
    }

    fn builtin_blocks() -> Vec<DesignBlock> {
        let mut all = Vec::new();
        all.extend(blocks::hero::blocks());
        all.extend(blocks::features::blocks());
        all.extend(blocks::pricing::blocks());
        all.extend(blocks::cta::blocks());
        all.extend(blocks::navbar::blocks());
        all.extend(blocks::footer::blocks());
        all.extend(blocks::auth::blocks());
        all.extend(blocks::dashboard::blocks());
        all.extend(blocks::table::blocks());
        all
    }
}

impl Default for BlockRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Page Composer ───────────────────────────────────────────────────────

/// Composes a complete page from a sequence of design blocks and a theme.
pub struct PageComposer;

impl PageComposer {
    /// Compose a set of blocks into generated files, applying the given theme.
    ///
    /// Returns a list of files to write: one per block component, plus a
    /// page file that imports and renders them in order.
    pub fn compose(
        block_ids: &[&str],
        registry: &BlockRegistry,
        theme: &themes::Theme,
        page_path: &str,
    ) -> Vec<GeneratedFile> {
        let mut files = Vec::new();
        let mut imports = Vec::new();
        let mut jsx_elements = Vec::new();

        for block_id in block_ids {
            let Some(block) = registry.get(block_id) else {
                tracing::warn!(block_id, "Block not found in registry, skipping");
                continue;
            };

            let component_name = to_pascal_case(&block.variant);
            let file_name = format!("{}.tsx", block.variant.replace(' ', "-"));
            let component_path = format!("src/components/blocks/{file_name}");

            // Apply theme color overrides to the component code
            let themed_code = themes::apply_theme(&block.component_code, theme);

            files.push(GeneratedFile {
                path: component_path.clone(),
                content: themed_code,
            });

            imports.push(format!(
                "import {{ {component_name} }} from \"@/components/blocks/{}\";",
                file_name.trim_end_matches(".tsx")
            ));
            jsx_elements.push(format!("      <{component_name} />"));
        }

        // Generate the page file that composes all blocks
        let page_content = format!(
            r#""use client";

{imports}

export default function Page() {{
  return (
    <main className="min-h-screen bg-background text-foreground {font_class}">
{elements}
    </main>
  );
}}
"#,
            imports = imports.join("\n"),
            elements = jsx_elements.join("\n"),
            font_class = theme.font_class,
        );

        files.push(GeneratedFile {
            path: page_path.to_string(),
            content: page_content,
        });

        files
    }
}

/// Convert a kebab-case string to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_all_categories() {
        let registry = BlockRegistry::new();
        let all = registry.all();
        assert!(!all.is_empty(), "Registry should have blocks");

        // Check that we have at least one block per category
        for category in &[
            BlockCategory::Hero,
            BlockCategory::Features,
            BlockCategory::Pricing,
            BlockCategory::Cta,
            BlockCategory::Navbar,
            BlockCategory::Footer,
            BlockCategory::Auth,
            BlockCategory::Dashboard,
            BlockCategory::Table,
        ] {
            let blocks = registry.by_category(category);
            assert!(
                !blocks.is_empty(),
                "No blocks found for category: {}",
                category
            );
        }
    }

    #[test]
    fn get_block_by_id() {
        let registry = BlockRegistry::new();
        let block = registry.get("hero-gradient-centered");
        assert!(block.is_some(), "Should find hero-gradient-centered block");
    }

    #[test]
    fn pascal_case_conversion() {
        assert_eq!(to_pascal_case("hero-gradient-centered"), "HeroGradientCentered");
        assert_eq!(to_pascal_case("navbar-sticky"), "NavbarSticky");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn compose_produces_files() {
        let registry = BlockRegistry::new();
        let theme = themes::Theme::modern();
        let files = PageComposer::compose(
            &["hero-gradient-centered", "features-grid-icons"],
            &registry,
            &theme,
            "src/app/page.tsx",
        );
        assert!(files.len() >= 3, "Should produce component files + page file");
        let page = files.iter().find(|f| f.path == "src/app/page.tsx");
        assert!(page.is_some(), "Should include the page file");
    }
}
