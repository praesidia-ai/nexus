"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Loader2 } from "lucide-react";
import { api } from "@/lib/api";
import { useToast } from "@/components/toast";
import { NexusLogo } from "@/components/brand/nexus-logo";
import { HomeHeader } from "@/components/home/home-header";
import { GenerationInput } from "@/components/home/generation-input";
import {
  FirstRunGate,
  openFirstRunGateAsModal,
} from "@/components/onboarding/first-run-gate";
import { useLlmKeyStatus } from "@/hooks/use-llm-key-status";
import { useProjects, useDeleteProject } from "@/hooks/api/use-projects";
import { useQueryClient } from "@tanstack/react-query";

export default function HomePage() {
  const router = useRouter();
  const { toast } = useToast();
  const queryClient = useQueryClient();

  const { data: projects = [], isError: backendOffline } = useProjects();
  const deleteProject = useDeleteProject();
  const { hasAnyKey, ollamaAvailable, isLoading: keyStatusLoading } = useLlmKeyStatus();

  const [input, setInput] = useState("");
  const [creating, setCreating] = useState(false);

  async function handleStart(text: string) {
    const description = text.trim();
    if (!description || creating) return;

    // Guard: if the user has no provider configured and Ollama is unavailable,
    // intercept the very first Send and surface the onboarding gate as a
    // modal instead of kicking off a doomed generation. We only block on the
    // first interaction — once the key-status query has resolved we trust it.
    if (!keyStatusLoading && !hasAnyKey && !ollamaAvailable) {
      openFirstRunGateAsModal();
      return;
    }

    setCreating(true);
    try {
      const name = derivePlaceholderName(description);
      // Backend persists the description as the first user message in a fresh
      // conversation when we pass it here, so the workspace can recover it
      // from the database without needing a URL param or sessionStorage.
      const project = await api.createProject(name, description);
      queryClient.invalidateQueries({ queryKey: ["projects"] });
      router.push(`/${project.id}?stream=1`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Could not start generation";
      toast("error", "Failed to start", msg);
      setCreating(false);
    }
  }

  function handleDeleteProject(id: string) {
    deleteProject.mutate(id, {
      onSuccess: () => toast("success", "Project deleted"),
      onError: (err) =>
        toast(
          "error",
          "Could not delete project",
          err instanceof Error ? err.message : "Unknown error",
        ),
    });
  }

  function handleRetryConnection() {
    queryClient.invalidateQueries({ queryKey: ["projects"] });
  }

  return (
    <div className="gradient-bg flex flex-col h-full">
      <HomeHeader
        projects={projects}
        onDeleteProject={handleDeleteProject}
        onNewProject={() => setInput("")}
      />

      <main className="flex-1 flex flex-col items-center justify-center px-6 overflow-y-auto">
        {creating ? (
          <div className="flex flex-col items-center gap-6 fade-in-up">
            <NexusLogo size={120} animated />
            <div className="flex items-center gap-2 text-slate-300 text-sm">
              <Loader2 className="w-4 h-4 animate-spin text-glow-cyan" />
              <span>Creating your project…</span>
            </div>
          </div>
        ) : (
          <div className="flex flex-col items-center w-full">
            <FirstRunGate />
            <GenerationInput
              input={input}
              onInputChange={setInput}
              onSubmit={handleStart}
              onRetryConnection={handleRetryConnection}
              backendOnline={!backendOffline}
              projects={projects}
              hasError={false}
            />
          </div>
        )}
      </main>
    </div>
  );
}

function derivePlaceholderName(description: string): string {
  const cleaned = description.replace(/\s+/g, " ").trim();
  if (cleaned.length <= 60) return cleaned;
  return cleaned.slice(0, 57).trimEnd() + "…";
}
