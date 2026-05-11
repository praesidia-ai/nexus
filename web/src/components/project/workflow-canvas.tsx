"use client";

import { useState, useCallback, useRef, useEffect, type DragEvent } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  addEdge,
  useNodesState,
  useEdgesState,
  type Connection,
  type Node,
  type Edge,
  type ReactFlowInstance,
  Handle,
  Position,
  type NodeProps,
  MarkerType,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  Play, Loader2, GripVertical, Zap, ShieldCheck, Rocket,
  Brain, Wrench, User, GitBranch, Layers, Download, Upload,
  CheckCircle, Circle, AlertCircle, Save,
} from "lucide-react";
import { api, type AgentDefinition } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/toast";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

const PALETTE_COLORS = [
  "#3b82f6", "#06b6d4", "#22c55e", "#ec4899", "#ef4444",
  "#a855f7", "#f97316", "#eab308", "#6366f1", "#14b8a6",
];

interface AgentDef { name: string; role: string; color: string; }

function toAgentDef(agent: AgentDefinition, index: number): AgentDef {
  return { name: agent.name, role: agent.role, color: PALETTE_COLORS[index % PALETTE_COLORS.length] };
}

type NodeStatus = "idle" | "running" | "completed" | "failed";

function StatusDot({ status }: { status: NodeStatus }) {
  if (status === "running") return <Loader2 className="w-3 h-3 animate-spin text-yellow-400" />;
  if (status === "completed") return <CheckCircle className="w-3 h-3 text-green-400" />;
  if (status === "failed") return <AlertCircle className="w-3 h-3 text-red-400" />;
  return <Circle className="w-3 h-3 text-white/20" />;
}

// ---------------------------------------------------------------------------
// Custom node types
// ---------------------------------------------------------------------------

function BaseNode({ data, icon: Icon, accent }: NodeProps & { icon: React.ElementType; accent: string }) {
  return (
    <div
      className="px-4 py-3 rounded-xl border backdrop-blur-xl min-w-[200px] max-w-[260px]"
      style={{ borderColor: accent + "40", backgroundColor: accent + "08" }}
    >
      <Handle type="target" position={Position.Top} className="!bg-white/20 !border-white/30 !w-3 !h-3" />
      <div className="flex items-center gap-2 mb-2">
        <div className="w-7 h-7 rounded-lg flex items-center justify-center" style={{ backgroundColor: accent + "30" }}>
          <Icon className="w-4 h-4" style={{ color: accent }} />
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-sm font-semibold text-slate-200 truncate">{data.label as string}</div>
          <div className="text-[11px] text-slate-400">{data.subLabel as string}</div>
        </div>
        <StatusDot status={(data.status as NodeStatus) ?? "idle"} />
      </div>
      {(data.prompt !== undefined) && (
        <textarea
          className="w-full text-xs bg-white/[0.03] border border-white/[0.08] rounded p-2 text-slate-400 resize-none focus:outline-none focus:border-white/20"
          rows={2}
          placeholder="Prompt / instructions..."
          defaultValue={(data.prompt as string) || ""}
          onChange={(e) => { (data.onPromptChange as ((v: string) => void) | undefined)?.(e.target.value); }}
        />
      )}
      <Handle type="source" position={Position.Bottom} className="!bg-white/20 !border-white/30 !w-3 !h-3" />
    </div>
  );
}

function AgentNode(props: NodeProps) {
  return <BaseNode {...props} icon={Brain} accent={(props.data.color as string) || "#3b82f6"} />;
}

function ToolNode(props: NodeProps) {
  return <BaseNode {...props} icon={Wrench} accent="#f97316" />;
}

function ApprovalNode(props: NodeProps) {
  return (
    <div className="px-4 py-3 rounded-xl border border-yellow-500/30 bg-yellow-500/[0.06] backdrop-blur-xl min-w-[200px]">
      <Handle type="target" position={Position.Top} className="!bg-white/20 !border-white/30 !w-3 !h-3" />
      <div className="flex items-center gap-2">
        <div className="w-7 h-7 rounded-lg bg-yellow-500/20 flex items-center justify-center">
          <User className="w-4 h-4 text-yellow-400" />
        </div>
        <div>
          <div className="text-sm font-semibold text-slate-200">{props.data.label as string}</div>
          <div className="text-[11px] text-yellow-400/70">Human Approval</div>
        </div>
        <StatusDot status={(props.data.status as NodeStatus) ?? "idle"} />
      </div>
      <Handle type="source" position={Position.Bottom} className="!bg-white/20 !border-white/30 !w-3 !h-3" />
    </div>
  );
}

