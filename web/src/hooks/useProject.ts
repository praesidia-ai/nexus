"use client";

import { useState, useEffect, useCallback } from "react";
import { api, type Project, ApiError } from "@/lib/api";

export function useProject(projectId: string) {
  const [project, setProject] = useState<Project | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    api
      .getProject(projectId)
      .then(setProject)
      .catch((err: unknown) => {
        setError(err instanceof ApiError ? err.message : "Failed to load project");
      })
      .finally(() => setLoading(false));
  }, [projectId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { project, loading, error, refresh };
}

export function useProjects() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    api
      .listProjects()
      .then(setProjects)
      .catch((err: unknown) => {
        setError(err instanceof ApiError ? err.message : "Failed to load projects");
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const create = useCallback(
    async (name: string, description?: string): Promise<Project> => {
      const project = await api.createProject(name, description);
      setProjects((prev) => [...prev, project]);
      return project;
    },
    []
  );

  const remove = useCallback(async (id: string): Promise<void> => {
    await api.deleteProject(id);
    setProjects((prev) => prev.filter((p) => p.id !== id));
  }, []);

  return { projects, loading, error, create, remove, refresh };
}
