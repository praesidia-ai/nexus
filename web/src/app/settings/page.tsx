"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { ArrowLeft, Check, Trash2, Wifi, WifiOff, Zap, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import { useToast } from "@/components/toast";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ProviderModel {
  id: string;
  name: string;
  context_window: number;
  supports_streaming?: boolean;
}

interface Provider {
  id: string;
  name: string;
  api_base: string;
  requires_key: boolean;
  models: ProviderModel[];
}

interface ApiKeyStatus {
  provider: string;
  configured: boolean;
  source: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PROVIDER_COLORS: Record<string, string> = {
  openai: "bg-emerald-500",
  anthropic: "bg-orange-500",
  gemini: "bg-blue-400",
  mistral: "bg-indigo-500",
  deepseek: "bg-cyan-500",
  xai: "bg-slate-500",
  groq: "bg-red-500",
  ollama: "bg-blue-500",
  openrouter: "bg-purple-500",
};

const PROVIDER_DESCRIPTIONS: Record<string, string> = {
  openai: "GPT-4.1, GPT-4o, o3-mini and more from OpenAI.",
  anthropic: "Claude Sonnet 4.6, Opus 4.6, Haiku 4.5 from Anthropic.",
  gemini: "Gemini 2.5 Pro, Flash models from Google.",
  mistral: "Mistral Large 3, Codestral, Devstral from Mistral AI.",
  deepseek: "DeepSeek V3 Chat and Reasoning models.",
  xai: "Grok 3 models from xAI.",
  groq: "Ultra-fast inference — Llama, Qwen on Groq hardware.",
  ollama: "Run open-source models locally. No API key needed.",
  openrouter: "Unified API for 500+ models from all providers.",
};

// Fallback providers when backend is unreachable
const FALLBACK_PROVIDERS: Provider[] = [
  {
    id: "openai", name: "OpenAI", api_base: "https://api.openai.com/v1", requires_key: true,
    models: [
      { id: "gpt-4o", name: "GPT-4o", context_window: 128000 },
      { id: "gpt-4o-mini", name: "GPT-4o Mini", context_window: 128000 },
      { id: "gpt-4-turbo", name: "GPT-4 Turbo", context_window: 128000 },
    ],
  },
  {
    id: "anthropic", name: "Anthropic", api_base: "https://api.anthropic.com/v1", requires_key: true,
    models: [
      { id: "claude-sonnet-4-5-20250929", name: "Claude Sonnet 4.5", context_window: 200000 },
      { id: "claude-opus-4-20250514", name: "Claude Opus 4", context_window: 200000 },
      { id: "claude-3-5-sonnet-20241022", name: "Claude 3.5 Sonnet", context_window: 200000 },
      { id: "claude-3-5-haiku-20241022", name: "Claude 3.5 Haiku", context_window: 200000 },
    ],
  },
  {
    id: "gemini", name: "Google Gemini", api_base: "https://generativelanguage.googleapis.com/v1beta", requires_key: true,
    models: [
      { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", context_window: 1048576 },
      { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash", context_window: 1048576 },
      { id: "gemini-2.5-flash-lite", name: "Gemini 2.5 Flash-Lite", context_window: 1048576 },
    ],
  },
  {
    id: "mistral", name: "Mistral AI", api_base: "https://api.mistral.ai/v1", requires_key: true,
    models: [
      { id: "mistral-large-latest", name: "Mistral Large 3", context_window: 256000 },
      { id: "mistral-small-latest", name: "Mistral Small 4", context_window: 256000 },
      { id: "codestral-latest", name: "Codestral", context_window: 256000 },
      { id: "devstral-latest", name: "Devstral 2", context_window: 256000 },
      { id: "ministral-8b-latest", name: "Ministral 8B", context_window: 256000 },
    ],
  },
  {
    id: "deepseek", name: "DeepSeek", api_base: "https://api.deepseek.com/v1", requires_key: true,
    models: [
      { id: "deepseek-chat", name: "DeepSeek V3 (Chat)", context_window: 128000 },
      { id: "deepseek-reasoner", name: "DeepSeek V3 (Reasoning)", context_window: 128000 },
    ],
  },
  {
    id: "xai", name: "xAI (Grok)", api_base: "https://api.x.ai/v1", requires_key: true,
    models: [
      { id: "grok-3-fast", name: "Grok 3 Fast", context_window: 131072 },
      { id: "grok-3-mini-fast", name: "Grok 3 Mini Fast", context_window: 131072 },
    ],
  },
  {
    id: "groq", name: "Groq", api_base: "https://api.groq.com/openai/v1", requires_key: true,
    models: [
      { id: "llama-3.3-70b-versatile", name: "Llama 3.3 70B", context_window: 131072 },
      { id: "llama-3.1-8b-instant", name: "Llama 3.1 8B", context_window: 131072 },
      { id: "meta-llama/llama-4-scout-17b-16e-instruct", name: "Llama 4 Scout 17B", context_window: 131072 },
      { id: "qwen/qwen3-32b", name: "Qwen3 32B", context_window: 131072 },
    ],
  },
  {
    id: "ollama", name: "Ollama (Local)", api_base: "http://localhost:11434/v1", requires_key: false,
    models: [
      { id: "llama3.3", name: "Llama 3.3 70B", context_window: 128000 },
      { id: "llama3.2", name: "Llama 3.2 3B", context_window: 128000 },
      { id: "llama3.1", name: "Llama 3.1 8B", context_window: 128000 },
      { id: "qwen3", name: "Qwen3", context_window: 128000 },
      { id: "qwen2.5-coder", name: "Qwen 2.5 Coder", context_window: 128000 },
      { id: "gemma3", name: "Gemma 3", context_window: 128000 },
      { id: "mistral", name: "Mistral 7B", context_window: 32000 },
      { id: "deepseek-r1", name: "DeepSeek R1", context_window: 128000 },
      { id: "phi4", name: "Phi-4 14B", context_window: 16000 },
      { id: "codellama", name: "Code Llama", context_window: 16000 },
    ],
  },
  {
    id: "openrouter", name: "OpenRouter", api_base: "https://openrouter.ai/api/v1", requires_key: true,
    models: [
      { id: "anthropic/claude-3.5-sonnet", name: "Claude 3.5 Sonnet", context_window: 200000 },
      { id: "openai/gpt-4o", name: "GPT-4o", context_window: 128000 },
      { id: "google/gemini-2.0-flash-exp", name: "Gemini 2.0 Flash", context_window: 1000000 },
      { id: "deepseek/deepseek-chat", name: "DeepSeek V3", context_window: 128000 },
      { id: "meta-llama/llama-3.3-70b-instruct", name: "Llama 3.3 70B", context_window: 131072 },
    ],
  },
];

function formatContextWindow(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(tokens % 1_000_000 === 0 ? 0 : 1)}M`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K`;
  return `${tokens}`;
}

// ---------------------------------------------------------------------------
// Settings Page
// ---------------------------------------------------------------------------

export default function SettingsPage() {
  const { toast } = useToast();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [apiKeyStatuses, setApiKeyStatuses] = useState<ApiKeyStatus[]>([]);
  const [apiKeyInputs, setApiKeyInputs] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState<Record<string, boolean>>({});
  const [removing, setRemoving] = useState<Record<string, boolean>>({});
  const [ollamaStatus, setOllamaStatus] = useState<"idle" | "testing" | "ok" | "error">("idle");

  // Default model state
  const [defaultProvider, setDefaultProvider] = useState<string>("");
  const [defaultModel, setDefaultModel] = useState<string>("");
  const [selectedProvider, setSelectedProvider] = useState<string>("");
  const [selectedModel, setSelectedModel] = useState<string>("");
  const [savingDefault, setSavingDefault] = useState(false);

  useEffect(() => {
    loadData();
  }, []);

  async function loadData() {
    // Always start with fallback providers (instant render, no empty state)
    setProviders(FALLBACK_PROVIDERS);

    try {
      const [provs, keys, defModel] = await Promise.all([
        api.listProviders().catch(() => null),
        api.listApiKeys().catch(() => null),
        api.getDefaultModel().catch(() => null),
      ]);
      // Override with backend data if available
      if (provs && Array.isArray(provs) && provs.length > 0) {
        setProviders(provs);
      }
      if (keys?.providers) {
        setApiKeyStatuses(keys.providers);
      }
      if (defModel) {
        setDefaultProvider(defModel.provider);
        setDefaultModel(defModel.model);
        setSelectedProvider(defModel.provider);
        setSelectedModel(defModel.model);
      }
    } catch {
      // Backend unreachable — fallback providers already set
    }
  }

  function isConfigured(providerId: string): boolean {
    return apiKeyStatuses.some((k) => k.provider === providerId && k.configured);
  }

  function validateApiKey(providerId: string, key: string): string | null {
    if (key.length < 8) return "API key looks too short.";
    if (key.length > 512) return "API key looks too long.";
    // Keys must be ASCII printables without spaces or control chars —
    // if the user pasted a label or accidental newline we catch it early.
    // eslint-disable-next-line no-control-regex
    if (/[\s\u0000-\u001f\u007f]/.test(key)) return "API key contains whitespace or control characters.";
    const prefixes: Record<string, string[]> = {
      openai: ["sk-"],
      anthropic: ["sk-ant-"],
      openrouter: ["sk-or-"],
    };
    const expected = prefixes[providerId];
    if (expected && !expected.some((p) => key.startsWith(p))) {
      return `Expected ${providerId} key to start with one of: ${expected.join(", ")}`;
    }
    return null;
  }

  async function handleSaveKey(providerId: string) {
    const raw = apiKeyInputs[providerId];
    if (!raw?.trim()) return;
    const key = raw.trim();
    const validationError = validateApiKey(providerId, key);
    if (validationError) {
      toast("error", "Invalid API key", validationError);
      return;
    }
    setSaving((p) => ({ ...p, [providerId]: true }));
    try {
      await api.setApiKey(providerId, key);
      setApiKeyInputs((p) => ({ ...p, [providerId]: "" }));
      const keys = await api.listApiKeys();
      setApiKeyStatuses(keys.providers ?? []);
      const name = providers.find((p) => p.id === providerId)?.name ?? providerId;
      toast("success", "API key saved", `${name} is now configured.`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to save key";
      toast("error", "Could not save API key", msg);
    } finally {
      setSaving((p) => ({ ...p, [providerId]: false }));
    }
  }

  async function handleRemoveKey(providerId: string) {
    setRemoving((p) => ({ ...p, [providerId]: true }));
    try {
      await api.deleteApiKey(providerId);
      const keys = await api.listApiKeys();
      setApiKeyStatuses(keys.providers ?? []);
      toast("success", "API key removed");
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to remove key";
      toast("error", "Could not remove API key", msg);
    } finally {
      setRemoving((p) => ({ ...p, [providerId]: false }));
    }
  }

  async function testOllama() {
    setOllamaStatus("testing");
    try {
      const info = await api.listAgentModels();
      if (info.ollama_available) {
        setOllamaStatus("ok");
        toast("success", "Ollama connected", `${info.ollama_models.length} models available`);
      } else {
        setOllamaStatus("error");
        toast("error", "Ollama not reachable", "Make sure Ollama is running on localhost:11434");
      }
    } catch {
      setOllamaStatus("error");
      toast("error", "Connection test failed", "Backend may be offline");
    }
  }

  async function handleSaveDefault() {
    if (!selectedProvider || !selectedModel) return;
    setSavingDefault(true);
    try {
      await api.setDefaultModel(selectedProvider, selectedModel);
      setDefaultProvider(selectedProvider);
      setDefaultModel(selectedModel);
      const provName = providers.find((p) => p.id === selectedProvider)?.name ?? selectedProvider;
      const modelName = providers.find((p) => p.id === selectedProvider)?.models.find((m) => m.id === selectedModel)?.name ?? selectedModel;
      toast("success", "Default model saved", `${provName} / ${modelName}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to save default model";
      toast("error", "Could not save default model", msg);
    } finally {
      setSavingDefault(false);
    }
  }

  const modelsForSelectedProvider = providers.find((p) => p.id === selectedProvider)?.models ?? [];

  // All models across all providers for the grid
  const allModels = providers.flatMap((p) =>
    p.models.map((m) => ({ ...m, provider: p.id, providerName: p.name }))
  );

  return (
    <div className="gradient-bg min-h-screen">
      <div className="max-w-4xl mx-auto px-6 py-8">
        {/* Back link */}
        <Link
          href="/"
          className="inline-flex items-center gap-1.5 text-sm text-slate-400 hover:text-slate-200 transition-colors mb-6"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Home
        </Link>

        {/* Page header */}
        <div className="mb-8">
          <h1 className="text-2xl font-bold text-slate-200">Settings</h1>
          <p className="text-slate-400 text-sm mt-1">
            Configure LLM providers and API keys
          </p>
        </div>

        {/* ----------------------------------------------------------------- */}
        {/* Section 1: API Keys */}
        {/* ----------------------------------------------------------------- */}
        <div className="space-y-4 mb-10">
          <h2 className="text-lg font-semibold text-slate-200">API Keys</h2>
          <div className="grid gap-4">
            {providers.map((provider) => {
              const configured = isConfigured(provider.id);
              const color = PROVIDER_COLORS[provider.id] ?? "bg-slate-500";
              const desc = PROVIDER_DESCRIPTIONS[provider.id] ?? provider.api_base;
              const isOllama = provider.id === "ollama";

              return (
                <Card key={provider.id}>
                  <CardHeader className="pb-3">
                    <div className="flex items-center gap-3">
                      <div className={cn("w-8 h-8 rounded-lg flex items-center justify-center", color)}>
                        <span className="text-white font-bold text-sm">
                          {provider.name.charAt(0).toUpperCase()}
                        </span>
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <CardTitle className="text-sm">{provider.name}</CardTitle>
                          {configured && (
                            <Badge variant="success" className="text-[10px]">
                              <Check className="w-3 h-3 mr-0.5" />
                              Configured
                            </Badge>
                          )}
                        </div>
                        <CardDescription className="text-xs mt-0.5">{desc}</CardDescription>
                      </div>
                    </div>
                  </CardHeader>
                  <CardContent>
                    {isOllama ? (
                      <div className="space-y-3">
                        <p className="text-xs text-slate-400">
                          No API key needed. Make sure Ollama is running on localhost:11434
                        </p>
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={testOllama}
                          disabled={ollamaStatus === "testing"}
                          className="text-xs"
                        >
                          {ollamaStatus === "testing" ? (
                            "Testing..."
                          ) : ollamaStatus === "ok" ? (
                            <><Wifi className="w-3.5 h-3.5 mr-1.5 text-emerald-400" /> Connected</>
                          ) : ollamaStatus === "error" ? (
                            <><WifiOff className="w-3.5 h-3.5 mr-1.5 text-red-400" /> Connection Failed</>
                          ) : (
                            <><Wifi className="w-3.5 h-3.5 mr-1.5" /> Test Connection</>
                          )}
                        </Button>
                      </div>
                    ) : provider.requires_key ? (
                      <div className="flex gap-2">
                        <Input
                          type="password"
                          placeholder={configured ? "******* (key is set)" : `Enter ${provider.name} API key...`}
                          value={apiKeyInputs[provider.id] ?? ""}
                          onChange={(e) =>
                            setApiKeyInputs((p) => ({ ...p, [provider.id]: e.target.value }))
                          }
                          className="flex-1 text-xs h-9"
                        />
                        <Button
                          size="sm"
                          onClick={() => handleSaveKey(provider.id)}
                          disabled={!apiKeyInputs[provider.id]?.trim() || saving[provider.id]}
                          className="text-xs h-9"
                        >
                          {saving[provider.id] ? "Saving..." : "Save"}
                        </Button>
                        {configured && (
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={() => handleRemoveKey(provider.id)}
                            disabled={removing[provider.id]}
                            className="text-xs h-9 text-red-400 hover:text-red-300"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </Button>
                        )}
                      </div>
                    ) : (
                      <p className="text-xs text-slate-400">
                        This provider does not require an API key.
                      </p>
                    )}
                  </CardContent>
                </Card>
              );
            })}
          </div>
        </div>

        {/* ----------------------------------------------------------------- */}
        {/* Section 2: Default Model */}
        {/* ----------------------------------------------------------------- */}
        <div className="mb-10">
          <h2 className="text-lg font-semibold text-slate-200 mb-4">Default Model</h2>
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm">Default Model</CardTitle>
              <CardDescription className="text-xs">
                Choose which model to use by default for new conversations.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {defaultProvider && defaultModel && (
                <div className="mb-4 flex items-center gap-2">
                  <span className="text-xs text-slate-400">Current:</span>
                  <Badge variant="default" className="text-[10px]">
                    {defaultProvider} / {defaultModel}
                  </Badge>
                </div>
              )}
              <div className="flex flex-col sm:flex-row gap-3">
                {/* Provider select */}
                <div className="relative flex-1">
                  <select
                    value={selectedProvider}
                    onChange={(e) => {
                      setSelectedProvider(e.target.value);
                      setSelectedModel("");
                    }}
                    className="w-full h-9 rounded-lg border border-white/[0.08] bg-white/[0.03] backdrop-blur-sm px-3 text-sm text-slate-200 appearance-none cursor-pointer focus:outline-none focus:shadow-glow-sm"
                  >
                    <option value="" className="bg-[#0a0a0a]">Select provider...</option>
                    {providers.map((p) => (
                      <option key={p.id} value={p.id} className="bg-[#0a0a0a]">
                        {p.name}
                      </option>
                    ))}
                  </select>
                  <ChevronDown className="absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-slate-400 pointer-events-none" />
                </div>

                {/* Model select */}
                <div className="relative flex-1">
                  <select
                    value={selectedModel}
                    onChange={(e) => setSelectedModel(e.target.value)}
                    disabled={!selectedProvider}
                    className="w-full h-9 rounded-lg border border-white/[0.08] bg-white/[0.03] backdrop-blur-sm px-3 text-sm text-slate-200 appearance-none cursor-pointer focus:outline-none focus:shadow-glow-sm disabled:opacity-50"
                  >
                    <option value="" className="bg-[#0a0a0a]">Select model...</option>
                    {modelsForSelectedProvider.map((m) => (
                      <option key={m.id} value={m.id} className="bg-[#0a0a0a]">
                        {m.name} ({formatContextWindow(m.context_window)})
                      </option>
                    ))}
                  </select>
                  <ChevronDown className="absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-slate-400 pointer-events-none" />
                </div>

                <Button
                  size="sm"
                  onClick={handleSaveDefault}
                  disabled={!selectedProvider || !selectedModel || savingDefault}
                  className="text-xs h-9"
                >
                  {savingDefault ? "Saving..." : "Save Default"}
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* ----------------------------------------------------------------- */}
        {/* Section 3: Available Models */}
        {/* ----------------------------------------------------------------- */}
        <div className="mb-10">
          <h2 className="text-lg font-semibold text-slate-200 mb-4">Available Models</h2>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {allModels.map((m) => {
              const color = PROVIDER_COLORS[m.provider] ?? "bg-slate-500";
              return (
                <Card key={`${m.provider}-${m.id}`} className="p-4">
                  <div className="flex items-start justify-between gap-2 mb-2">
                    <span className="text-sm font-medium text-slate-200 truncate">{m.name}</span>
                    <Badge
                      variant="outline"
                      className="text-[10px] flex-shrink-0 flex items-center gap-1"
                    >
                      <span className={cn("w-1.5 h-1.5 rounded-full", color)} />
                      {m.providerName}
                    </Badge>
                  </div>
                  <div className="flex items-center gap-3 text-xs text-slate-400">
                    <span>{formatContextWindow(m.context_window)} context</span>
                    <span className="flex items-center gap-1">
                      <Zap className="w-3 h-3 text-emerald-400" />
                      Streaming
                    </span>
                  </div>
                </Card>
              );
            })}
            {allModels.length === 0 && (
              <p className="text-sm text-slate-400 col-span-full text-center py-8">
                No providers loaded yet.
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
