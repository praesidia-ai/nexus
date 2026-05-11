//! Navigation bar block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![navbar_sticky(), navbar_transparent()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn navbar_sticky() -> DesignBlock {
    DesignBlock {
        id: "navbar-sticky".into(),
        category: BlockCategory::Navbar,
        variant: "navbar-sticky".into(),
        component_code: r##""use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Menu, X } from "lucide-react";

const navLinks = [
  { label: "Features", href: "#features" },
  { label: "Pricing", href: "#pricing" },
  { label: "Docs", href: "/docs" },
  { label: "Blog", href: "/blog" },
];

export function NavbarSticky() {
  const [mobileOpen, setMobileOpen] = useState(false);

  return (
    <header className="sticky top-0 z-50 w-full border-b border-border/50 bg-background/80 backdrop-blur-lg">
      <nav className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between">
        <a href="/" className="text-xl font-bold tracking-tight">
          Brand
        </a>

        {/* Desktop nav */}
        <div className="hidden md:flex items-center gap-8">
          {navLinks.map((link) => (
            <a
              key={link.label}
              href={link.href}
              className="text-sm text-muted-foreground hover:text-foreground transition-colors"
            >
              {link.label}
            </a>
          ))}
        </div>

        <div className="hidden md:flex items-center gap-3">
          <Button variant="ghost" size="sm">Sign In</Button>
          <Button size="sm">Get Started</Button>
        </div>

        {/* Mobile toggle */}
        <button
          className="md:hidden p-2"
          onClick={() => setMobileOpen(!mobileOpen)}
          aria-label="Toggle menu"
        >
          {mobileOpen ? <X className="w-5 h-5" /> : <Menu className="w-5 h-5" />}
        </button>
      </nav>

      {/* Mobile menu */}
      {mobileOpen && (
        <div className="md:hidden border-t border-border bg-background px-6 py-4 space-y-3">
          {navLinks.map((link) => (
            <a
              key={link.label}
              href={link.href}
              className="block text-sm text-muted-foreground hover:text-foreground"
            >
              {link.label}
            </a>
          ))}
          <div className="flex gap-3 pt-3 border-t border-border">
            <Button variant="ghost" size="sm" className="flex-1">Sign In</Button>
            <Button size="sm" className="flex-1">Get Started</Button>
          </div>
        </div>
      )}
    </header>
  );
}
"##
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into()],
        customization_points: vec![
            cp("brandName", "Brand/logo text", "Brand", CustomizationType::Text),
            cp("navLinks", "Navigation link items", "[]", CustomizationType::LongText),
        ],
    }
}

fn navbar_transparent() -> DesignBlock {
    DesignBlock {
        id: "navbar-transparent".into(),
        category: BlockCategory::Navbar,
        variant: "navbar-transparent".into(),
        component_code: r##""use client";

import { Button } from "@/components/ui/button";

const navLinks = [
  { label: "Product", href: "#product" },
  { label: "Solutions", href: "#solutions" },
  { label: "Resources", href: "#resources" },
  { label: "Company", href: "#company" },
];

export function NavbarTransparent() {
  return (
    <header className="absolute top-0 left-0 right-0 z-50">
      <nav className="max-w-7xl mx-auto px-6 h-20 flex items-center justify-between">
        <a href="/" className="text-xl font-bold tracking-tight">
          Brand
        </a>

        <div className="hidden md:flex items-center gap-8">
          {navLinks.map((link) => (
            <a
              key={link.label}
              href={link.href}
              className="text-sm text-muted-foreground hover:text-foreground transition-colors"
            >
              {link.label}
            </a>
          ))}
        </div>

        <div className="hidden md:flex items-center gap-3">
          <Button variant="ghost" size="sm">Log In</Button>
          <Button size="sm">Sign Up Free</Button>
        </div>
      </nav>
    </header>
  );
}
"##
        .into(),
        required_packages: vec![],
        required_components: vec!["button".into()],
        customization_points: vec![
            cp("brandName", "Brand/logo text", "Brand", CustomizationType::Text),
        ],
    }
}
