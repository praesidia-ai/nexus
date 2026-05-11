//! Authentication page block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![login_centered(), register_split()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn login_centered() -> DesignBlock {
    DesignBlock {
        id: "auth-login-centered".into(),
        category: BlockCategory::Auth,
        variant: "auth-login-centered".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";

export function AuthLoginCentered() {
  return (
    <section className="min-h-screen flex items-center justify-center px-6 py-12">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <h1 className="text-2xl font-bold tracking-tight">Welcome back</h1>
          <p className="text-sm text-muted-foreground mt-1">
            Sign in to your account to continue
          </p>
        </CardHeader>
        <CardContent className="space-y-4">
          <Button variant="outline" className="w-full">
            <svg className="w-4 h-4 mr-2" viewBox="0 0 24 24">
              <path fill="currentColor" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 01-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"/>
              <path fill="currentColor" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
              <path fill="currentColor" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
              <path fill="currentColor" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
            </svg>
            Continue with Google
          </Button>

          <div className="relative">
            <Separator />
            <span className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 bg-card px-3 text-xs text-muted-foreground">
              or
            </span>
          </div>

          <div className="space-y-2">
            <Label htmlFor="email">Email</Label>
            <Input id="email" type="email" placeholder="name@example.com" />
          </div>
          <div className="space-y-2">
            <div className="flex justify-between">
              <Label htmlFor="password">Password</Label>
              <a href="/forgot-password" className="text-xs text-primary hover:underline">
                Forgot password?
              </a>
            </div>
            <Input id="password" type="password" placeholder="Enter your password" />
          </div>
          <Button className="w-full">Sign In</Button>
        </CardContent>
        <CardFooter className="justify-center">
          <p className="text-sm text-muted-foreground">
            Don&apos;t have an account?{" "}
            <a href="/register" className="text-primary font-medium hover:underline">
              Sign up
            </a>
          </p>
        </CardFooter>
      </Card>
    </section>
  );
}
"#
        .into(),
        required_packages: vec![],
        required_components: vec![
            "button".into(), "card".into(), "input".into(), "label".into(), "separator".into(),
        ],
        customization_points: vec![
            cp("title", "Login page heading", "Welcome back", CustomizationType::Text),
            cp("showSocialLogin", "Show Google/GitHub OAuth buttons", "true", CustomizationType::Boolean),
        ],
    }
}

fn register_split() -> DesignBlock {
    DesignBlock {
        id: "auth-register-split".into(),
        category: BlockCategory::Auth,
        variant: "auth-register-split".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { CheckCircle2 } from "lucide-react";

const benefits = [
  "14-day free trial, no credit card required",
  "Access to all Pro features",
  "Unlimited team members",
  "Cancel anytime",
];

export function AuthRegisterSplit() {
  return (
    <section className="min-h-screen grid lg:grid-cols-2">
      {/* Left: benefits */}
      <div className="hidden lg:flex flex-col justify-center px-12 py-16 bg-primary/5">
        <h2 className="text-3xl font-bold tracking-tight mb-4">
          Start building today
        </h2>
        <p className="text-muted-foreground mb-8 max-w-md leading-relaxed">
          Join over 10,000 teams who trust our platform to ship faster and build better products.
        </p>
        <ul className="space-y-4">
          {benefits.map((benefit) => (
            <li key={benefit} className="flex items-center gap-3">
              <CheckCircle2 className="w-5 h-5 text-primary shrink-0" />
              <span className="text-sm">{benefit}</span>
            </li>
          ))}
        </ul>
      </div>

      {/* Right: form */}
      <div className="flex items-center justify-center px-6 py-12">
        <div className="w-full max-w-md space-y-6">
          <div>
            <h1 className="text-2xl font-bold tracking-tight">Create your account</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Already have an account?{" "}
              <a href="/login" className="text-primary font-medium hover:underline">Sign in</a>
            </p>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="firstName">First name</Label>
              <Input id="firstName" placeholder="Jane" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="lastName">Last name</Label>
              <Input id="lastName" placeholder="Doe" />
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="email">Work email</Label>
            <Input id="email" type="email" placeholder="jane@company.com" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="password">Password</Label>
            <Input id="password" type="password" placeholder="8+ characters" />
          </div>
          <Button className="w-full" size="lg">
            Create Account
          </Button>
          <p className="text-xs text-muted-foreground text-center">
            By signing up, you agree to our{" "}
            <a href="/terms" className="underline hover:text-foreground">Terms</a> and{" "}
            <a href="/privacy" className="underline hover:text-foreground">Privacy Policy</a>.
          </p>
        </div>
      </div>
    </section>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "input".into(), "label".into()],
        customization_points: vec![
            cp("title", "Registration heading", "Create your account", CustomizationType::Text),
            cp("benefits", "Left-side benefit list", "[]", CustomizationType::LongText),
        ],
    }
}