function BranchNode(props: NodeProps) {
  return (
    <div className="px-4 py-3 rounded-xl border border-purple-500/30 bg-purple-500/[0.06] backdrop-blur-xl min-w-[180px]">
      <Handle type="target" position={Position.Top} className="!bg-white/20 !border-white/30 !w-3 !h-3" />
      <div className="flex items-center gap-2">
        <div className="w-7 h-7 rounded-lg bg-purple-500/20 flex items-center justify-center">
          <GitBranch className="w-4 h-4 text-purple-400" />
        </div>
        <div>
          <div className="text-sm font-semibold text-slate-200">{props.data.label as string}</div>
          <div className="text-[11px] text-purple-400/70">Conditional Branch</div>
        </div>
      </div>
      <Handle type="source" id="true" position={Position.Bottom} style={{ left: "30%" }} className="!bg-green-400/60 !border-white/30 !w-3 !h-3" />
      <Handle type="source" id="false" position={Position.Bottom} style={{ left: "70%" }} className="!bg-red-400/60 !border-white/30 !w-3 !h-3" />
    </div>
  );
}

function ParallelNode(props: NodeProps) {
  return (
    <div className="px-4 py-3 rounded-xl border border-cyan-500/30 bg-cyan-500/[0.06] backdrop-blur-xl min-w-[180px]">
      <Handle type="target" position={Position.Top} className="!bg-white/20 !border-white/30 !w-3 !h-3" />
      <div className="flex items-center gap-2">
        <div className="w-7 h-7 rounded-lg bg-cyan-500/20 flex items-center justify-center">
          <Layers className="w-4 h-4 text-cyan-400" />
        </div>
        <div>
          <div className="text-sm font-semibold text-slate-200">{props.data.label as string}</div>
          <div className="text-[11px] text-cyan-400/70">Parallel Split</div>
        </div>
      </div>
      <Handle type="source" position={Position.Bottom} className="!bg-white/20 !border-white/30 !w-3 !h-3" />
    </div>
  );
}

const nodeTypes = {
  agentNode: AgentNode,
  toolNode: ToolNode,
  approvalNode: ApprovalNode,
  branchNode: BranchNode,
  parallelNode: ParallelNode,
};

// ---------------------------------------------------------------------------
// Node palette items
// ---------------------------------------------------------------------------

const SPECIAL_NODES = [
  { type: "toolNode", label: "Tool Call", subLabel: "Execute a tool", color: "#f97316", icon: Wrench },
  { type: "approvalNode", label: "Human Approval", subLabel: "Pause for review", color: "#eab308", icon: User },
  { type: "branchNode", label: "Condition", subLabel: "Branch on result", color: "#a855f7", icon: GitBranch },
  { type: "parallelNode", label: "Parallel", subLabel: "Split & rejoin", color: "#06b6d4", icon: Layers },
];

// ---------------------------------------------------------------------------
// Pipeline templates
// ---------------------------------------------------------------------------

function makePipeline(steps: { prompt: string }[], agentDefs: AgentDef[]): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = steps.map((s, i) => {
    const agent = agentDefs[i % Math.max(agentDefs.length, 1)];
    if (!agent) return null;
    return {
      id: `pipeline-${i}`, type: "agentNode", position: { x: 300, y: 80 + i * 160 },
      data: { label: agent.name, subLabel: agent.role, color: agent.color, prompt: s.prompt, status: "idle" },
    };
  }).filter(Boolean) as Node[];
  const edges: Edge[] = nodes.slice(1).map((_, i) => ({
    id: `pe-${i}`, source: `pipeline-${i}`, target: `pipeline-${i + 1}`,
    animated: true, style: { stroke: "#ffffff20" },
    markerEnd: { type: MarkerType.ArrowClosed, color: "#ffffff20" },
  }));
  return { nodes, edges };
}

