"use client";

import { useState, useEffect, useCallback } from "react";
import { safeCopyToClipboard } from "@/lib/utils";
import { useParams } from "next/navigation";
import {
  Lock,
  Plus,
  Trash2,
  Copy,
  Check,
  Loader2,
  RefreshCw,
  KeyRound,
  ShieldAlert,
  Eye,
  EyeOff,
  X,
} from "lucide-react";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useToast } from "@/components/toast";
import { BASE } from "@/lib/api";

// ---------------------------------------------------------------------------
// Types — matches backend nexus_store::portal::VaultEntry
// ---------------------------------------------------------------------------

interface VaultEntry {
  id?: string;
  project_id?: string;
  key: string;
  /** Server returns a masked preview of the value (e.g. "••••••••"). */
  value: string;
  created_at?: string;
}

// ---------------------------------------------------------------------------
// AddSecretForm
// ---------------------------------------------------------------------------

function AddSecretForm({
  onAdd,
  adding,
}: {
  onAdd: (key: string, value: string) => void;
  adding: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [showValue, setShowValue] = useState(false);

  const handleSubmit = () => {
    if (!key.trim() || !value.trim()) return;
    onAdd(key.trim(), value.trim());
    setKey("");
    setValue("");
    setShowValue(false);
    setOpen(false);
  };

  if (!open) {
    return (
      <Button
        size="sm"
        className="bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20"
        onClick={() => setOpen(true)}
      >
        <Plus className="h-4 w-4 mr-1.5" />
        Add Secret
      </Button>
    );
  }

  return (
    <Card className="border-white/[0.08] bg-white/[0.02]">
      <CardContent className="p-4 space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium text-slate-200">Add New Secret</span>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0 text-slate-500"
            onClick={() => setOpen(false)}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
        <input
          type="text"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="Secret key (e.g. API_KEY)"
          aria-label="Secret key"
          className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 text-sm text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30 focus:shadow-glow-sm font-mono"
        />
        <div className="relative">
          <input
            type={showValue ? "text" : "password"}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="Secret value"
            aria-label="Secret value"
            className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 pr-10 text-sm text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30 focus:shadow-glow-sm font-mono"
          />
          <button
            type="button"
            aria-label={showValue ? "Hide secret value" : "Show secret value"}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-400"
            onClick={() => setShowValue(!showValue)}
          >
            {showValue ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </button>
        </div>
        <div className="flex items-center gap-2">
          <ShieldAlert className="h-3.5 w-3.5 text-amber-400 shrink-0" />
          <p className="text-[10px] text-amber-400">
            Secrets are encrypted at rest. Never share secrets in chat or logs.
          </p>
        </div>
        <div className="flex justify-end">
          <Button
            size="sm"
            className="bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20"
            onClick={handleSubmit}
            disabled={adding || !key.trim() || !value.trim()}
          >
            {adding ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin mr-1.5" />
            ) : (
              <Lock className="h-3.5 w-3.5 mr-1.5" />
            )}
            Store Secret
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// SecretRow
// ---------------------------------------------------------------------------

function SecretRow({
  entry,
  onDelete,
  deleting,
}: {
  entry: VaultEntry;
  onDelete: (key: string) => void;
  deleting: string | null;
}) {
  const [copied, setCopied] = useState(false);
  const isDeleting = deleting === entry.key;

  const handleCopyKey = () => {
    void safeCopyToClipboard(entry.key);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex items-center gap-3 py-3 border-b border-white/[0.04] last:border-0">
      <div className="rounded-lg bg-violet-500/10 p-2 shrink-0">
        <KeyRound className="h-4 w-4 text-violet-400" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-slate-200 font-mono">{entry.key}</span>
        </div>
        <p className="text-xs text-slate-500 font-mono mt-0.5">{entry.value}</p>
        {entry.created_at && (
          <p className="text-[10px] text-slate-500 mt-0.5">
            Added {new Date(entry.created_at).toLocaleDateString()}
          </p>
        )}
      </div>
      <div className="flex items-center gap-1 shrink-0">
        <Button
          variant="ghost"
          size="sm"
          className="h-8 w-8 p-0 text-slate-500 hover:text-slate-300"
          onClick={handleCopyKey}
        >
          {copied ? (
            <Check className="h-3.5 w-3.5 text-emerald-400" />
          ) : (
            <Copy className="h-3.5 w-3.5" />
          )}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-8 w-8 p-0 text-red-400 hover:text-red-300 hover:bg-red-500/10"
          onClick={() => onDelete(entry.key)}
          disabled={isDeleting}
        >
          {isDeleting ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Trash2 className="h-3.5 w-3.5" />
          )}
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function VaultPage() {
  const params = useParams();
  const projectId = params.projectId as string;
  const { toast } = useToast();

  const [entries, setEntries] = useState<VaultEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);

  const fetchSecrets = useCallback(async () => {
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/vault`);
      if (!res.ok) throw new Error(await res.text());
      const data = await res.json();
      setEntries(Array.isArray(data) ? data : data.secrets ?? []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load vault");
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    fetchSecrets();
  }, [fetchSecrets]);

  const handleAdd = async (key: string, value: string) => {
    setAdding(true);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/vault/${encodeURIComponent(key)}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ value }),
      });
      if (!res.ok) throw new Error(await res.text());
      toast("success", "Secret stored", `${key} saved to vault`);
      await fetchSecrets();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to store secret";
      toast("error", "Could not store secret", msg);
    } finally {
      setAdding(false);
    }
  };

  const handleDelete = async (key: string) => {
    if (!confirm(`Delete secret "${key}"? This cannot be undone.`)) return;
    setDeleting(key);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/vault/${encodeURIComponent(key)}`, {
        method: "DELETE",
      });
      if (!res.ok) throw new Error(await res.text());
      toast("success", "Secret deleted");
      await fetchSecrets();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to delete secret";
      toast("error", "Could not delete secret", msg);
    } finally {
      setDeleting(null);
    }
  };

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-violet-500/20 to-purple-500/20 border border-violet-500/10 flex items-center justify-center">
            <Lock className="w-4 h-4 text-violet-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">Vault</h1>
            <p className="text-xs text-slate-400">
              {loading
                ? "Loading..."
                : `${entries.length} secret${entries.length !== 1 ? "s" : ""} stored`}
            </p>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="text-slate-400 hover:text-slate-200"
          onClick={() => {
            setLoading(true);
            fetchSecrets();
          }}
        >
          <RefreshCw className={`h-4 w-4 mr-1.5 ${loading ? "animate-spin" : ""}`} />
          Refresh
        </Button>
      </div>

      {/* Warning banner */}
      <div className="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3 flex items-start gap-2.5">
        <ShieldAlert className="w-4 h-4 text-amber-400 mt-0.5 shrink-0" />
        <div>
          <p className="text-xs text-amber-300 font-medium">Sensitive Data</p>
          <p className="text-[11px] text-amber-400/70 mt-0.5">
            Secrets stored here are encrypted and accessible only to agents within this project.
            Never share secret values in chat messages or logs.
          </p>
        </div>
      </div>

      {/* Add secret form */}
      <AddSecretForm onAdd={handleAdd} adding={adding} />

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* Secrets list */}
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader>
          <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
            <KeyRound className="w-4 h-4 text-slate-400" />
            Stored Secrets
          </CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="space-y-3">
              {[1, 2, 3].map((i) => (
                <div key={i} className="flex items-center gap-3 py-3">
                  <Skeleton className="h-8 w-8 rounded-lg" />
                  <div className="flex-1 space-y-1.5">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-3 w-24" />
                  </div>
                </div>
              ))}
            </div>
          ) : entries.length === 0 ? (
            <div className="py-8 text-center">
              <Lock className="w-8 h-8 text-slate-600 mx-auto mb-2" />
              <p className="text-sm text-slate-400">No secrets stored</p>
              <p className="text-xs text-slate-500 mt-1">
                Add secrets like API keys, tokens, or credentials
              </p>
            </div>
          ) : (
            entries.map((entry) => (
              <SecretRow
                key={entry.key}
                entry={entry}
                onDelete={handleDelete}
                deleting={deleting}
              />
            ))
          )}
        </CardContent>
      </Card>
    </div>
  );
}
