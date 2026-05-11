"use client";

import { useState, useMemo } from "react";
import { ChevronDown, ChevronRight, File, Folder, FolderOpen } from "lucide-react";
import { cn } from "@/lib/utils";

interface FileTreeNode {
  name: string;
  path: string;
  type: "file" | "directory";
  children: FileTreeNode[];
  isNew?: boolean;
}

function buildTree(paths: string[], newPaths: Set<string>): FileTreeNode[] {
  const root: FileTreeNode = {
    name: "",
    path: "",
    type: "directory",
    children: [],
  };

  for (const path of paths) {
    const parts = path.split("/").filter(Boolean);
    let current = root;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isFile = i === parts.length - 1;
      const fullPath = parts.slice(0, i + 1).join("/");

      let child = current.children.find((c) => c.name === part);
      if (!child) {
        child = {
          name: part,
          path: fullPath,
          type: isFile ? "file" : "directory",
          children: [],
          isNew: isFile ? newPaths.has(path) : false,
        };
        current.children.push(child);
      }
      if (!isFile) {
        current = child;
      }
    }
  }

  // Sort: directories first, then alphabetical
  function sortNodes(nodes: FileTreeNode[]) {
    nodes.sort((a, b) => {
      if (a.type !== b.type) return a.type === "directory" ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const node of nodes) {
      if (node.children.length > 0) sortNodes(node.children);
    }
  }
  sortNodes(root.children);

  return root.children;
}

function fileExtensionColor(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "tsx":
    case "jsx":
      return "text-sky-400";
    case "ts":
    case "js":
      return "text-amber-400";
    case "css":
    case "scss":
      return "text-pink-400";
    case "html":
      return "text-orange-400";
    case "json":
      return "text-emerald-400";
    case "md":
      return "text-violet-400";
    case "rs":
      return "text-orange-500";
    case "toml":
    case "yaml":
    case "yml":
      return "text-slate-400";
    default:
      return "text-slate-400";
  }
}

interface TreeNodeProps {
  node: FileTreeNode;
  depth: number;
  defaultOpen?: boolean;
}

function TreeNode({ node, depth, defaultOpen = true }: TreeNodeProps) {
  const [open, setOpen] = useState(defaultOpen && depth < 2);

  if (node.type === "file") {
    return (
      <div
        className={cn(
          "flex items-center gap-1.5 py-0.5 px-2 rounded-sm text-[12px] hover:bg-white/[0.04] transition-colors",
          node.isNew && "bg-emerald-500/[0.06]"
        )}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        <File className={cn("h-3.5 w-3.5 shrink-0", fileExtensionColor(node.name))} />
        <span className={cn("truncate", node.isNew ? "text-emerald-400 font-medium" : "text-slate-200/80")}>
          {node.name}
        </span>
        {node.isNew && (
          <span className="ml-auto text-[9px] text-emerald-500 font-medium uppercase tracking-wider shrink-0">
            new
          </span>
        )}
      </div>
    );
  }

  return (
    <div>
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center gap-1.5 py-0.5 px-2 rounded-sm text-[12px] hover:bg-white/[0.04] transition-colors"
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        {open ? (
          <ChevronDown className="h-3 w-3 text-slate-400/50 shrink-0" />
        ) : (
          <ChevronRight className="h-3 w-3 text-slate-400/50 shrink-0" />
        )}
        {open ? (
          <FolderOpen className="h-3.5 w-3.5 text-amber-500/70 shrink-0" />
        ) : (
          <Folder className="h-3.5 w-3.5 text-amber-500/50 shrink-0" />
        )}
        <span className="text-slate-200/70 font-medium truncate">{node.name}</span>
        <span className="ml-auto text-[10px] text-slate-400/40 tabular-nums shrink-0">
          {node.children.length}
        </span>
      </button>
      {open && (
        <div>
          {node.children.map((child) => (
            <TreeNode key={child.path} node={child} depth={depth + 1} defaultOpen={depth < 1} />
          ))}
        </div>
      )}
    </div>
  );
}

interface Props {
  files: string[];
  /** Files added in the last batch (highlighted as new) */
  recentFiles?: string[];
  className?: string;
}

export function FileTree({ files, recentFiles, className }: Props) {
  const newPaths = useMemo(() => new Set(recentFiles ?? []), [recentFiles]);
  const tree = useMemo(() => buildTree(files, newPaths), [files, newPaths]);

  if (files.length === 0) {
    return (
      <div className={cn("flex items-center justify-center py-8 text-slate-400 text-sm", className)}>
        Files will appear here as they are generated
      </div>
    );
  }

  return (
    <div className={cn("font-mono", className)}>
      {tree.map((node) => (
        <TreeNode key={node.path} node={node} depth={0} />
      ))}
      <div className="mt-2 px-2 text-[10px] text-slate-400/40">
        {files.length} file{files.length !== 1 ? "s" : ""}
      </div>
    </div>
  );
}
