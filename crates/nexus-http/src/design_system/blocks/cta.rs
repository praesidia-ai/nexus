//! Call-to-action block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![banner_cta(), card_cta(), newsletter_cta()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn banner_cta() -> DesignBlock {
    DesignBlock {
        id: "cta-banner".into(),
        category: BlockCategory::Cta,
        variant: "cta-banner".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { ArrowRight } from "lucide-react";

export function CtaBanner() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-4xl mx-auto text-center">
        <h2 className="text-3xl md:text-5xl font-bold tracking-tight mb-6">
          Ready to get started?
        </h2>
        <p className="text-lg text-muted-foreground max-w-xl mx-auto mb-10">
          Join thousands of teams already building with our platform. Start
          your free trial today — no credit card required.
        </p>
        <div className="flex flex-col sm:flex-row gap-4 justify-center">
          <Button size="lg" className="text-base px-8">
            Start Free Trial
            <ArrowRight className="w-4 h-4 ml-2" />
          </Button>
          <Button size="lg" variant="outline" className="text-base px-8">
            Talk to Sales
          </Button>
        </div>
      </div>
    </section>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into()],
        customization_points: vec![
            cp("title", "CTA headline", "Ready to get started?", CustomizationType::Text),
            cp("ctaText", "Primary button text", "Start Free Trial", CustomizationType::Text),
        ],
    }
}

fn card_cta() -> DesignBlock {
    DesignBlock {
        id: "cta-card".into(),
        category: BlockCategory::Cta,
        variant: "cta-card".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Rocket } from "lucide-react";

export function CtaCard() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-4xl mx-auto">
        <Card className="bg-gradient-to-br from-primary/10 via-background to-secondary/10 border-primary/20">
          <CardContent className="p-12 flex flex-col md:flex-row items-center gap-8">
            <div className="w-16 h-16 rounded-2xl bg-primary/20 flex items-center justify-center shrink-0">
              <Rocket className="w-8 h-8 text-primary" />
            </div>
            <div className="flex-1 text-center md:text-left">
              <h3 className="text-2xl font-bold mb-2">Take your project to the next level</h3>
              <p className="text-muted-foreground">
                Upgrade to Pro and unlock advanced features, priority support, and unlimited projects.
              </p>
            </div>
            <Button size="lg" className="shrink-0 px-8">
              Upgrade Now
            </Button>
          </CardContent>
        </Card>
      </div>
    </section>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "card".into()],
        customization_points: vec![
            cp("title", "CTA headline", "Take your project to the next level", CustomizationType::Text),
            cp("ctaText", "Button text", "Upgrade Now", CustomizationType::Text),
        ],
    }
}

fn newsletter_cta() -> DesignBlock {
    DesignBlock {
        id: "cta-newsletter".into(),
        category: BlockCategory::Cta,
        variant: "cta-newsletter".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Mail } from "lucide-react";

export function CtaNewsletter() {
  return (
    <section className="py-24 px-6 bg-muted/30">
      <div className="max-w-2xl mx-auto text-center">
        <div className="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center mx-auto mb-6">
          <Mail className="w-6 h-6 text-primary" />
        </div>
        <h2 className="text-2xl md:text-3xl font-bold tracking-tight mb-3">
          Stay in the loop
        </h2>
        <p className="text-muted-foreground mb-8">
          Get the latest updates, tips, and product news delivered to your inbox.
          No spam, unsubscribe anytime.
        </p>
        <form className="flex flex-col sm:flex-row gap-3 max-w-md mx-auto">
          <Input
            type="email"
            placeholder="Enter your email"
            className="flex-1"
          />
          <Button type="submit" className="px-6">
            Subscribe
          </Button>
        </form>
      </div>
    </section>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "input".into()],
        customization_points: vec![
            cp("title", "Newsletter heading", "Stay in the loop", CustomizationType::Text),
            cp("placeholder", "Email input placeholder", "Enter your email", CustomizationType::Text),
        ],
    }
}
