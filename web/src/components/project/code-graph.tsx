"use client";

import { useState, useCallback, useMemo } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  type Node,
  type Edge,
  Handle,
  Position,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  useQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import {
  GitBranch,
  RefreshCw,
  Loader2,
  Search,
  AlertCircle,
} from "lucide-react";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/empty-state";
import { cn } from "@/lib/utils";

// -- Type colors by node type --------------------------------------------------

const TYPE_COLORS: Record<string, string> = {
  component: "#06b6d4", // cyan
  function: "#a855f7",  // purple
  module: "#3b82f6",    // blue
  type: "#f59e0b",      // amber
  hook: "#ec4899",      // pink
  class: "#6366f1",     // indigo
  interface: "#14b8a6", // teal
  enum: "#22c55e",      // green
  constant: "#64748b",  // slate
};

function colorForType(type: string | undefined | null): string {
  return TYPE_COLORS[(type ?? "").toLowerCase()] ?? "#64748b";
}

// -- Custom node ---------------------------------------------------------------

function CodeGraphNode({ data }: NodeProps) {
  const color = data.color as string;
  const highlighted = data.highlighted as boolean;
  return (
    <div
      className={cn(
        "px-4 py-2.5 rounded-xl border bg-white/[0.04] backdrop-blur-xl min-w-[160px] transition-all",
        highlighted
          ? "border-cyan-400/60 shadow-[0_0_12px_rgba(6,182,212,0.3)]"
          : "border-white/10",
      )}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!bg-white/20 !border-white/30 !w-2.5 !h-2.5"
      />
      <div className="flex items-center gap-2">
        <div
          className="w-2.5 h-2.5 rounded-full flex-shrink-0"
          style={{ backgroundColor: color }}
        />
        <div className="min-w-0">
          <div className="text-sm font-medium text-slate-200 truncate">
            {data.label as string}
          </div>
          <div className="text-[11px] text-slate-400 capitalize">
            {data.nodeType as string}
          </div>
        </div>
      </div>
      {data.file ? (
        <div className="mt-1.5 text-[10px] text-slate-500 truncate max-w-[200px]">
          {String(data.file)}
        </div>
      ) : null}
      <Handle
        type="source"
        position={Position.Bottom}
        className="!bg-white/20 !border-white/30 !w-2.5 !h-2.5"
      />
    </div>
  );
}

const nodeTypes = { codeGraphNode: CodeGraphNode };

// -- Layout helper: simple force-directed-ish grid layout ----------------------

function layoutNodes(
  rawNodes: { id: string; label: string; type: string; file?: string }[],
  searchTerm: string,
): Node[] {
  const cols = Math.max(Math.ceil(Math.sqrt(rawNodes.length)), 1);
  const xGap = 280;
  const yGap = 120;
  const lowerSearch = searchTerm.toLowerCase();

  return rawNodes.map((n, i) => ({
    id: n.id,
    type: "codeGraphNode",
    position: { x: (i % cols) * xGap, y: Math.floor(i / cols) * yGap },
    data: {
      label: n.label,
      nodeType: n.type,
      color: colorForType(n.type),
      file: n.file,
      highlighted:
        lowerSearch.length > 0 &&
        (n.label ?? "").toLowerCase().includes(lowerSearch),
    },
  }));
}

function buildEdges(
  rawEdges: { from: string; to: string; type: string }[],
): Edge[] {
  return rawEdges.map((e, i) => ({
    id: `e-${i}`,
    source: e.from,
    target: e.to,
    label: e.type,
    animated: false,
    style: { stroke: "rgba(255,255,255,0.12)" },
    labelStyle: { fill: "rgba(255,255,255,0.3)", fontSize: 10 },
    labelBgStyle: { fill: "#0a0a0f", fillOpacity: 0.8 },
  }));
}

// -- Main component ------------------------------------------------------------

