"use client";

import { createContext, useContext, useEffect, useState } from "react";

type Theme = "dark" | "system";

const ThemeContext = createContext<{
  theme: Theme;
  resolvedTheme: "dark" | "light";
  setTheme: (theme: Theme) => void;
}>({ theme: "dark", resolvedTheme: "dark", setTheme: () => {} });

function getResolvedTheme(theme: Theme): "dark" | "light" {
  if (theme === "system") {
    if (typeof window !== "undefined") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }
    return "dark";
  }
  return theme;
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<Theme>("dark");
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">("dark");

  // Load saved preference
  useEffect(() => {
    const saved = localStorage.getItem("nexus-theme");
    if (saved === "dark" || saved === "system") {
      setTheme(saved);
    }
  }, []);

  // Apply theme to document and listen for system changes
  useEffect(() => {
    const root = document.documentElement;
    const resolved = getResolvedTheme(theme);
    setResolvedTheme(resolved);
    localStorage.setItem("nexus-theme", theme);

    // Nexus is dark-first: always keep "dark" class for the design system.
    // Only remove it when explicitly resolved to light AND a future light theme exists.
    // For now, always enforce dark since all CSS is dark-only.
    root.classList.add("dark");

    if (theme === "system") {
      const mql = window.matchMedia("(prefers-color-scheme: dark)");
      const handler = (e: MediaQueryListEvent) => {
        setResolvedTheme(e.matches ? "dark" : "light");
        // Keep dark class — Nexus doesn't have light theme CSS yet
        root.classList.add("dark");
      };
      mql.addEventListener("change", handler);
      return () => mql.removeEventListener("change", handler);
    }
  }, [theme]);

  return (
    <ThemeContext.Provider value={{ theme, resolvedTheme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

export const useTheme = () => useContext(ThemeContext);
