//! Data table block variants.

use crate::design_system::{BlockCategory, CustomizationPoint, CustomizationType, DesignBlock};

pub fn blocks() -> Vec<DesignBlock> {
    vec![table_sortable(), table_with_actions()]
}

fn cp(name: &str, desc: &str, default: &str, vtype: CustomizationType) -> CustomizationPoint {
    CustomizationPoint {
        name: name.into(),
        description: desc.into(),
        default_value: default.into(),
        value_type: vtype,
    }
}

fn table_sortable() -> DesignBlock {
    DesignBlock {
        id: "table-sortable".into(),
        category: BlockCategory::Table,
        variant: "table-sortable".into(),
        component_code: r#""use client";

import { useState } from "react";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Search, ArrowUpDown } from "lucide-react";

interface DataRow {
  id: string;
  name: string;
  email: string;
  status: "active" | "inactive" | "pending";
  role: string;
  joined: string;
}

const sampleData: DataRow[] = [
  { id: "1", name: "Alice Johnson", email: "alice@example.com", status: "active", role: "Admin", joined: "2024-01-15" },
  { id: "2", name: "Bob Smith", email: "bob@example.com", status: "active", role: "Editor", joined: "2024-02-20" },
  { id: "3", name: "Carol Davis", email: "carol@example.com", status: "pending", role: "Viewer", joined: "2024-03-10" },
  { id: "4", name: "David Lee", email: "david@example.com", status: "inactive", role: "Editor", joined: "2024-01-28" },
  { id: "5", name: "Eve Martinez", email: "eve@example.com", status: "active", role: "Admin", joined: "2024-04-05" },
];

const statusColors = {
  active: "bg-green-500/10 text-green-600 border-green-500/20",
  inactive: "bg-red-500/10 text-red-600 border-red-500/20",
  pending: "bg-yellow-500/10 text-yellow-600 border-yellow-500/20",
};

export function TableSortable() {
  const [search, setSearch] = useState("");
  const [sortField, setSortField] = useState<keyof DataRow>("name");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");

  const filtered = sampleData
    .filter((row) =>
      row.name.toLowerCase().includes(search.toLowerCase()) ||
      row.email.toLowerCase().includes(search.toLowerCase())
    )
    .sort((a, b) => {
      const aVal = a[sortField];
      const bVal = b[sortField];
      const cmp = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
      return sortDir === "asc" ? cmp : -cmp;
    });

  const toggleSort = (field: keyof DataRow) => {
    if (sortField === field) {
      setSortDir(sortDir === "asc" ? "desc" : "asc");
    } else {
      setSortField(field);
      setSortDir("asc");
    }
  };

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <h3 className="font-semibold">Users</h3>
        <div className="relative w-64">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            placeholder="Search users..."
            className="pl-9 h-9"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </CardHeader>
      <CardContent className="p-0">
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="border-y bg-muted/50">
                {(["name", "email", "status", "role", "joined"] as const).map((col) => (
                  <th key={col} className="text-left p-3">
                    <button
                      className="flex items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-muted-foreground hover:text-foreground"
                      onClick={() => toggleSort(col)}
                    >
                      {col}
                      <ArrowUpDown className="w-3 h-3" />
                    </button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {filtered.map((row) => (
                <tr key={row.id} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                  <td className="p-3 text-sm font-medium">{row.name}</td>
                  <td className="p-3 text-sm text-muted-foreground">{row.email}</td>
                  <td className="p-3">
                    <Badge variant="outline" className={`text-xs ${statusColors[row.status]}`}>
                      {row.status}
                    </Badge>
                  </td>
                  <td className="p-3 text-sm">{row.role}</td>
                  <td className="p-3 text-sm text-muted-foreground">{row.joined}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </CardContent>
    </Card>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["card".into(), "input".into(), "badge".into()],
        customization_points: vec![
            cp("columns", "Table column definitions", "[]", CustomizationType::LongText),
            cp("title", "Table header title", "Users", CustomizationType::Text),
        ],
    }
}

fn table_with_actions() -> DesignBlock {
    DesignBlock {
        id: "table-with-actions".into(),
        category: BlockCategory::Table,
        variant: "table-with-actions".into(),
        component_code: r#""use client";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { MoreHorizontal, Plus, Download, Filter } from "lucide-react";

const data = [
  { id: "INV-001", customer: "Acme Corp", amount: "$1,250.00", status: "paid", date: "2024-03-15" },
  { id: "INV-002", customer: "Globex Inc", amount: "$3,400.00", status: "pending", date: "2024-03-14" },
  { id: "INV-003", customer: "Initech", amount: "$890.00", status: "overdue", date: "2024-03-10" },
  { id: "INV-004", customer: "Umbrella Corp", amount: "$5,200.00", status: "paid", date: "2024-03-08" },
  { id: "INV-005", customer: "Stark Industries", amount: "$2,100.00", status: "pending", date: "2024-03-05" },
];

const statusStyles: Record<string, string> = {
  paid: "bg-green-500/10 text-green-600",
  pending: "bg-yellow-500/10 text-yellow-600",
  overdue: "bg-red-500/10 text-red-600",
};

export function TableWithActions() {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <div>
          <h3 className="font-semibold">Invoices</h3>
          <p className="text-sm text-muted-foreground">Manage and track your invoices.</p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm">
            <Filter className="w-4 h-4 mr-2" />
            Filter
          </Button>
          <Button variant="outline" size="sm">
            <Download className="w-4 h-4 mr-2" />
            Export
          </Button>
          <Button size="sm">
            <Plus className="w-4 h-4 mr-2" />
            New Invoice
          </Button>
        </div>
      </CardHeader>
      <CardContent className="p-0">
        <table className="w-full">
          <thead>
            <tr className="border-y bg-muted/50">
              <th className="text-left p-3 text-xs font-medium uppercase tracking-wider text-muted-foreground">Invoice</th>
              <th className="text-left p-3 text-xs font-medium uppercase tracking-wider text-muted-foreground">Customer</th>
              <th className="text-left p-3 text-xs font-medium uppercase tracking-wider text-muted-foreground">Amount</th>
              <th className="text-left p-3 text-xs font-medium uppercase tracking-wider text-muted-foreground">Status</th>
              <th className="text-left p-3 text-xs font-medium uppercase tracking-wider text-muted-foreground">Date</th>
              <th className="w-12"></th>
            </tr>
          </thead>
          <tbody>
            {data.map((row) => (
              <tr key={row.id} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                <td className="p-3 text-sm font-medium">{row.id}</td>
                <td className="p-3 text-sm">{row.customer}</td>
                <td className="p-3 text-sm font-medium">{row.amount}</td>
                <td className="p-3">
                  <Badge variant="secondary" className={`text-xs ${statusStyles[row.status]}`}>
                    {row.status}
                  </Badge>
                </td>
                <td className="p-3 text-sm text-muted-foreground">{row.date}</td>
                <td className="p-3">
                  <Button variant="ghost" size="icon" className="h-8 w-8">
                    <MoreHorizontal className="w-4 h-4" />
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </CardContent>
    </Card>
  );
}
"#
        .into(),
        required_packages: vec!["lucide-react".into()],
        required_components: vec!["button".into(), "card".into(), "badge".into()],
        customization_points: vec![
            cp("title", "Table title", "Invoices", CustomizationType::Text),
            cp("columns", "Column definitions", "[]", CustomizationType::LongText),
            cp("showActions", "Show action buttons in header", "true", CustomizationType::Boolean),
        ],
    }
}
