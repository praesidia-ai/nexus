'use client';

import { useState, useEffect, useRef } from 'react';
import { ChevronDown, Cpu, Check, Lock } from 'lucide-react';
import { cn } from '@/lib/utils';
import { api } from '@/lib/api';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ProviderModel {
  id: string;
  name: string;
  context_window: number;
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
  openai: 'bg-emerald-500',
  anthropic: 'bg-orange-500',
  gemini: 'bg-blue-400',
  mistral: 'bg-indigo-500',
  deepseek: 'bg-cyan-500',
  xai: 'bg-slate-500',
  groq: 'bg-red-500',
  ollama: 'bg-blue-500',
  openrouter: 'bg-purple-500',
};

// Fallback providers when backend is unreachable
const FALLBACK_PROVIDERS: Provider[] = [
  { id: "openai", name: "OpenAI", api_base: "", requires_key: true, models: [
    { id: "gpt-5.4", name: "GPT-5.4", context_window: 400000 },
    { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", context_window: 400000 },
    { id: "gpt-5.4-nano", name: "GPT-5.4 Nano", context_window: 128000 },
    { id: "gpt-5.2", name: "GPT-5.2", context_window: 400000 },
    { id: "gpt-5", name: "GPT-5", context_window: 400000 },
    { id: "gpt-4.1", name: "GPT-4.1", context_window: 1047576 },
  ]},
  { id: "anthropic", name: "Anthropic", api_base: "", requires_key: true, models: [
    { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", context_window: 1000000 },
    { id: "claude-opus-4-6", name: "Claude Opus 4.6", context_window: 1000000 },
    { id: "claude-haiku-4-5-20251001", name: "Claude Haiku 4.5", context_window: 200000 },
  ]},
  { id: "gemini", name: "Google Gemini", api_base: "", requires_key: true, models: [
    { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", context_window: 1048576 },
    { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash", context_window: 1048576 },
  ]},
  { id: "mistral", name: "Mistral AI", api_base: "", requires_key: true, models: [
    { id: "mistral-large-latest", name: "Mistral Large 3", context_window: 256000 },
    { id: "codestral-latest", name: "Codestral", context_window: 256000 },
  ]},
  { id: "deepseek", name: "DeepSeek", api_base: "", requires_key: true, models: [
    { id: "deepseek-chat", name: "DeepSeek V3", context_window: 128000 },
    { id: "deepseek-reasoner", name: "DeepSeek V3 Reasoning", context_window: 128000 },
  ]},
  { id: "groq", name: "Groq", api_base: "", requires_key: true, models: [
    { id: "llama-3.3-70b-versatile", name: "Llama 3.3 70B", context_window: 131072 },
  ]},
  { id: "ollama", name: "Ollama (Local)", api_base: "", requires_key: false, models: [
    { id: "llama3.3", name: "Llama 3.3 70B", context_window: 128000 },
    { id: "qwen3", name: "Qwen3", context_window: 128000 },
    { id: "deepseek-r1", name: "DeepSeek R1", context_window: 128000 },
    { id: "mistral", name: "Mistral 7B", context_window: 32000 },
  ]},
  { id: "openrouter", name: "OpenRouter", api_base: "", requires_key: true, models: [
    { id: "anthropic/claude-sonnet-4-6", name: "Claude Sonnet 4.6", context_window: 1000000 },
    { id: "google/gemini-2.5-flash", name: "Gemini 2.5 Flash", context_window: 1048576 },
    { id: "deepseek/deepseek-chat", name: "DeepSeek V3", context_window: 128000 },
  ]},
];

function formatContextWindow(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(tokens % 1_000_000 === 0 ? 0 : 1)}M`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K`;
  return `${tokens}`;
}

// ---------------------------------------------------------------------------
// ModelSelector
// ---------------------------------------------------------------------------

interface ModelSelectorProps {
  selectedProvider?: string;
  selectedModel?: string;
  onChange: (provider: string, model: string) => void;
  className?: string;
}

export function ModelSelector({ selectedProvider, selectedModel, onChange, className }: ModelSelectorProps) {
  const [open, setOpen] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [apiKeyStatuses, setApiKeyStatuses] = useState<ApiKeyStatus[]>([]);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Start with fallback (instant render)
    setProviders(FALLBACK_PROVIDERS);
    // Then try to load from backend
    Promise.all([
      api.listProviders().catch(() => null),
      api.listApiKeys().catch(() => null),
    ]).then(([provs, keys]) => {
      if (provs && Array.isArray(provs) && provs.length > 0) setProviders(provs);
      if (keys?.providers) setApiKeyStatuses(keys.providers);
    });
  }, []);

  // Close on outside click
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener('mousedown', handleClick);
      return () => document.removeEventListener('mousedown', handleClick);
    }
  }, [open]);

  function isConfigured(providerId: string): boolean {
    // Ollama doesn't need a key
    const prov = providers.find((p) => p.id === providerId);
    if (prov && !prov.requires_key) return true;
    return apiKeyStatuses.some((k) => k.provider === providerId && k.configured);
  }

  // Find display name for the current model
  const currentModelName = (() => {
    if (!selectedProvider || !selectedModel) return null;
    const prov = providers.find((p) => p.id === selectedProvider);
    const model = prov?.models.find((m) => m.id === selectedModel);
    return model?.name ?? selectedModel;
  })();

  return (
    <div ref={dropdownRef} className={cn('relative', className)}>
      {/* Trigger button */}
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={cn(
          'flex items-center gap-1.5 h-[42px] px-3 rounded-xl text-xs transition-colors',
          'border border-white/[0.08] bg-white/[0.03] hover:bg-white/[0.06]',
          'text-slate-400 hover:text-slate-200'
        )}
      >
        <Cpu className="w-3.5 h-3.5" />
        <span className="max-w-[120px] truncate">
          {currentModelName ?? 'Select model'}
        </span>
        <ChevronDown className={cn('w-3 h-3 transition-transform', open && 'rotate-180')} />
      </button>

      {/* Dropdown panel */}
      {open && (
        <div className="absolute bottom-full mb-2 left-0 w-72 max-h-80 overflow-y-auto rounded-xl border border-white/[0.08] bg-[#0c0c0c]/95 backdrop-blur-xl shadow-2xl z-50 scrollbar-thin">
          <div className="p-2">
            {providers.map((provider) => {
              const configured = isConfigured(provider.id);
              const color = PROVIDER_COLORS[provider.id] ?? 'bg-slate-500';

              return (
                <div key={provider.id} className="mb-1 last:mb-0">
                  {/* Provider header */}
                  <div className="flex items-center gap-2 px-2 py-1.5">
                    <span className={cn('w-2 h-2 rounded-full flex-shrink-0', color)} />
                    <span className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider flex-1">
                      {provider.name}
                    </span>
                    {configured ? (
                      <Check className="w-3 h-3 text-emerald-400" />
                    ) : (
                      <span className="text-[10px] text-slate-400/50 flex items-center gap-1">
                        <Lock className="w-2.5 h-2.5" /> No API key
                      </span>
                    )}
                  </div>

                  {/* Models */}
                  {provider.models.map((model) => {
                    const isSelected = selectedProvider === provider.id && selectedModel === model.id;
                    return (
                      <button
                        key={model.id}
                        type="button"
                        disabled={!configured}
                        onClick={() => {
                          onChange(provider.id, model.id);
                          setOpen(false);
                        }}
                        className={cn(
                          'w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-xs transition-colors',
                          configured
                            ? 'hover:bg-white/[0.06] cursor-pointer'
                            : 'opacity-40 cursor-not-allowed',
                          isSelected && 'bg-glow-cyan/[0.08] text-glow-cyan'
                        )}
                      >
                        <span className="flex-1 text-left truncate text-slate-200">
                          {model.name}
                        </span>
                        <span className="text-[10px] text-slate-400 flex-shrink-0">
                          {formatContextWindow(model.context_window)}
                        </span>
                        {isSelected && <Check className="w-3 h-3 text-glow-cyan flex-shrink-0" />}
                      </button>
                    );
                  })}
                </div>
              );
            })}
            {providers.length === 0 && (
              <p className="text-xs text-slate-400 text-center py-4">Loading models...</p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
