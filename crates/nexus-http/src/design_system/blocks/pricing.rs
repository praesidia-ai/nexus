//! Pricing section block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![three_tier(), comparison_table()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn three_tier() -> DesignBlock {
    DesignBlock {
        id: "pricing-three-tier".into(),
        category: BlockCategory::Pricing,
        variant: "pricing-three-tier".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Check } from "lucide-react";

const plans = [
  { name: "Starter", price: "$9", period: "/month", description: "Perfect for individuals.", features: ["5 projects", "10GB storage", "Basic analytics", "Email support"], cta: "Start Free Trial", popular: false },
  { name: "Pro", price: "$29", period: "/month", description: "For growing teams.", features: ["Unlimited projects", "100GB storage", "Advanced analytics", "Priority support", "Custom domains", "API access"], cta: "Start Free Trial", popular: true },
  { name: "Enterprise", price: "$99", period: "/month", description: "For organizations.", features: ["Everything in Pro", "Unlimited storage", "SSO & SAML", "Dedicated support", "SLA guarantee", "Custom integrations"], cta: "Contact Sales", popular: false },
];

export function PricingThreeTier() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-6xl mx-auto">
        <div className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-bold tracking-tight mb-4">Simple, transparent pricing</h2>
          <p className="text-lg text-muted-foreground max-w-2xl mx-auto">No hidden fees. Pick a plan that works for you.</p>
        </div>
        <div className="grid md:grid-cols-3 gap-8 items-start">
          {plans.map((plan) => (
            <Card key={plan.name} className={`relative ${plan.popular ? "border-primary shadow-lg scale-105" : "border-border/50"}`}>
              {plan.popular && <div className="absolute -top-3 left-1/2 -translate-x-1/2"><Badge className="px-3 py-1">Most Popular</Badge></div>}
              <CardHeader className="pb-4">
                <h3 className="text-lg font-semibold">{plan.name}</h3>
                <div className="flex items-baseline gap-1 mt-2">
                  <span className="text-4xl font-bold">{plan.price}</span>
                  <span className="text-muted-foreground">{plan.period}</span>
                </div>
                <p className="text-sm text-muted-foreground mt-2">{plan.description}</p>
              </CardHeader>
              <CardContent>
                <ul className="space-y-3">
                  {plan.features.map((feature) => (
                    <li key={feature} className="flex items-center gap-3 text-sm">
                      <Check className="w-4 h-4 text-primary shrink-0" /> {feature}
                    </li>
                  ))}
                </ul>
              </CardContent>
              <CardFooter>
                <Button className="w-full" variant={plan.popular ? "default" : "outline"}>{plan.cta}</Button>
              </CardFooter>
            </Card>
          ))}
        </div>
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "card".into(), "badge".into()],
        customization_points: vec![
            cp("sectionTitle", "Section heading", "Simple, transparent pricing", CustomizationType::Text),
        ],
    }
}

fn comparison_table() -> DesignBlock {
    DesignBlock {
        id: "pricing-comparison".into(),
        category: BlockCategory::Pricing,
        variant: "pricing-comparison".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Check, Minus } from "lucide-react";

const features = [
  { name: "Projects", free: "3", pro: "Unlimited", enterprise: "Unlimited" },
  { name: "Storage", free: "1GB", pro: "100GB", enterprise: "Unlimited" },
  { name: "Team members", free: "1", pro: "10", enterprise: "Unlimited" },
  { name: "API access", free: false, pro: true, enterprise: true },
  { name: "Custom domains", free: false, pro: true, enterprise: true },
  { name: "SSO / SAML", free: false, pro: false, enterprise: true },
  { name: "Priority support", free: false, pro: true, enterprise: true },
  { name: "SLA guarantee", free: false, pro: false, enterprise: true },
];

function CellValue({ value }: { value: string | boolean }) {
  if (typeof value === "boolean") {
    return value ? <Check className="w-5 h-5 text-primary mx-auto" /> : <Minus className="w-5 h-5 text-muted-foreground/40 mx-auto" />;
  }
  return <span className="font-medium">{value}</span>;
}

export function PricingComparison() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-4xl mx-auto">
        <div className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-bold tracking-tight mb-4">Compare plans</h2>
        </div>
        <div className="border rounded-xl overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="border-b bg-muted/50">
                <th className="text-left p-4 font-medium text-sm">Feature</th>
                <th className="text-center p-4 font-medium text-sm">Free</th>
                <th className="text-center p-4 font-medium text-sm bg-primary/5">Pro</th>
                <th className="text-center p-4 font-medium text-sm">Enterprise</th>
              </tr>
            </thead>
            <tbody>
              {features.map((f) => (
                <tr key={f.name} className="border-b last:border-0">
                  <td className="p-4 text-sm">{f.name}</td>
                  <td className="p-4 text-center text-sm"><CellValue value={f.free} /></td>
                  <td className="p-4 text-center text-sm bg-primary/5"><CellValue value={f.pro} /></td>
                  <td className="p-4 text-center text-sm"><CellValue value={f.enterprise} /></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
"#.into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into()],
        customization_points: vec![
            cp("features", "Comparison feature rows", "[]", CustomizationType::LongText),
        ],
    }
}
