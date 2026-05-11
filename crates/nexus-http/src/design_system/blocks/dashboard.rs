//! Dashboard layout block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![dashboard_sidebar(), dashboard_top_nav()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn dashboard_sidebar() -> DesignBlock {
    DesignBlock {
        id: "dashboard-sidebar".into(),
        category: BlockCategory::Dashboard,
        variant: "dashboard-sidebar".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  LayoutDashboard, Users, Settings, BarChart3, FileText,
  Bell, Search, ChevronDown, Plus,
} from "lucide-react";
import { Input } from "@/components/ui/input";

const sidebarLinks = [
  { icon: LayoutDashboard, label: "Dashboard", href: "/dashboard", active: true },
  { icon: BarChart3, label: "Analytics", href: "/analytics", active: false },
  { icon: Users, label: "Team", href: "/team", active: false },
  { icon: FileText, label: "Documents", href: "/documents", active: false },
  { icon: Settings, label: "Settings", href: "/settings", active: false },
];

export function DashboardSidebar() {
  return (
    <div className="flex h-screen">
      {/* Sidebar */}
      <aside className="w-64 border-r border-border bg-muted/30 flex flex-col">
        <div className="p-4 border-b border-border">
          <span className="text-lg font-bold">Brand</span>
        </div>
        <nav className="flex-1 p-3 space-y-1">
          {sidebarLinks.map((link) => (
            <a
              key={link.label}
              href={link.href}
              className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
                link.active
                  ? "bg-primary/10 text-primary font-medium"
                  : "text-muted-foreground hover:text-foreground hover:bg-muted"
              }`}
            >
              <link.icon className="w-4 h-4" />
              {link.label}
            </a>
          ))}
        </nav>
      </aside>

      {/* Main content */}
      <main className="flex-1 flex flex-col">
        <header className="h-14 border-b border-border px-6 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <div className="relative w-64">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
              <Input placeholder="Search..." className="pl-9 h-9" />
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="icon" className="relative">
              <Bell className="w-4 h-4" />
              <span className="absolute top-1 right-1 w-2 h-2 bg-primary rounded-full" />
            </Button>
            <Button variant="ghost" size="sm" className="gap-2">
              <div className="w-6 h-6 rounded-full bg-primary/20" />
              John Doe
              <ChevronDown className="w-3 h-3" />
            </Button>
          </div>
        </header>

        <div className="flex-1 p-6 space-y-6 overflow-auto">
          <div className="flex justify-between items-center">
            <h1 className="text-2xl font-bold">Dashboard</h1>
            <Button>
              <Plus className="w-4 h-4 mr-2" />
              New Project
            </Button>
          </div>

          {/* Stats row */}
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            {[
              { label: "Total Revenue", value: "$45,231", change: "+20.1%" },
              { label: "Active Users", value: "2,350", change: "+10.5%" },
              { label: "Projects", value: "48", change: "+3" },
              { label: "Conversion Rate", value: "3.2%", change: "+0.4%" },
            ].map((stat) => (
              <Card key={stat.label}>
                <CardContent className="p-4">
                  <p className="text-sm text-muted-foreground">{stat.label}</p>
                  <div className="flex items-baseline gap-2 mt-1">
                    <span className="text-2xl font-bold">{stat.value}</span>
                    <span className="text-xs text-green-500 font-medium">{stat.change}</span>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>

          <Card>
            <CardHeader>
              <h3 className="font-semibold">Recent Activity</h3>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground">Activity chart placeholder</p>
            </CardContent>
          </Card>
        </div>
      </main>
    </div>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "card".into(), "input".into()],
        customization_points: vec![
            cp("brandName", "Sidebar brand name", "Brand", CustomizationType::Text),
            cp("sidebarLinks", "Sidebar navigation items", "[]", CustomizationType::LongText),
        ],
    }
}

fn dashboard_top_nav() -> DesignBlock {
    DesignBlock {
        id: "dashboard-top-nav".into(),
        category: BlockCategory::Dashboard,
        variant: "dashboard-top-nav".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  LayoutDashboard, Users, Settings, BarChart3,
  Bell, Plus, ArrowUpRight, ArrowDownRight,
} from "lucide-react";

const tabs = [
  { icon: LayoutDashboard, label: "Overview", active: true },
  { icon: BarChart3, label: "Analytics", active: false },
  { icon: Users, label: "Customers", active: false },
  { icon: Settings, label: "Settings", active: false },
];

export function DashboardTopNav() {
  return (
    <div className="min-h-screen flex flex-col">
      {/* Top navigation */}
      <header className="border-b border-border bg-background">
        <div className="max-w-7xl mx-auto px-6">
          <div className="h-16 flex items-center justify-between">
            <span className="text-lg font-bold">Brand</span>
            <div className="flex items-center gap-3">
              <Button variant="ghost" size="icon">
                <Bell className="w-4 h-4" />
              </Button>
              <div className="w-8 h-8 rounded-full bg-primary/20" />
            </div>
          </div>
          <nav className="flex gap-1 -mb-px">
            {tabs.map((tab) => (
              <button
                key={tab.label}
                className={`flex items-center gap-2 px-4 py-3 text-sm border-b-2 transition-colors ${
                  tab.active
                    ? "border-primary text-primary font-medium"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                }`}
              >
                <tab.icon className="w-4 h-4" />
                {tab.label}
              </button>
            ))}
          </nav>
        </div>
      </header>

      {/* Content */}
      <main className="flex-1 max-w-7xl mx-auto w-full px-6 py-8 space-y-6">
        <div className="flex justify-between items-center">
          <div>
            <h1 className="text-2xl font-bold">Overview</h1>
            <p className="text-sm text-muted-foreground">Welcome back. Here is a summary of your activity.</p>
          </div>
          <Button>
            <Plus className="w-4 h-4 mr-2" />
            Create
          </Button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {[
            { label: "Revenue", value: "$12,430", change: 12.5, up: true },
            { label: "Orders", value: "342", change: 8.2, up: true },
            { label: "Refunds", value: "$1,204", change: 3.1, up: false },
          ].map((stat) => (
            <Card key={stat.label}>
              <CardContent className="p-6">
                <div className="flex justify-between items-start">
                  <p className="text-sm text-muted-foreground">{stat.label}</p>
                  <Badge variant={stat.up ? "default" : "destructive"} className="text-xs">
                    {stat.up ? <ArrowUpRight className="w-3 h-3 mr-1" /> : <ArrowDownRight className="w-3 h-3 mr-1" />}
                    {stat.change}%
                  </Badge>
                </div>
                <p className="text-3xl font-bold mt-2">{stat.value}</p>
              </CardContent>
            </Card>
          ))}
        </div>
      </main>
    </div>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "card".into(), "badge".into()],
        customization_points: vec![
            cp("brandName", "Top nav brand name", "Brand", CustomizationType::Text),
            cp("tabs", "Navigation tab items", "[]", CustomizationType::LongText),
        ],
    }
}