const PIPELINE_TEMPLATES = [
  {
    label: "App Build", icon: Rocket, color: "text-glow-cyan",
    steps: [
      { prompt: "Research requirements and prior art" },
      { prompt: "Create detailed product spec" },
      { prompt: "Design architecture and infrastructure" },
      { prompt: "Implement the application code" },
      { prompt: "Run security audit" },
      { prompt: "Deploy to production" },
    ],
  },
  {
    label: "Code Review", icon: Zap, color: "text-blue-400",
    steps: [
      { prompt: "Analyze code changes" },
      { prompt: "Check security vulnerabilities" },
      { prompt: "Validate performance" },
      { prompt: "Summarize findings" },
    ],
  },
  {
    label: "Security Audit", icon: ShieldCheck, color: "text-red-400",
    steps: [
      { prompt: "Scan OWASP Top 10" },
      { prompt: "Audit IAM and network policies" },
      { prompt: "Check data encryption" },
      { prompt: "Compile audit report" },
    ],
  },
];

// ---------------------------------------------------------------------------
// Export/Import helpers
// ---------------------------------------------------------------------------

function exportWorkflow(nodes: Node[], edges: Edge[], name: string) {
  const workflow = {
    name,
    version: "1.0",
    nodes: nodes.map((n) => ({
      id: n.id, type: n.type, x: n.position.x, y: n.position.y,
      label: n.data.label, subLabel: n.data.subLabel, prompt: n.data.prompt,
    })),
    edges: edges.map((e) => ({ id: e.id, source: e.source, target: e.target })),
  };
  const blob = new Blob([JSON.stringify(workflow, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${name.toLowerCase().replace(/\s+/g, "-")}.workflow.json`;
  a.click();
  URL.revokeObjectURL(url);
}

let nodeIdCounter = 0;

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function WorkflowCanvas({ projectId }: { projectId: string }) {
  const { toast } = useToast();
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);
  const [workflowName, setWorkflowName] = useState("Untitled Workflow");
  const [running, setRunning] = useState(false);
  const [saved, setSaved] = useState(false);
  const [reactFlowInstance, setReactFlowInstance] = useState<ReactFlowInstance | null>(null);
  const [agentDefs, setAgentDefs] = useState<AgentDef[]>([]);
  const agentMapRef = useRef<Record<string, AgentDef>>({});

  useEffect(() => {
    api.listAgents(projectId).then((agents) => {
      const defs = agents.map(toAgentDef);
      setAgentDefs(defs);
      agentMapRef.current = Object.fromEntries(defs.map((a) => [a.name, a]));
    }).catch(() => { toast("error", "Failed to load agents"); });
  }, [projectId, toast]);

  const onConnect = useCallback(
    (connection: Connection) => setEdges((eds) => addEdge({
      ...connection, animated: true,
      style: { stroke: "#ffffff20" },
      markerEnd: { type: MarkerType.ArrowClosed, color: "#ffffff20" },
    }, eds)),
    [setEdges]
  );

  const onNodeClick = useCallback((_: React.MouseEvent, node: Node) => setSelectedNode(node), []);
  const onPaneClick = useCallback(() => setSelectedNode(null), []);
  const onDragOver = useCallback((event: DragEvent) => { event.preventDefault(); event.dataTransfer.dropEffect = "move"; }, []);

  const makePromptCallback = useCallback((nodeId: string) => (val: string) => {
    setNodes((nds) => nds.map((n) => n.id === nodeId ? { ...n, data: { ...n.data, prompt: val } } : n));
  }, [setNodes]);

  const onDrop = useCallback((event: DragEvent) => {
    event.preventDefault();
    const data = event.dataTransfer.getData("application/nexus-node");
    if (!data) return;
    const parsed = JSON.parse(data);
    const bounds = reactFlowWrapper.current?.getBoundingClientRect();
    if (!bounds || !reactFlowInstance?.screenToFlowPosition) return;
    const position = reactFlowInstance.screenToFlowPosition({ x: event.clientX, y: event.clientY });
    const id = `node-${++nodeIdCounter}-${Date.now()}`;
    const newNode: Node = {
      id, type: parsed.nodeType || "agentNode", position,
      data: {
        label: parsed.label, subLabel: parsed.subLabel || parsed.role || "",
        color: parsed.color || "#3b82f6",
        prompt: "",
        status: "idle" as NodeStatus,
        onPromptChange: makePromptCallback(id),
      },
    };
    setNodes((nds) => [...nds, newNode]);
  }, [reactFlowInstance, setNodes, makePromptCallback]);

  const loadPipeline = useCallback((steps: { prompt: string }[]) => {
    const pipeline = makePipeline(steps, agentDefs);
    const withCallbacks = pipeline.nodes.map((n) => ({
      ...n, data: { ...n.data, status: "idle" as NodeStatus, onPromptChange: makePromptCallback(n.id) },
    }));
    setNodes(withCallbacks); setEdges(pipeline.edges); setSelectedNode(null);
    toast("info", "Pipeline loaded");
  }, [agentDefs, setNodes, setEdges, setSelectedNode, makePromptCallback, toast]);

  const handleRun = useCallback(async () => {
    if (nodes.length === 0) return;
    setRunning(true);
    // Mark all nodes as idle first
    setNodes((nds) => nds.map((n) => ({ ...n, data: { ...n.data, status: "idle" as NodeStatus } })));
    try {
      // Simulate sequential execution with status updates
      for (const node of nodes) {
        setNodes((nds) => nds.map((n) => n.id === node.id ? { ...n, data: { ...n.data, status: "running" as NodeStatus } } : n));
        await new Promise((r) => setTimeout(r, 600));
        setNodes((nds) => nds.map((n) => n.id === node.id ? { ...n, data: { ...n.data, status: "completed" as NodeStatus } } : n));
      }
      const steps = nodes.map((n) => ({ agent: n.data.label as string, role: n.data.subLabel as string, prompt: (n.data.prompt as string) || "" }));
      await api.runWorkflow(projectId, "custom", { steps });
      toast("success", "Workflow completed");
    } catch {
      toast("error", "Workflow failed");
      setNodes((nds) => nds.map((n) => n.data.status === "running" ? { ...n, data: { ...n.data, status: "failed" as NodeStatus } } : n));
    } finally {
      setRunning(false);
    }
  }, [nodes, projectId, setNodes, toast]);

  const handleExport = useCallback(() => {
    exportWorkflow(nodes, edges, workflowName);
  }, [nodes, edges, workflowName]);

  const handleImport = useCallback((event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (e) => {
      try {
        const workflow = JSON.parse(e.target?.result as string);
        const importedNodes: Node[] = workflow.nodes.map((n: { id: string; type: string; x: number; y: number; label: string; subLabel: string; prompt: string }) => ({
          id: n.id, type: n.type || "agentNode",
          position: { x: n.x, y: n.y },
          data: { label: n.label, subLabel: n.subLabel, prompt: n.prompt, status: "idle", onPromptChange: makePromptCallback(n.id) },
        }));
        const importedEdges: Edge[] = workflow.edges.map((e: { id: string; source: string; target: string }) => ({
          ...e, animated: true, style: { stroke: "#ffffff20" },
          markerEnd: { type: MarkerType.ArrowClosed, color: "#ffffff20" },
        }));
        setNodes(importedNodes);
        setEdges(importedEdges);
        setWorkflowName(workflow.name || "Imported Workflow");
        toast("success", "Workflow imported");
      } catch {
        toast("error", "Invalid workflow file");
      }
    };
    reader.readAsText(file);
    event.target.value = "";
  }, [setNodes, setEdges, makePromptCallback, toast]);

  const handleSave = useCallback(() => {
    const key = `nexus:workflow:${projectId}:${workflowName}`;
    const payload = {
      nodes: nodes.map((n) => ({ id: n.id, type: n.type, x: n.position.x, y: n.position.y, label: n.data.label, subLabel: n.data.subLabel, prompt: n.data.prompt })),
      edges: edges.map((e) => ({ id: e.id, source: e.source, target: e.target })),
    };
    localStorage.setItem(key, JSON.stringify(payload));
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
    toast("success", "Workflow saved");
  }, [nodes, edges, projectId, workflowName, toast]);

  const onPaletteDragStart = useCallback((event: DragEvent<HTMLDivElement>, payload: object) => {
    event.dataTransfer.setData("application/nexus-node", JSON.stringify(payload));
    event.dataTransfer.effectAllowed = "move";
  }, []);

  return (
    <>
      <style>{`
        .react-flow__background { background-color: #0a0a0f !important; }
        .react-flow__minimap { background-color: #111118 !important; border: 1px solid rgba(255,255,255,0.06) !important; border-radius: 8px !important; }
        .react-flow__controls { border: 1px solid rgba(255,255,255,0.06) !important; border-radius: 8px !important; overflow: hidden; }
        .react-flow__controls-button { background-color: #111118 !important; border-color: rgba(255,255,255,0.06) !important; fill: rgba(255,255,255,0.5) !important; }
        .react-flow__controls-button:hover { background-color: #1a1a24 !important; }
        .react-flow__edge-path { stroke: rgba(255,255,255,0.12) !important; }
      `}</style>

      <div className="flex h-full overflow-hidden">
        {/* Left palette */}
        <div className="w-[220px] flex-shrink-0 glass border-r border-white/[0.06] flex flex-col h-full overflow-hidden">
          <div className="px-4 pt-4 pb-2">
            <h2 className="text-sm font-semibold text-slate-200">Node Palette</h2>
            <p className="text-[11px] text-slate-400 mt-0.5">Drag onto canvas</p>
          </div>

          {/* Special node types */}
          <div className="px-3 pb-2">
            <div className="text-[10px] text-slate-500 uppercase tracking-wider mb-1.5 px-1">Built-in Nodes</div>
            <div className="space-y-1">
              {SPECIAL_NODES.map((n) => (
                <div
                  key={n.type}
                  draggable
                  onDragStart={(e) => onPaletteDragStart(e, { nodeType: n.type, label: n.label, subLabel: n.subLabel, color: n.color })}
                  className="flex items-center gap-2 px-3 py-2 rounded-lg border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.05] cursor-grab active:cursor-grabbing transition-colors"
                >
                  <GripVertical className="w-3 h-3 text-slate-400/40 flex-shrink-0" />
                  <div className="w-5 h-5 rounded flex items-center justify-center" style={{ backgroundColor: n.color + "20" }}>
                    <n.icon className="w-3 h-3" style={{ color: n.color }} />
                  </div>
                  <div>
                    <div className="text-[12px] font-medium text-slate-200">{n.label}</div>
                    <div className="text-[10px] text-slate-400">{n.subLabel}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Agent nodes */}
          <div className="flex-1 overflow-y-auto scrollbar-thin px-3 pb-4">
            <div className="text-[10px] text-slate-500 uppercase tracking-wider mb-1.5 px-1 mt-2">Agents</div>
            {agentDefs.length === 0 ? (
              <p className="text-[11px] text-slate-400/50 px-1">No agents yet.</p>
            ) : (
              <div className="space-y-1">
                {agentDefs.map((agent) => (
                  <div
                    key={agent.name}
                    draggable
                    onDragStart={(e) => onPaletteDragStart(e, { nodeType: "agentNode", label: agent.name, subLabel: agent.role, color: agent.color })}
                    className="flex items-center gap-2 px-3 py-2 rounded-lg border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.05] cursor-grab active:cursor-grabbing transition-colors group"
                  >
                    <GripVertical className="w-3 h-3 text-slate-400/40 group-hover:text-slate-400/70 flex-shrink-0" />
                    <div className="w-2 h-2 rounded-full flex-shrink-0" style={{ backgroundColor: agent.color }} />
                    <div className="min-w-0 flex-1">
                      <div className="text-[12px] font-medium text-slate-200 truncate">{agent.name}</div>
                      <div className="text-[10px] text-slate-400 truncate">{agent.role}</div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Canvas */}
        <div className="flex-1 relative" ref={reactFlowWrapper}>
          <ReactFlow
            nodes={nodes} edges={edges}
            onNodesChange={onNodesChange} onEdgesChange={onEdgesChange}
            onConnect={onConnect} onNodeClick={onNodeClick} onPaneClick={onPaneClick}
            onDrop={onDrop} onDragOver={onDragOver} onInit={setReactFlowInstance}
            nodeTypes={nodeTypes} fitView proOptions={{ hideAttribution: true }}
            className="!bg-[#0a0a0f]"
          >
            <Background color="#ffffff06" gap={20} size={1} />
            <Controls position="bottom-left" />
            <MiniMap position="bottom-right" nodeColor={(n) => (n.data?.color as string) || "#666"} maskColor="rgba(0,0,0,0.7)" />
          </ReactFlow>
        </div>

        {/* Right panel */}
        <div className="w-[260px] flex-shrink-0 glass border-l border-white/[0.06] flex flex-col h-full overflow-hidden">
          <div className="px-4 pt-4 pb-4 space-y-3 flex-shrink-0">
            <Button
              onClick={handleRun}
              disabled={running || nodes.length === 0}
              className="w-full h-9 bg-gradient-to-r from-glow-cyan to-glow-blue hover:brightness-110 text-white font-semibold shadow-glow-sm transition-all"
            >
              {running ? (<><Loader2 className="w-4 h-4 mr-2 animate-spin" />Running...</>) : (<><Play className="w-4 h-4 mr-2" />Run Workflow</>)}
            </Button>

            <div className="flex gap-2">
              <button
                onClick={handleSave}
                className="flex-1 flex items-center justify-center gap-1 h-8 text-[12px] text-slate-300 border border-white/[0.08] rounded-md hover:bg-white/[0.05] transition-colors"
              >
                {saved ? <CheckCircle className="w-3.5 h-3.5 text-green-400" /> : <Save className="w-3.5 h-3.5" />}
                Save
              </button>
              <button
                onClick={handleExport}
                disabled={nodes.length === 0}
                className="flex-1 flex items-center justify-center gap-1 h-8 text-[12px] text-slate-300 border border-white/[0.08] rounded-md hover:bg-white/[0.05] transition-colors disabled:opacity-40"
              >
                <Download className="w-3.5 h-3.5" />Export
              </button>
              <button
                onClick={() => fileInputRef.current?.click()}
                className="flex-1 flex items-center justify-center gap-1 h-8 text-[12px] text-slate-300 border border-white/[0.08] rounded-md hover:bg-white/[0.05] transition-colors"
              >
                <Upload className="w-3.5 h-3.5" />Import
              </button>
              <input ref={fileInputRef} type="file" accept=".json" className="hidden" onChange={handleImport} />
            </div>

            <div>
              <label className="text-[11px] text-slate-400 uppercase tracking-wider block mb-1">Workflow Name</label>
              <input
                type="text" value={workflowName}
                onChange={(e) => setWorkflowName(e.target.value)}
                className="w-full text-sm bg-white/[0.03] border border-white/[0.08] rounded-md px-3 py-1.5 text-slate-200 focus:outline-none focus:border-white/20"
              />
            </div>

            <div className="flex gap-2">
              <Badge variant="outline" className="text-xs">{nodes.length} nodes</Badge>
              <Badge variant="outline" className="text-xs">{edges.length} edges</Badge>
            </div>
          </div>

          {/* Selected node inspector */}
          {selectedNode && (
            <div className="px-4 pb-4 border-t border-white/[0.06] pt-3 space-y-3 flex-shrink-0">
              <div className="text-[11px] text-slate-400 uppercase tracking-wider">Node Inspector</div>
              <div className="flex items-center gap-2">
                <div className="w-6 h-6 rounded-md flex items-center justify-center" style={{ backgroundColor: (selectedNode.data.color as string) || "#3b82f6" }}>
                  <Brain className="w-3.5 h-3.5 text-white" />
                </div>
                <div>
                  <div className="text-sm font-semibold text-slate-200">{selectedNode.data.label as string}</div>
                  <div className="text-[11px] text-slate-400">{selectedNode.type}</div>
                </div>
              </div>
              <div>
                <label className="text-[11px] text-slate-400 uppercase tracking-wider block mb-1">Prompt</label>
                <textarea
                  className="w-full text-xs bg-white/[0.03] border border-white/[0.08] rounded-md p-2 text-slate-400 resize-none focus:outline-none focus:border-white/20"
                  rows={5}
                  placeholder="Instructions for this node..."
                  defaultValue={(selectedNode.data.prompt as string) || ""}
                  key={selectedNode.id}
                  onChange={(e) => {
                    const val = e.target.value;
                    setNodes((nds) => nds.map((n) => n.id === selectedNode.id ? { ...n, data: { ...n.data, prompt: val } } : n));
                  }}
                />
              </div>
            </div>
          )}

          {/* Templates */}
          <div className="mt-auto px-4 pb-4 pt-3 border-t border-white/[0.06] space-y-1.5 flex-shrink-0">
            <div className="text-[11px] text-slate-400 uppercase tracking-wider mb-2">Templates</div>
            {PIPELINE_TEMPLATES.map((tmpl) => (
              <button
                key={tmpl.label}
                disabled={agentDefs.length === 0}
                onClick={() => loadPipeline(tmpl.steps)}
                className="w-full flex items-center gap-2 px-3 py-2 rounded-lg border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.05] text-[12px] text-slate-200 transition-colors text-left disabled:opacity-40 disabled:cursor-not-allowed"
              >
                <tmpl.icon className={`w-3.5 h-3.5 ${tmpl.color} flex-shrink-0`} />
                {tmpl.label}
              </button>
            ))}
          </div>
        </div>
      </div>
    </>
  );
}
