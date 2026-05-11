import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Emit a self-contained server bundle under `.next/standalone` so the
  // production Docker image can ship a ~50MB tree instead of the full
  // `node_modules/`. The entrypoint is `node .next/standalone/server.js`.
  output: "standalone",

  // Strip `console.log` from production bundles (keep `warn` / `error`).
  compiler: {
    removeConsole: process.env.NODE_ENV === "production"
      ? { exclude: ["error", "warn"] }
      : false,
  },

  // Best-effort hardening headers. The reverse proxy in front is responsible
  // for HSTS + strict CSP since it terminates TLS.
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          { key: "X-Frame-Options", value: "SAMEORIGIN" },
          { key: "X-Content-Type-Options", value: "nosniff" },
          { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
          { key: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=()" },
        ],
      },
    ];
  },

  async rewrites() {
    const apiUrl = process.env.NEXUS_API_URL || "http://localhost:8020";
    return [
      {
        source: "/api/:path*",
        destination: `${apiUrl}/:path*`,
      },
    ];
  },
};

export default nextConfig;
