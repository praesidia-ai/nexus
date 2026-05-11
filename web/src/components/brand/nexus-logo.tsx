"use client";

import Image from "next/image";
import { cn } from "@/lib/utils";

interface NexusLogoProps {
  size?: number;
  animated?: boolean;
  className?: string;
}

export function NexusLogo({ size = 120, animated = true, className }: NexusLogoProps) {
  return (
    <div
      className={cn("relative inline-flex items-center justify-center", className)}
      style={{ width: size, height: size }}
    >
      {/* Outer ambient glow */}
      <div
        className={cn("absolute rounded-full", animated && "animate-glow-pulse")}
        style={{
          width: size * 1.6,
          height: size * 1.6,
          background: [
            "radial-gradient(circle,",
            "rgba(0,212,255,0.18) 0%,",
            "rgba(59,130,246,0.12) 18%,",
            "rgba(139,92,246,0.08) 35%,",
            "rgba(139,92,246,0.03) 55%,",
            "transparent 70%)",
          ].join(" "),
          filter: "blur(24px)",
          inset: "50%",
          transform: "translate(-50%, -50%)",
          position: "absolute",
        }}
      />

      {/* Slow-rotating glow ring behind the image */}
      {animated && (
        <div
          className="absolute rounded-full"
          style={{
            width: size * 1.1,
            height: size * 1.1,
            background: "conic-gradient(from 0deg, transparent 0%, rgba(0,212,255,0.12) 20%, rgba(139,92,246,0.10) 40%, rgba(212,160,23,0.06) 60%, transparent 80%)",
            animation: "nexus-logo-spin 12s linear infinite",
            filter: "blur(8px)",
          }}
        />
      )}

      {/* The logo image — slow gentle rotation */}
      <div
        className="relative"
        style={{
          width: size,
          height: size,
          ...(animated ? { animation: "nexus-logo-spin 40s linear infinite" } : {}),
        }}
      >
        <Image
          src="/image/nexus-logo.png"
          alt="Nexus logo"
          width={size}
          height={size}
          priority
          className="object-contain"
          style={{ filter: "drop-shadow(0 0 8px rgba(0,212,255,0.4)) drop-shadow(0 0 24px rgba(139,92,246,0.25))" }}
        />
      </div>

      {/* Center starburst pulse */}
      {animated && (
        <div
          className="absolute rounded-full pointer-events-none animate-glow-pulse"
          style={{
            width: size * 0.1,
            height: size * 0.1,
            background: "radial-gradient(circle, rgba(255,255,255,0.9) 0%, rgba(0,212,255,0.6) 40%, transparent 70%)",
            filter: "blur(3px)",
          }}
        />
      )}
    </div>
  );
}

export function NexusLogoMini({ size = 28, animated = true, className }: {
  size?: number;
  animated?: boolean;
  className?: string;
}) {
  return (
    <div
      className={cn("relative inline-flex items-center justify-center flex-shrink-0", className)}
      style={{ width: size, height: size }}
    >
      {animated && (
        <div
          className="absolute rounded-full animate-glow-pulse"
          style={{
            width: size * 1.8,
            height: size * 1.8,
            background: "radial-gradient(circle, rgba(0,212,255,0.2) 0%, rgba(139,92,246,0.06) 50%, transparent 70%)",
            filter: "blur(4px)",
            inset: "50%",
            transform: "translate(-50%, -50%)",
            position: "absolute",
          }}
        />
      )}
      <Image
        src="/image/nexus-logo.png"
        alt="Nexus"
        width={size}
        height={size}
        className="relative object-contain"
        style={{
          filter: animated
            ? "drop-shadow(0 0 4px rgba(0,212,255,0.5)) drop-shadow(0 0 10px rgba(139,92,246,0.2))"
            : "drop-shadow(0 0 3px rgba(0,212,255,0.35))",
        }}
      />
    </div>
  );
}