export function ProjectCodeGraph({ projectId }: { projectId: string }) {
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");

  const {
    data,
    isLoading,
    isError,
    refetch,
  } = useQuery({
    queryKey: ["code-graph", projectId],
    queryFn: () => api.getCodeGraph(projectId),
  });

  const rebuildMutation = useMutation({
    mutationFn: () => api.rebuildCodeGraph(projectId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["code-graph", projectId] });
    },
  });

  // Backend returns { edges, files, stats, symbols }. Adapt symbols → nodes
  // (fall back to files when there are no symbols, e.g. non-code projects).
  const rawNodes = useMemo(() => {
    type Symbol = { name: string; kind?: string; file?: string };
    type FileEntry = { path: string; language?: string };
    type Shape = {
      nodes?: { id: string; label: string; type: string; file?: string }[];
      symbols?: Symbol[];
      files?: FileEntry[];
    };
    const shape = (data ?? {}) as Shape;
    if (shape.nodes?.length) return shape.nodes;
    if (shape.symbols?.length) {
      return shape.symbols.map((s) => ({
        id: `${s.file ?? ""}::${s.name}`,
        label: s.name,
        type: s.kind ?? "symbol",
        file: s.file,
      }));
    }
    if (shape.files?.length) {
      return shape.files.map((f) => ({
        id: `file::${f.path}`,
        label: f.path.split("/").pop() ?? f.path,
        type: f.language ?? "file",
        file: f.path,
      }));
    }
    return [];
  }, [data]);

  const nodes = useMemo(
    () => layoutNodes(rawNodes, search),
    [rawNodes, search],
  );

  const edges = useMemo(() => {
    const raw = (data as { edges?: { from: string; to: string; type: string }[] } | undefined)
      ?.edges;
    return raw ? buildEdges(raw) : [];
  }, [data]);

  const handleNodeClick = useCallback((_: React.MouseEvent, node: Node) => {
    // Could be extended to show detail panel
    void node;
  }, []);

  // -- Loading state -----------------------------------------------------------
  if (isLoading) {
    return (
      <div className="flex flex-col gap-4 p-6 h-full">
        <div className="flex items-center gap-3">
          <Skeleton className="h-10 w-40" />
          <Skeleton className="h-10 flex-1 max-w-xs" />
        </div>
        <div className="flex-1 grid grid-cols-3 gap-4">
          {Array.from({ length: 9 }).map((_, i) => (
            <Skeleton key={i} className="h-24 rounded-xl" />
          ))}
        </div>
      </div>
    );
  }

  // -- Error state -------------------------------------------------------------
  if (isError) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4">
        <AlertCircle className="w-12 h-12 text-red-400/60" />
        <p className="text-sm text-slate-400">
          Failed to load code graph.
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={() => refetch()}
        >
          <RefreshCw className="w-4 h-4 mr-2" />
          Retry
        </Button>
      </div>
    );
  }

  // -- Empty state -------------------------------------------------------------
  if (rawNodes.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-6">
        <EmptyState
          icon={GitBranch}
          title="No code graph yet"
          description="Build the code graph to visualize the relationships between your project's symbols."
          action={
            <Button
              onClick={() => rebuildMutation.mutate()}
              disabled={rebuildMutation.isPending}
            >
              {rebuildMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  Building...
                </>
              ) : (
                <>
                  <RefreshCw className="w-4 h-4 mr-2" />
                  Build Code Graph
                </>
              )}
            </Button>
          }
        />
      </div>
    );
  }

  // -- Graph view --------------------------------------------------------------
  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-3 px-4 py-3 border-b border-white/[0.06]">
        <Button
          size="sm"
          onClick={() => rebuildMutation.mutate()}
          disabled={rebuildMutation.isPending}
        >
          {rebuildMutation.isPending ? (
            <Loader2 className="w-4 h-4 mr-2 animate-spin" />
          ) : (
            <RefreshCw className="w-4 h-4 mr-2" />
          )}
          Rebuild Graph
        </Button>

        <div className="relative flex-1 max-w-xs">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400" />
          <input
            type="text"
            placeholder="Search nodes..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full h-8 pl-9 pr-3 text-sm bg-white/[0.03] border border-white/[0.08] rounded-md text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-white/20 transition-colors"
          />
        </div>

        <div className="ml-auto flex items-center gap-2 text-[11px] text-slate-400">
          <span>{nodes.length} nodes</span>
          <span className="text-white/10">|</span>
          <span>{edges.length} edges</span>
        </div>

        {/* Legend */}
        <div className="flex items-center gap-3 ml-4">
          {Object.entries(TYPE_COLORS)
            .slice(0, 5)
            .map(([type, color]) => (
              <div key={type} className="flex items-center gap-1.5">
                <div
                  className="w-2 h-2 rounded-full"
                  style={{ backgroundColor: color }}
                />
                <span className="text-[11px] text-slate-400 capitalize">
                  {type}
                </span>
              </div>
            ))}
        </div>
      </div>

      {/* ReactFlow canvas */}
      <style>{`
        .code-graph .react-flow__background { background-color: #0a0a0f !important; }
        .code-graph .react-flow__minimap { background-color: #111118 !important; border: 1px solid rgba(255,255,255,0.06) !important; border-radius: 8px !important; }
        .code-graph .react-flow__controls { border: 1px solid rgba(255,255,255,0.06) !important; border-radius: 8px !important; overflow: hidden; }
        .code-graph .react-flow__controls-button { background-color: #111118 !important; border-color: rgba(255,255,255,0.06) !important; fill: rgba(255,255,255,0.5) !important; }
        .code-graph .react-flow__controls-button:hover { background-color: #1a1a24 !important; }
        .code-graph .react-flow__edge-path { stroke: rgba(255,255,255,0.12) !important; }
      `}</style>

      <div className="flex-1 code-graph">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodeClick={handleNodeClick}
          nodeTypes={nodeTypes}
          fitView
          proOptions={{ hideAttribution: true }}
          className="!bg-[#0a0a0f]"
          minZoom={0.2}
          maxZoom={2}
        >
          <Background color="#ffffff08" gap={20} size={1} />
          <Controls position="bottom-left" />
          <MiniMap
            position="bottom-right"
            nodeColor={(n) => (n.data?.color as string) || "#666"}
            maskColor="rgba(0,0,0,0.7)"
          />
        </ReactFlow>
      </div>
    </div>
  );
}

export default ProjectCodeGraph;
