"use client";

import { cn } from "@/lib/utils";

export interface RadarAxis {
  key: string;
  label: string;
  value: number; // 0-100
}

const DEFAULT_AXES: RadarAxis[] = [
  { key: "color_contrast", label: "Color & Contrast", value: 0 },
  { key: "spacing", label: "Spacing", value: 0 },
  { key: "typography", label: "Typography", value: 0 },
  { key: "accessibility", label: "Accessibility", value: 0 },
  { key: "content_quality", label: "Content", value: 0 },
  { key: "code_quality", label: "Code Quality", value: 0 },
];

interface Props {
  axes?: RadarAxis[];
  size?: number;
  fillColor?: string;
  strokeColor?: string;
  className?: string;
}

export function RadarChart({
  axes = DEFAULT_AXES,
  size = 240,
  fillColor = "rgba(249, 115, 22, 0.15)",
  strokeColor = "rgba(249, 115, 22, 0.8)",
  className,
}: Props) {
  const cx = size / 2;
  const cy = size / 2;
  const maxR = size / 2 - 30; // leave room for labels
  const n = axes.length;
  const angleStep = (2 * Math.PI) / n;

  // Get point on the radar for a given axis index and radius
  function getPoint(index: number, radius: number): [number, number] {
    const angle = angleStep * index - Math.PI / 2; // start from top
    return [cx + radius * Math.cos(angle), cy + radius * Math.sin(angle)];
  }

  // Grid rings
  const rings = [0.25, 0.5, 0.75, 1.0];

  // Data polygon points
  const dataPoints = axes.map((axis, i) => {
    const r = (axis.value / 100) * maxR;
    return getPoint(i, r);
  });
  const dataPath = dataPoints.map((p, i) => `${i === 0 ? "M" : "L"} ${p[0]} ${p[1]}`).join(" ") + " Z";

  return (
    <div className={cn("flex flex-col items-center", className)}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        {/* Grid rings */}
        {rings.map((ring) => {
          const points = Array.from({ length: n }, (_, i) => getPoint(i, maxR * ring));
          const path = points.map((p, i) => `${i === 0 ? "M" : "L"} ${p[0]} ${p[1]}`).join(" ") + " Z";
          return (
            <path
              key={ring}
              d={path}
              fill="none"
              stroke="currentColor"
              strokeWidth={0.5}
              className="text-white/[0.08]"
            />
          );
        })}

        {/* Axis lines */}
        {axes.map((_, i) => {
          const [x, y] = getPoint(i, maxR);
          return (
            <line
              key={i}
              x1={cx}
              y1={cy}
              x2={x}
              y2={y}
              stroke="currentColor"
              strokeWidth={0.5}
              className="text-white/[0.06]"
            />
          );
        })}

        {/* Data polygon */}
        <path
          d={dataPath}
          fill={fillColor}
          stroke={strokeColor}
          strokeWidth={1.5}
          strokeLinejoin="round"
        />

        {/* Data points */}
        {dataPoints.map(([x, y], i) => (
          <circle
            key={i}
            cx={x}
            cy={y}
            r={3}
            fill={strokeColor}
            className="drop-shadow-sm"
          />
        ))}

        {/* Axis labels */}
        {axes.map((axis, i) => {
          const [x, y] = getPoint(i, maxR + 18);
          return (
            <text
              key={axis.key}
              x={x}
              y={y}
              textAnchor="middle"
              dominantBaseline="central"
              className="fill-muted-foreground text-[10px]"
            >
              {axis.label}
            </text>
          );
        })}

        {/* Score values on points */}
        {dataPoints.map(([x, y], i) => {
          if (axes[i].value === 0) return null;
          // Offset the text slightly away from center
          const angle = angleStep * i - Math.PI / 2;
          const labelX = x + 12 * Math.cos(angle);
          const labelY = y + 12 * Math.sin(angle);
          return (
            <text
              key={`val-${i}`}
              x={labelX}
              y={labelY}
              textAnchor="middle"
              dominantBaseline="central"
              className="fill-foreground text-[9px] font-bold"
            >
              {Math.round(axes[i].value)}
            </text>
          );
        })}
      </svg>
    </div>
  );
}
