//! Feature section block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![grid_icons(), bento_grid(), alternating_rows()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn grid_icons() -> DesignBlock {
    DesignBlock {
        id: "features-grid-icons".into(),
        category: BlockCategory::Features,
        variant: "features-grid-icons".into(),
        component_code: r#""use client";

import { Card, CardContent } from "@/components/ui/card";
import { Zap, Shield, Globe, BarChart3, Lock, Cpu } from "lucide-react";

const features = [
  { icon: Zap, title: "Lightning Fast", description: "Optimized for speed with edge-first architecture." },
  { icon: Shield, title: "Enterprise Security", description: "SOC 2 compliant with end-to-end encryption." },
  { icon: Globe, title: "Global Scale", description: "Deploy to 50+ regions with automatic failover." },
  { icon: BarChart3, title: "Real-time Analytics", description: "Customizable dashboards and alerts." },
  { icon: Lock, title: "Access Control", description: "Fine-grained permissions with SSO support." },
  { icon: Cpu, title: "AI Powered", description: "Built-in AI for automation and insights." },
];

export function FeaturesGridIcons() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-6xl mx-auto">
        <div className="text-center mb-16">
          <p className="text-sm font-semibold text-primary uppercase tracking-wider mb-3">Features</p>
          <h2 className="text-3xl md:text-4xl font-bold tracking-tight mb-4">Everything you need to succeed</h2>
          <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
            A comprehensive toolkit designed to help your team move faster and build better products.
          </p>
        </div>
        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-6">
          {features.map((feature) => (
            <Card key={feature.title} className="group hover:shadow-lg transition-all duration-300 hover:-translate-y-1 border-border/50">
              <CardContent className="p-6">
                <div className="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center mb-4 group-hover:bg-primary/20 transition-colors">
                  <feature.icon className="w-6 h-6 text-primary" />
                </div>
                <h3 className="text-lg font-semibold mb-2">{feature.title}</h3>
                <p className="text-muted-foreground text-sm leading-relaxed">{feature.description}</p>
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["card".into()],
        customization_points: vec![
            cp("sectionTitle", "Section heading", "Everything you need to succeed", CustomizationType::Text),
        ],
    }
}

fn bento_grid() -> DesignBlock {
    DesignBlock {
        id: "features-bento-grid".into(),
        category: BlockCategory::Features,
        variant: "features-bento-grid".into(),
        component_code: r#""use client";

import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Sparkles, Layers, Gauge, GitBranch } from "lucide-react";

export function FeaturesBentoGrid() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-6xl mx-auto">
        <div className="text-center mb-16">
          <Badge variant="secondary" className="mb-4">Platform</Badge>
          <h2 className="text-3xl md:text-4xl font-bold tracking-tight mb-4">Designed for modern teams</h2>
        </div>
        <div className="grid md:grid-cols-4 gap-4 auto-rows-[180px]">
          <Card className="md:col-span-2 md:row-span-2 group hover:shadow-lg transition-all border-border/50">
            <CardContent className="p-8 h-full flex flex-col justify-between">
              <div>
                <div className="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center mb-4">
                  <Sparkles className="w-6 h-6 text-primary" />
                </div>
                <h3 className="text-xl font-semibold mb-2">AI-Native Workflows</h3>
                <p className="text-muted-foreground text-sm leading-relaxed">
                  Every feature is designed with AI at its core. Automate repetitive tasks and get intelligent suggestions.
                </p>
              </div>
              <div className="mt-4 h-20 rounded-lg bg-gradient-to-br from-primary/5 to-primary/10 border border-primary/10" />
            </CardContent>
          </Card>
          <Card className="md:col-span-2 group hover:shadow-lg transition-all border-border/50">
            <CardContent className="p-6 h-full flex items-center gap-4">
              <div className="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center shrink-0">
                <Layers className="w-5 h-5 text-primary" />
              </div>
              <div>
                <h3 className="font-semibold mb-1">Component Library</h3>
                <p className="text-muted-foreground text-sm">200+ pre-built, customizable components.</p>
              </div>
            </CardContent>
          </Card>
          <Card className="group hover:shadow-lg transition-all border-border/50">
            <CardContent className="p-6 h-full flex flex-col justify-center">
              <Gauge className="w-8 h-8 text-primary mb-3" />
              <h3 className="font-semibold mb-1">99.99% Uptime</h3>
              <p className="text-muted-foreground text-xs">Enterprise-grade reliability.</p>
            </CardContent>
          </Card>
          <Card className="group hover:shadow-lg transition-all border-border/50">
            <CardContent className="p-6 h-full flex flex-col justify-center">
              <GitBranch className="w-8 h-8 text-primary mb-3" />
              <h3 className="font-semibold mb-1">Version Control</h3>
              <p className="text-muted-foreground text-xs">Built-in branching and rollback.</p>
            </CardContent>
          </Card>
        </div>
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["card".into(), "badge".into()],
        customization_points: vec![
            cp("sectionTitle", "Section heading", "Designed for modern teams", CustomizationType::Text),
        ],
    }
}

fn alternating_rows() -> DesignBlock {
    DesignBlock {
        id: "features-alternating-rows".into(),
        category: BlockCategory::Features,
        variant: "features-alternating-rows".into(),
        component_code: r#""use client";

import { Badge } from "@/components/ui/badge";
import { CheckCircle2 } from "lucide-react";

const features = [
  {
    badge: "Collaboration",
    title: "Work together in real-time",
    description: "Multiple team members can edit simultaneously with live cursors and instant sync.",
    bullets: ["Real-time cursors", "Comment threads", "Change history"],
  },
  {
    badge: "Automation",
    title: "Automate your workflows",
    description: "Set up triggers, conditions, and actions to automate repetitive tasks.",
    bullets: ["Custom triggers", "Conditional logic", "100+ integrations"],
  },
  {
    badge: "Analytics",
    title: "Insights that drive decisions",
    description: "Comprehensive analytics with customizable dashboards and real-time metrics.",
    bullets: ["Custom dashboards", "Real-time metrics", "Scheduled reports"],
  },
];

export function FeaturesAlternatingRows() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-6xl mx-auto space-y-32">
        {features.map((feature, index) => (
          <div key={feature.title} className={`flex flex-col lg:flex-row gap-12 items-center ${index % 2 === 1 ? "lg:flex-row-reverse" : ""}`}>
            <div className="flex-1">
              <Badge variant="secondary" className="mb-4">{feature.badge}</Badge>
              <h3 className="text-3xl font-bold tracking-tight mb-4">{feature.title}</h3>
              <p className="text-muted-foreground mb-6 leading-relaxed">{feature.description}</p>
              <ul className="space-y-3">
                {feature.bullets.map((bullet) => (
                  <li key={bullet} className="flex items-center gap-3 text-sm">
                    <CheckCircle2 className="w-4 h-4 text-primary shrink-0" />
                    {bullet}
                  </li>
                ))}
              </ul>
            </div>
            <div className="flex-1">
              <div className="aspect-[4/3] rounded-2xl bg-muted border border-border" />
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["badge".into()],
        customization_points: vec![
            cp("features", "Array of feature objects", "[]", CustomizationType::LongText),
        ],
    }
}
