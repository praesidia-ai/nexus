//! Hero section block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![
        gradient_centered(),
        split_image(),
        video_background(),
        minimal_left_aligned(),
    ]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn gradient_centered() -> DesignBlock {
    DesignBlock {
        id: "hero-gradient-centered".into(),
        category: BlockCategory::Hero,
        variant: "hero-gradient-centered".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ArrowRight, Sparkles } from "lucide-react";

export function HeroGradientCentered() {
  return (
    <section className="relative min-h-[90vh] flex items-center justify-center overflow-hidden">
      <div className="absolute inset-0 bg-gradient-to-br from-primary/20 via-background to-secondary/20" />
      <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_top,_var(--tw-gradient-stops))] from-primary/10 via-transparent to-transparent" />
      <div className="relative z-10 max-w-4xl mx-auto px-6 text-center">
        <Badge variant="secondary" className="mb-6 px-4 py-1.5 text-sm">
          <Sparkles className="w-3.5 h-3.5 mr-1.5" />
          Now Available
        </Badge>
        <h1 className="text-5xl md:text-7xl font-bold tracking-tight mb-6 bg-gradient-to-r from-foreground to-foreground/70 bg-clip-text text-transparent">
          Build something<br />extraordinary
        </h1>
        <p className="text-lg md:text-xl text-muted-foreground max-w-2xl mx-auto mb-10 leading-relaxed">
          The modern platform for teams who want to ship faster without sacrificing quality.
        </p>
        <div className="flex flex-col sm:flex-row gap-4 justify-center">
          <Button size="lg" className="text-base px-8">
            Get Started Free <ArrowRight className="w-4 h-4 ml-2" />
          </Button>
          <Button size="lg" variant="outline" className="text-base px-8">View Demo</Button>
        </div>
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "badge".into()],
        customization_points: vec![
            cp("title", "Hero headline text", "Build something extraordinary", CustomizationType::Text),
            cp("subtitle", "Supporting description", "The modern platform...", CustomizationType::LongText),
            cp("ctaText", "Primary CTA button text", "Get Started Free", CustomizationType::Text),
            cp("badgeText", "Badge text above title", "Now Available", CustomizationType::Text),
        ],
    }
}

fn split_image() -> DesignBlock {
    DesignBlock {
        id: "hero-split-image".into(),
        category: BlockCategory::Hero,
        variant: "hero-split-image".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { ArrowRight, Play } from "lucide-react";

export function HeroSplitImage() {
  return (
    <section className="min-h-[85vh] flex items-center">
      <div className="max-w-7xl mx-auto px-6 grid lg:grid-cols-2 gap-12 items-center">
        <div>
          <p className="text-sm font-semibold text-primary uppercase tracking-wider mb-4">Welcome to the future</p>
          <h1 className="text-4xl md:text-6xl font-bold tracking-tight mb-6">Your ideas deserve a better platform</h1>
          <p className="text-lg text-muted-foreground mb-8 leading-relaxed max-w-lg">
            We provide the tools, infrastructure, and support you need to bring your vision to life.
          </p>
          <div className="flex flex-wrap gap-4">
            <Button size="lg" className="text-base px-8">Start Building <ArrowRight className="w-4 h-4 ml-2" /></Button>
            <Button size="lg" variant="ghost" className="text-base"><Play className="w-4 h-4 mr-2" /> Watch Video</Button>
          </div>
        </div>
        <div className="relative">
          <div className="aspect-[4/3] rounded-2xl bg-gradient-to-br from-primary/20 to-secondary/20 border border-border overflow-hidden flex items-center justify-center">
            <div className="text-center text-muted-foreground">
              <div className="w-16 h-16 rounded-full bg-primary/10 flex items-center justify-center mx-auto mb-4">
                <Play className="w-6 h-6 text-primary" />
              </div>
              <p className="text-sm">Product Preview</p>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into()],
        customization_points: vec![
            cp("title", "Hero headline", "Your ideas deserve a better platform", CustomizationType::Text),
            cp("imageUrl", "Hero image URL", "/hero-image.png", CustomizationType::ImageUrl),
        ],
    }
}

fn video_background() -> DesignBlock {
    DesignBlock {
        id: "hero-video-background".into(),
        category: BlockCategory::Hero,
        variant: "hero-video-background".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { ArrowRight, ChevronDown } from "lucide-react";

export function HeroVideoBackground() {
  return (
    <section className="relative min-h-screen flex items-center justify-center overflow-hidden">
      <div className="absolute inset-0 bg-gradient-to-b from-black/60 via-black/40 to-background z-10" />
      <div className="absolute inset-0 bg-[url('/hero-bg.jpg')] bg-cover bg-center" />
      <div className="relative z-20 max-w-3xl mx-auto px-6 text-center text-white">
        <h1 className="text-5xl md:text-8xl font-bold tracking-tighter mb-6">
          Redefine<br /><span className="text-primary">what&apos;s possible</span>
        </h1>
        <p className="text-lg md:text-xl text-white/80 max-w-xl mx-auto mb-10">
          Push the boundaries of creativity with tools designed for the next generation of builders.
        </p>
        <Button size="lg" className="text-base px-10 py-6 text-lg">Explore Now <ArrowRight className="w-5 h-5 ml-2" /></Button>
      </div>
      <div className="absolute bottom-8 left-1/2 -translate-x-1/2 z-20 animate-bounce">
        <ChevronDown className="w-6 h-6 text-white/60" />
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into()],
        customization_points: vec![
            cp("title", "Hero headline", "Redefine what's possible", CustomizationType::Text),
            cp("backgroundImage", "Background image URL", "/hero-bg.jpg", CustomizationType::ImageUrl),
        ],
    }
}

fn minimal_left_aligned() -> DesignBlock {
    DesignBlock {
        id: "hero-minimal-left".into(),
        category: BlockCategory::Hero,
        variant: "hero-minimal-left".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { ArrowRight } from "lucide-react";

export function HeroMinimalLeft() {
  return (
    <section className="min-h-[80vh] flex items-center">
      <div className="max-w-7xl mx-auto px-6 w-full">
        <div className="max-w-2xl">
          <h1 className="text-4xl md:text-6xl font-bold tracking-tight mb-6 leading-[1.1]">
            Simple tools for<span className="text-primary"> complex </span>problems
          </h1>
          <p className="text-lg text-muted-foreground mb-8 leading-relaxed">
            Stop wrestling with complexity. Our platform gives you the clarity and power to solve real problems, fast.
          </p>
          <div className="flex gap-4">
            <Button size="lg">Get Started <ArrowRight className="w-4 h-4 ml-2" /></Button>
            <Button size="lg" variant="link" className="text-muted-foreground">Learn more</Button>
          </div>
        </div>
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into()],
        customization_points: vec![
            cp("title", "Hero headline", "Simple tools for complex problems", CustomizationType::Text),
        ],
    }
}
