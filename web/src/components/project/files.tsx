"use client";

// Re-exports the existing FilesPage as a component that accepts projectId prop.
// The full implementation lives in the original page; we re-use it via dynamic import
// to avoid duplicating ~900 lines of file tree + editor logic.

import { useState, useEffect, useCallback, useRef } from "react";
import {
  FileCode,
  Folder,
  FolderOpen,
  ChevronRight,
  ChevronDown,
  File,
  Loader2,
  Save,
  Download,
  Plus,
  Trash2,
  X,
  Check,
  AlertCircle,
  RotateCw,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import { useToast } from "@/components/toast";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface FileEntry {
  path: string;
  type: string;
  name: string;
  size?: number;
  extension?: string;
}

interface TreeNode {
  name: string;
  path: string;
  type: "file" | "dir";
  extension?: string;
  size?: number;
  children: TreeNode[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const EXT_COLORS: Record<string, string> = {
  tsx: "bg-blue-500/20 text-blue-400",
  ts: "bg-blue-500/20 text-blue-400",
  jsx: "bg-blue-500/20 text-blue-400",
  js: "bg-yellow-500/20 text-yellow-400",
  json: "bg-yellow-500/20 text-yellow-400",
  css: "bg-purple-500/20 text-purple-400",
  scss: "bg-purple-500/20 text-purple-400",
  md: "bg-green-500/20 text-green-400",
  mdx: "bg-green-500/20 text-green-400",
  yaml: "bg-orange-500/20 text-orange-400",
  yml: "bg-orange-500/20 text-orange-400",
  html: "bg-red-500/20 text-red-400",
  svg: "bg-pink-500/20 text-pink-400",
};

function extBadgeClass(ext?: string): string {
  if (!ext) return "bg-white/[0.04] text-slate-400";
  return EXT_COLORS[ext] ?? "bg-white/[0.04] text-slate-400";
}

function buildTree(files: FileEntry[]): TreeNode[] {
  const root: TreeNode[] = [];
  const dirMap = new Map<string, TreeNode>();

  for (const f of files) {
    if (f.type === "dir") {
      const node: TreeNode = { name: f.name, path: f.path, type: "dir", children: [] };
      dirMap.set(f.path, node);
    }
  }

  for (const f of files) {
    const parts = f.path.split("/");
    if (f.type === "dir") {
      const node = dirMap.get(f.path)!;
      if (parts.length <= 1) { root.push(node); }
      else {
        const parentPath = parts.slice(0, -1).join("/");
        const parent = dirMap.get(parentPath);
        if (parent) parent.children.push(node);
        else root.push(node);
      }
    } else {
      const node: TreeNode = { name: f.name, path: f.path, type: "file", extension: f.extension, size: f.size, children: [] };
      if (parts.length <= 1) { root.push(node); }
      else {
        const parentPath = parts.slice(0, -1).join("/");
        const parent = dirMap.get(parentPath);
        if (parent) parent.children.push(node);
        else root.push(node);
      }
    }
  }

  const sortNodes = (nodes: TreeNode[]) => {
    nodes.sort((a, b) => { if (a.type !== b.type) return a.type === "dir" ? -1 : 1; return a.name.localeCompare(b.name); });
    for (const n of nodes) { if (n.children.length > 0) sortNodes(n.children); }
  };
  sortNodes(root);
  return root;
}

function formatSize(bytes?: number): string {
  if (bytes === undefined || bytes === null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ---------------------------------------------------------------------------
// TreeItem
// ---------------------------------------------------------------------------

function TreeItem({
  node, depth, selectedPath, expandedDirs, onSelectFile, onToggleDir,
}: {
  node: TreeNode; depth: number; selectedPath: string | null;
  expandedDirs: Set<string>; onSelectFile: (path: string) => void; onToggleDir: (path: string) => void;
}) {
  const isDir = node.type === "dir";
  const isExpanded = expandedDirs.has(node.path);
  const isSelected = selectedPath === node.path;

  return (
    <>
      <button
        onClick={() => (isDir ? onToggleDir(node.path) : onSelectFile(node.path))}
        className={cn("w-full flex items-center gap-1.5 py-1 px-2 text-[12px] rounded-md transition-colors text-left",
          isSelected ? "bg-glow-cyan/[0.08] text-glow-cyan font-medium" : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-200"
        )}
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
      >
        {isDir ? (
          <>
            {isExpanded ? <ChevronDown className="w-3 h-3 flex-shrink-0 text-slate-400" /> : <ChevronRight className="w-3 h-3 flex-shrink-0 text-slate-400" />}
            {isExpanded ? <FolderOpen className="w-3.5 h-3.5 flex-shrink-0 text-orange-400" /> : <Folder className="w-3.5 h-3.5 flex-shrink-0 text-orange-400" />}
          </>
        ) : (
          <><span className="w-3 flex-shrink-0" /><File className="w-3.5 h-3.5 flex-shrink-0 text-slate-400" /></>
        )}
        <span className="truncate flex-1">{node.name}</span>
        {!isDir && node.extension && <span className={cn("text-[10px] px-1 rounded flex-shrink-0", extBadgeClass(node.extension))}>. {node.extension}</span>}
      </button>
      {isDir && isExpanded && node.children.map((child) => (
        <TreeItem key={child.path} node={child} depth={depth + 1} selectedPath={selectedPath} expandedDirs={expandedDirs} onSelectFile={onSelectFile} onToggleDir={onToggleDir} />
      ))}
    </>
  );
}

// ---------------------------------------------------------------------------
// Modal
// ---------------------------------------------------------------------------

function Modal({ open, onClose, title, children }: { open: boolean; onClose: () => void; title: string; children: React.ReactNode }) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="glass-card rounded-xl border border-white/[0.1] shadow-2xl w-full max-w-lg mx-4">
        <div className="flex items-center justify-between px-5 py-4 border-b border-white/[0.06]">
          <h3 className="text-sm font-semibold text-slate-200">{title}</h3>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close dialog"
            className="text-slate-400 hover:text-slate-200 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="p-5">{children}</div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// ProjectFiles
// ---------------------------------------------------------------------------

export function ProjectFiles({ projectId }: { projectId: string }) {
  const { toast } = useToast();
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [editContent, setEditContent] = useState<string | null>(null);
  const [fileExt, setFileExt] = useState<string>("");
  const [fileSize, setFileSize] = useState<number>(0);
  const [loadingFile, setLoadingFile] = useState(false);
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set());
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saved" | "error">("idle");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [newFileModal, setNewFileModal] = useState(false);
  const [newFilePath, setNewFilePath] = useState("");
  const [restarting, setRestarting] = useState(false);
  const [restartStatus, setRestartStatus] = useState<"idle" | "restarting" | "done" | "error">("idle");

  const isDirty = editContent !== null && editContent !== fileContent;

  const fetchFiles = useCallback(() => {
    if (!projectId) return;
    setLoading(true);
    api.listFiles(projectId)
      .then((data) => {
        setFiles(data.files);
        const topDirs = data.files.filter((f) => f.type === "dir" && !f.path.includes("/")).map((f) => f.path);
        setExpandedDirs(new Set(topDirs));
      })
      .catch(() => setFiles([]))
      .finally(() => setLoading(false));
  }, [projectId]);

  useEffect(() => { fetchFiles(); }, [fetchFiles]);

  const onSelectFile = useCallback((path: string) => {
    if (!projectId) return;
    setSelectedPath(path); setLoadingFile(true); setFileContent(null); setEditContent(null); setSaveStatus("idle");
    api.readFile(projectId, path)
      .then((data) => { setFileContent(data.content); setEditContent(data.content); setFileExt(data.extension); setFileSize(data.size); })
      .catch((err) => {
        const msg = `// Error loading file: ${err?.message || "unknown error"}`;
        setFileContent(msg); setEditContent(msg);
      })
      .finally(() => setLoadingFile(false));
  }, [projectId]);

  const onToggleDir = useCallback((path: string) => {
    setExpandedDirs((prev) => { const next = new Set(prev); if (next.has(path)) next.delete(path); else next.add(path); return next; });
  }, []);

  const handleSave = useCallback(async () => {
    if (!projectId || !selectedPath || editContent === null) return;
    setSaving(true); setSaveStatus("idle");
    try { await api.writeFile(projectId, selectedPath, editContent); setFileContent(editContent); setSaveStatus("saved"); setTimeout(() => setSaveStatus("idle"), 2000); }
    catch { setSaveStatus("error"); }
    finally { setSaving(false); }
  }, [projectId, selectedPath, editContent]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if ((e.metaKey || e.ctrlKey) && e.key === "s") { e.preventDefault(); if (isDirty) handleSave(); } };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isDirty, handleSave]);

  const handleDelete = useCallback(async () => {
    if (!projectId || !selectedPath) return;
    if (typeof window !== "undefined" && !window.confirm(`Delete ${selectedPath}?`)) return;
    try {
      await api.deleteFile(projectId, selectedPath);
      setSelectedPath(null);
      setFileContent(null);
      setEditContent(null);
      fetchFiles();
      toast("success", "File deleted");
    } catch (err) {
      toast("error", "Failed to delete file", err instanceof Error ? err.message : String(err));
    }
  }, [projectId, selectedPath, fetchFiles, toast]);

  const handleNewFile = useCallback(async () => {
    if (!projectId || !newFilePath.trim()) return;
    try {
      await api.writeFile(projectId, newFilePath.trim(), "");
      setNewFileModal(false);
      setNewFilePath("");
      fetchFiles();
      onSelectFile(newFilePath.trim());
    } catch (err) {
      toast("error", "Failed to create file", err instanceof Error ? err.message : String(err));
    }
  }, [projectId, newFilePath, fetchFiles, onSelectFile, toast]);

  const handleRestart = useCallback(async () => {
    if (!projectId) return;
    setRestarting(true); setRestartStatus("restarting");
    try { await api.restartApp(projectId); setRestartStatus("done"); setTimeout(() => setRestartStatus("idle"), 3000); }
    catch { setRestartStatus("error"); setTimeout(() => setRestartStatus("idle"), 3000); }
    finally { setRestarting(false); }
  }, [projectId]);

  const tree = buildTree(files);
  const fileCount = files.filter((f) => f.type === "file").length;

  return (
    <div className="flex flex-col h-full">
      <div className="px-6 pt-4 pb-3 flex items-center justify-between">
        <div>
          <span className="text-sm text-slate-400">
            {loading ? "Loading..." : `${fileCount} generated file${fileCount !== 1 ? "s" : ""}`}
          </span>
        </div>
        {fileCount > 0 && (
          <div className="flex items-center gap-2">
            <button onClick={() => setNewFileModal(true)} className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] rounded-lg bg-white/[0.05] border border-white/[0.08] text-slate-400 hover:text-slate-200 hover:bg-white/[0.08] transition-colors">
              <Plus className="w-3.5 h-3.5" />New File
            </button>
            <a href={api.downloadZipUrl(projectId)} className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] rounded-lg bg-white/[0.05] border border-white/[0.08] text-slate-400 hover:text-slate-200 hover:bg-white/[0.08] transition-colors">
              <Download className="w-3.5 h-3.5" />Download ZIP
            </a>
            <div className="w-px h-5 bg-white/[0.08]" />
            <button onClick={handleRestart} disabled={restarting} className={cn("flex items-center gap-1.5 px-3 py-1.5 text-[12px] rounded-lg border transition-colors", restartStatus === "done" ? "bg-emerald-500/10 border-emerald-500/20 text-emerald-400" : restartStatus === "error" ? "bg-red-500/10 border-red-500/20 text-red-400" : "bg-orange-500/10 border-orange-500/20 text-orange-400 hover:bg-orange-500/20")}>
              {restarting ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RotateCw className="w-3.5 h-3.5" />}
              {restartStatus === "done" ? "Restarted" : restartStatus === "error" ? "Failed" : "Restart App"}
            </button>
          </div>
        )}
      </div>

      {loading ? (
        <div className="flex-1 flex items-center justify-center"><Loader2 className="w-6 h-6 animate-spin text-slate-400" /></div>
      ) : fileCount === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-3 text-slate-400">
          <FileCode className="w-12 h-12 opacity-30" /><p className="text-sm">No generated files yet.</p>
          <button onClick={() => setNewFileModal(true)} className="mt-2 flex items-center gap-1.5 px-4 py-2 text-[12px] rounded-lg bg-glow-cyan/[0.08] text-glow-cyan hover:bg-glow-cyan/[0.12] transition-colors"><Plus className="w-3.5 h-3.5" />Create first file</button>
        </div>
      ) : (
        <div className="flex-1 flex gap-0 min-h-0 px-6 pb-6">
          <div className="w-[280px] flex-shrink-0 glass-card rounded-l-xl border border-white/[0.06] overflow-hidden flex flex-col">
            <div className="px-3 py-2 border-b border-white/[0.06] text-[11px] uppercase tracking-wider text-slate-400/50 font-semibold">Explorer</div>
            <div className="flex-1 overflow-y-auto scrollbar-thin py-1">
              {tree.map((node) => <TreeItem key={node.path} node={node} depth={0} selectedPath={selectedPath} expandedDirs={expandedDirs} onSelectFile={onSelectFile} onToggleDir={onToggleDir} />)}
            </div>
          </div>
          <div className="flex-1 glass-card rounded-r-xl border border-l-0 border-white/[0.06] overflow-hidden flex flex-col min-w-0">
            {selectedPath ? (
              <>
                <div className="px-4 py-2 border-b border-white/[0.06] flex items-center gap-2 text-[12px]">
                  <File className="w-3.5 h-3.5 text-slate-400" />
                  <span className="text-slate-200 font-medium truncate">{selectedPath}</span>
                  {fileExt && <span className={cn("text-[10px] px-1.5 py-0.5 rounded", extBadgeClass(fileExt))}>. {fileExt}</span>}
                  {isDirty && <span className="text-[10px] px-1.5 py-0.5 rounded bg-orange-500/20 text-orange-400">unsaved</span>}
                  {saveStatus === "saved" && <span className="flex items-center gap-1 text-[10px] text-emerald-400"><Check className="w-3 h-3" /> Saved</span>}
                  {saveStatus === "error" && <span className="flex items-center gap-1 text-[10px] text-red-400"><AlertCircle className="w-3 h-3" /> Error saving</span>}
                  <div className="ml-auto flex items-center gap-1.5">
                    <span className="text-slate-400 text-[11px] mr-2">{formatSize(fileSize)}</span>
                    <button onClick={handleSave} disabled={!isDirty || saving} className={cn("flex items-center gap-1 px-2 py-1 rounded text-[11px] transition-colors", isDirty ? "bg-glow-cyan/[0.12] text-glow-cyan hover:bg-glow-cyan/[0.16]" : "bg-white/[0.03] text-slate-400/40 cursor-not-allowed")}>
                      {saving ? <Loader2 className="w-3 h-3 animate-spin" /> : <Save className="w-3 h-3" />}Save
                    </button>
                    <button
                      type="button"
                      onClick={handleDelete}
                      aria-label="Delete file"
                      title="Delete file"
                      className="flex items-center gap-1 px-2 py-1 rounded text-[11px] text-red-400/60 hover:text-red-400 hover:bg-red-500/10 transition-colors"
                    >
                      <Trash2 className="w-3 h-3" />
                    </button>
                  </div>
                </div>
                <div className="flex-1 overflow-auto">
                  {loadingFile ? (
                    <div className="flex items-center justify-center h-full"><Loader2 className="w-5 h-5 animate-spin text-slate-400" /></div>
                  ) : (
                    <div className="flex text-[12px] font-mono leading-5 h-full">
                      <div className="flex-shrink-0 py-3 px-3 text-right select-none text-slate-400/30 border-r border-white/[0.04]">
                        {(editContent ?? "").split("\n").map((_, i) => <div key={i}>{i + 1}</div>)}
                      </div>
                      <textarea
                        ref={textareaRef}
                        value={editContent ?? ""}
                        onChange={(e) => setEditContent(e.target.value)}
                        spellCheck={false}
                        className="flex-1 py-3 px-4 bg-transparent text-slate-200/90 resize-none outline-none font-mono text-[12px] leading-5 whitespace-pre overflow-x-auto"
                        style={{ tabSize: 2 }}
                      />
                    </div>
                  )}
                </div>
              </>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center gap-2 text-slate-400">
                <FileCode className="w-8 h-8 opacity-20" /><p className="text-sm">Select a file to view and edit</p>
                <p className="text-[11px] text-slate-400/50">Cmd+S / Ctrl+S to save</p>
              </div>
            )}
          </div>
        </div>
      )}

      <Modal
        open={newFileModal}
        onClose={() => {
          setNewFileModal(false);
          setNewFilePath("");
        }}
        title="Create New File"
      >
        <div className="space-y-3">
          <div>
            <label className="block text-[11px] text-slate-400 mb-1">File path (relative)</label>
            <input value={newFilePath} onChange={(e) => setNewFilePath(e.target.value)} placeholder="e.g. src/components/Button.tsx" className="w-full px-3 py-2 text-[13px] rounded-lg bg-white/[0.05] border border-white/[0.1] text-slate-200 placeholder:text-slate-400/40 outline-none focus:border-glow-cyan/40 focus:shadow-glow-sm" />
          </div>
          <div className="flex justify-end gap-2">
            <button
              onClick={() => {
                setNewFileModal(false);
                setNewFilePath("");
              }}
              className="px-3 py-1.5 text-[12px] rounded-lg text-slate-400 hover:text-slate-200 transition-colors"
            >
              Cancel
            </button>
            <button onClick={handleNewFile} disabled={!newFilePath.trim()} className="px-4 py-1.5 text-[12px] rounded-lg bg-glow-cyan/[0.12] text-glow-cyan hover:bg-glow-cyan/[0.16] transition-colors disabled:opacity-40">Create</button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
