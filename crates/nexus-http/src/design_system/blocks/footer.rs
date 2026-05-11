//! Footer block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![footer_columns(), footer_simple()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn footer_columns() -> DesignBlock {
    DesignBlock {
        id: "footer-columns".into(),
        category: BlockCategory::Footer,
        variant: "footer-columns".into(),
        component_code: r##""use client";

const footerLinks = {
  Product: ["Features", "Pricing", "Integrations", "Changelog"],
  Company: ["About", "Blog", "Careers", "Press"],
  Resources: ["Documentation", "Help Center", "Community", "Templates"],
  Legal: ["Privacy", "Terms", "Security", "Cookies"],
};

export function FooterColumns() {
  return (
    <footer className="border-t border-border bg-muted/30">
      <div className="max-w-7xl mx-auto px-6 py-16">
        <div className="grid grid-cols-2 md:grid-cols-5 gap-8">
          {/* Brand column */}
          <div className="col-span-2 md:col-span-1">
            <a href="/" className="text-lg font-bold tracking-tight">
              Brand
            </a>
            <p className="text-sm text-muted-foreground mt-3 leading-relaxed">
              Building the future of team productivity, one feature at a time.
            </p>
          </div>

          {Object.entries(footerLinks).map(([category, links]) => (
            <div key={category}>
              <h4 className="text-sm font-semibold mb-4">{category}</h4>
              <ul className="space-y-2.5">
                {links.map((link) => (
                  <li key={link}>
                    <a
                      href="#"
                      className="text-sm text-muted-foreground hover:text-foreground transition-colors"
                    >
                      {link}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="mt-12 pt-8 border-t border-border flex flex-col md:flex-row justify-between items-center gap-4">
          <p className="text-xs text-muted-foreground">
            &copy; {new Date().getFullYear()} Brand. All rights reserved.
          </p>
          <div className="flex gap-6">
            <a href="#" className="text-xs text-muted-foreground hover:text-foreground">Twitter</a>
            <a href="#" className="text-xs text-muted-foreground hover:text-foreground">GitHub</a>
            <a href="#" className="text-xs text-muted-foreground hover:text-foreground">LinkedIn</a>
          </div>
        </div>
      </div>
    </footer>
  );
}
"##
        .into(),
        required_packages: vec![],
        required_components: vec![],
        customization_points: vec![
            cp("brandName", "Brand name in footer", "Brand", CustomizationType::Text),
            cp("tagline", "Brand tagline", "Building the future...", CustomizationType::Text),
        ],
    }
}

fn footer_simple() -> DesignBlock {
    DesignBlock {
        id: "footer-simple".into(),
        category: BlockCategory::Footer,
        variant: "footer-simple".into(),
        component_code: r##""use client";

const links = ["Features", "Pricing", "Docs", "Blog", "Privacy", "Terms"];

export function FooterSimple() {
  return (
    <footer className="border-t border-border">
      <div className="max-w-7xl mx-auto px-6 py-8 flex flex-col md:flex-row justify-between items-center gap-4">
        <a href="/" className="text-sm font-semibold">Brand</a>
        <nav className="flex flex-wrap gap-6">
          {links.map((link) => (
            <a
              key={link}
              href="#"
              className="text-sm text-muted-foreground hover:text-foreground transition-colors"
            >
              {link}
            </a>
          ))}
        </nav>
        <p className="text-xs text-muted-foreground">
          &copy; {new Date().getFullYear()} Brand
        </p>
      </div>
    </footer>
  );
}
"##
        .into(),
        required_packages: vec![],
        required_components: vec![],
        customization_points: vec![
            cp("brandName", "Brand name", "Brand", CustomizationType::Text),
            cp("links", "Footer link labels", "[]", CustomizationType::LongText),
        ],
    }
}
