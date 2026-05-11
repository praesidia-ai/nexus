import type { Config } from "tailwindcss";

const config: Config = {
  darkMode: "class",
  content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: {
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        nexus: {
          void: "#050810",
          deep: "#0a0e1a",
          surface: "#0d1321",
          elevated: "#111827",
          hover: "#1a2332",
          active: "#1e293b",
        },
        glow: {
          cyan: "#00d4ff",
          blue: "#3b82f6",
          purple: "#8b5cf6",
          amber: "#f59e0b",
          pink: "#ec4899",
          white: "#e2e8f0",
        },
      },
      fontFamily: {
        sans: ["var(--font-inter)", "Inter", "system-ui", "sans-serif"],
        mono: ["var(--font-mono)", "JetBrains Mono", "monospace"],
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
        nexus: "12px",
      },
      boxShadow: {
        "glow-sm": "0 0 10px rgba(0, 212, 255, 0.15)",
        "glow-md": "0 0 20px rgba(0, 212, 255, 0.2)",
        "glow-lg":
          "0 0 40px rgba(0, 212, 255, 0.25), 0 0 80px rgba(0, 212, 255, 0.1)",
        "glow-purple":
          "0 0 20px rgba(139, 92, 246, 0.25), 0 0 60px rgba(139, 92, 246, 0.1)",
        "glow-amber":
          "0 0 20px rgba(245, 158, 11, 0.25), 0 0 60px rgba(245, 158, 11, 0.1)",
      },
      keyframes: {
        "pulse-glow": {
          "0%, 100%": { opacity: "0.4" },
          "50%": { opacity: "1" },
        },
        "gradient-shift": {
          "0%": { backgroundPosition: "0% 50%" },
          "50%": { backgroundPosition: "100% 50%" },
          "100%": { backgroundPosition: "0% 50%" },
        },
        bounce: {
          "0%, 80%, 100%": { transform: "translateY(0)", opacity: "0.4" },
          "40%": { transform: "translateY(-4px)", opacity: "1" },
        },
        "fade-in": {
          from: { opacity: "0", transform: "translateY(4px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "glow-pulse": {
          "0%, 100%": { opacity: "0.6", filter: "brightness(1)" },
          "50%": { opacity: "1", filter: "brightness(1.3)" },
        },
        "glow-breathe": {
          "0%, 100%": { boxShadow: "0 0 4px currentColor" },
          "50%": { boxShadow: "0 0 12px currentColor, 0 0 24px currentColor" },
        },
        "spin-slow": {
          from: { transform: "rotate(0deg)" },
          to: { transform: "rotate(360deg)" },
        },
        "spin-slower": {
          from: { transform: "rotate(0deg)" },
          to: { transform: "rotate(360deg)" },
        },
        "fade-in-up": {
          from: { opacity: "0", transform: "translateY(12px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "slide-in-right": {
          from: { opacity: "0", transform: "translateX(16px)" },
          to: { opacity: "1", transform: "translateX(0)" },
        },
        "energy-flow": {
          "0%": { strokeDashoffset: "1000" },
          "100%": { strokeDashoffset: "0" },
        },
        float: {
          "0%, 100%": { transform: "translateY(0px)" },
          "50%": { transform: "translateY(-20px)" },
        },
      },
      animation: {
        "pulse-glow": "pulse-glow 2s ease-in-out infinite",
        "gradient-shift": "gradient-shift 8s ease infinite",
        bounce: "bounce 1.2s ease-in-out infinite",
        "fade-in": "fade-in 0.3s ease-out",
        "glow-pulse": "glow-pulse 3s ease-in-out infinite",
        "glow-breathe": "glow-breathe 2.5s ease-in-out infinite",
        "spin-slow": "spin-slow 12s linear infinite",
        "spin-slower": "spin-slower 20s linear infinite",
        "fade-in-up": "fade-in-up 0.4s ease-out forwards",
        "slide-in-right": "slide-in-right 0.3s ease-out forwards",
        "energy-flow": "energy-flow 3s linear infinite",
        float: "float 6s ease-in-out infinite",
      },
    },
  },
  plugins: [require("tailwindcss-animate")],
};

export default config;
