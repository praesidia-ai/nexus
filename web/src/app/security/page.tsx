"use client";

// /security — public red-team scorecard.
//
// Renders `public/security/scorecard.json`. The file is regenerated
// by nightly CI that runs AgentDojo + promptfoo red-team + garak
// against `nexus mcp serve` and aggregates the results. Because the
// output is a static JSON file committed alongside the code, the
// public claim always matches the current git SHA — no trust in a
// backend.

import { useEffect, useState } from "react";

interface ProbeResult {
  id: string;
  category?: string;
  passed: boolean;
  severity?: "info" | "low" | "medium" | "high" | "critical";
  note?: string;
}

interface SuiteResult {
  name: string;
  url: string;
  ran_at: string | null;
  passed: number;
  failed: number;
  pass_rate: number;
  probes: ProbeResult[];
}

interface Scorecard {
  ran_at: string;
  suite_version: string;
  summary: {
    total_probes: number;
    passed: number;
    failed: number;
    skipped: number;
    pass_rate: number;
  };
  suites: SuiteResult[];
  note?: string;
}

export default function SecurityPage() {
  const [data, setData] = useState<Scorecard | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    fetch("/security/scorecard.json")
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json() as Promise<Scorecard>;
      })
      .then(setData)
      .catch((e) => setErr(e instanceof Error ? e.message : String(e)));
  }, []);

  return (
    <div className="min-h-screen bg-neutral-950 text-neutral-100 p-8">
      <div className="mx-auto max-w-3xl space-y-6">
        <header>
          <h1 className="text-2xl font-semibold tracking-tight">
            Red-Team Scorecard
          </h1>
          <p className="mt-1 text-sm text-neutral-400">
            Nightly CI runs <code className="rounded bg-neutral-800 px-1 text-xs">agentdojo</code>
            , <code className="rounded bg-neutral-800 px-1 text-xs">promptfoo redteam</code>
            , and <code className="rounded bg-neutral-800 px-1 text-xs">garak</code>
            {" "}
            against a locally-spawned{" "}
            <code className="rounded bg-neutral-800 px-1 text-xs">nexus mcp serve</code>
            . The results land here as a static file —{" "}
            <code className="rounded bg-neutral-800 px-1 text-xs">
              web/public/security/scorecard.json
            </code>
            . Nothing is hidden; passing is verifiable at any git SHA.
          </p>
        </header>

        {err && (
          <div className="rounded-lg border border-red-900/60 bg-red-950/40 p-4 text-sm text-red-200">
            {err}
          </div>
        )}

        {data && (
          <>
            <section className="rounded-lg border border-neutral-800 bg-neutral-900/40 p-5">
              <div className="flex items-baseline justify-between">
                <h2 className="text-sm font-medium text-neutral-300">
                  Headline
                </h2>
                <span className="text-xs text-neutral-500">
                  suite {data.suite_version} · last run{" "}
                  {new Date(data.ran_at).toLocaleString()}
                </span>
              </div>
              <div className="mt-3 grid grid-cols-4 gap-4 text-sm">
                <Metric label="Probes" value={data.summary.total_probes} />
                <Metric
                  label="Passed"
                  value={data.summary.passed}
                  tone="emerald"
                />
                <Metric
                  label="Failed"
                  value={data.summary.failed}
                  tone={data.summary.failed > 0 ? "red" : "neutral"}
                />
                <Metric
                  label="Pass rate"
                  value={`${Math.round(data.summary.pass_rate * 100)}%`}
                  tone={data.summary.pass_rate >= 0.9 ? "emerald" : "amber"}
                />
              </div>
            </section>

            <section className="space-y-3">
              <h2 className="text-sm font-medium text-neutral-300">
                Per suite
              </h2>
              {data.suites.length === 0 && (
                <div className="rounded-lg border border-amber-900/60 bg-amber-950/30 p-3 text-xs text-amber-200">
                  No suites recorded yet.
                </div>
              )}
              {data.suites.map((suite) => (
                <SuiteBlock key={suite.name} suite={suite} />
              ))}
            </section>

            {data.note && (
              <p className="text-xs text-neutral-500">{data.note}</p>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function Metric({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string | number;
  tone?: "neutral" | "emerald" | "amber" | "red";
}) {
  const color =
    tone === "emerald"
      ? "text-emerald-300"
      : tone === "amber"
        ? "text-amber-300"
        : tone === "red"
          ? "text-red-300"
          : "text-neutral-200";
  return (
    <div>
      <div className="text-xs text-neutral-500">{label}</div>
      <div className={`font-mono text-lg ${color}`}>{value}</div>
    </div>
  );
}

function SuiteBlock({ suite }: { suite: SuiteResult }) {
  return (
    <article className="rounded-lg border border-neutral-800 bg-neutral-900/40 p-4">
      <div className="flex items-baseline justify-between">
        <h3 className="text-sm font-semibold">
          <a
            href={suite.url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-sky-300 hover:underline"
          >
            {suite.name}
          </a>
        </h3>
        <span className="text-xs text-neutral-500">
          {suite.ran_at
            ? new Date(suite.ran_at).toLocaleString()
            : "not yet run"}
        </span>
      </div>
      <div className="mt-2 flex items-center gap-4 text-xs text-neutral-400">
        <span>passed {suite.passed}</span>
        <span>failed {suite.failed}</span>
        <span>pass rate {Math.round(suite.pass_rate * 100)}%</span>
      </div>
      {suite.probes.length > 0 && (
        <ul className="mt-3 divide-y divide-neutral-800 text-xs">
          {suite.probes.slice(0, 8).map((p) => (
            <li key={p.id} className="flex items-center gap-3 py-1.5">
              <span
                className={
                  p.passed ? "text-emerald-400" : "text-red-400"
                }
              >
                {p.passed ? "✓" : "✗"}
              </span>
              <span className="font-mono text-neutral-300">{p.id}</span>
              {p.severity && (
                <span className="rounded bg-neutral-800 px-1 text-[10px] uppercase text-neutral-400">
                  {p.severity}
                </span>
              )}
              {p.note && (
                <span className="truncate text-neutral-500">{p.note}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </article>
  );
}
