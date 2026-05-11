"use client";

// /trust — Trust Certificate verifier.
//
// Paste a Nexus run certificate JSON + the issuing deployment's
// public key (from its /.well-known/nexus-trust.json) and see
// whether the signature + Merkle root verify offline against the
// supplied key. The page is intentionally plain — it exists for
// enterprise / compliance consumers to prove a signed run wasn't
// tampered with, without talking to the originating server.
//
// Backend endpoints used:
//   POST /trust/verify                    — the actual check
//   GET  /.well-known/nexus-trust.json    — fetch this deployment's
//                                           public key for the "use
//                                           this server's key" button

import { useState } from "react";

type VerifyResponse = {
  valid: boolean;
  run_id: string;
  alg: string;
  key_id: string;
  merkle_root: string;
  leaf_count: number;
};

type VerifyError = { error: { message: string } };

export default function TrustVerifierPage() {
  const [cert, setCert] = useState("");
  const [pubKey, setPubKey] = useState("");
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<VerifyResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function loadLocalKey() {
    setError(null);
    try {
      const res = await fetch("/.well-known/nexus-trust.json");
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = (await res.json()) as { public_key?: string };
      if (!data.public_key) throw new Error("no public_key in response");
      setPubKey(data.public_key);
    } catch (e) {
      setError(
        `could not load local trust identity: ${
          e instanceof Error ? e.message : String(e)
        }`,
      );
    }
  }

  async function verify() {
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const certificate = JSON.parse(cert);
      const res = await fetch("/trust/verify", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ certificate, public_key: pubKey.trim() }),
      });
      const body = (await res.json()) as VerifyResponse | VerifyError;
      if (!res.ok || "error" in body) {
        throw new Error(
          "error" in body ? body.error.message : `HTTP ${res.status}`,
        );
      }
      setResult(body);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="min-h-screen bg-neutral-950 text-neutral-100 p-8">
      <div className="mx-auto max-w-3xl space-y-6">
        <header>
          <h1 className="text-2xl font-semibold tracking-tight">
            Trust Certificate Verifier
          </h1>
          <p className="mt-1 text-sm text-neutral-400">
            Paste any Nexus run certificate JSON and the issuing
            deployment&apos;s Ed25519 public key. Verification runs
            server-side — a reachable Nexus instance (any instance)
            just needs to cross-check the signature.
          </p>
        </header>

        <section className="space-y-2">
          <label htmlFor="cert" className="text-sm font-medium">
            Certificate JSON
          </label>
          <textarea
            id="cert"
            value={cert}
            onChange={(e) => setCert(e.target.value)}
            placeholder='{"run_id":"…","alg":"ed25519","key_id":"…","leaves":[…],"merkle_root":"…","signature":"…"}'
            rows={10}
            className="w-full rounded-lg border border-neutral-800 bg-neutral-900 p-3 font-mono text-xs text-neutral-100 focus:border-neutral-600 focus:outline-none"
            spellCheck={false}
          />
        </section>

        <section className="space-y-2">
          <div className="flex items-center justify-between">
            <label htmlFor="pk" className="text-sm font-medium">
              Public key (base64url, 32 bytes)
            </label>
            <button
              type="button"
              onClick={loadLocalKey}
              className="text-xs text-sky-400 hover:underline"
            >
              use this server&apos;s key
            </button>
          </div>
          <input
            id="pk"
            value={pubKey}
            onChange={(e) => setPubKey(e.target.value)}
            placeholder="kzqJx6k7uBy7XiSC1rZAUdQ6UGQ7kKRmN4h7T9tI9AI"
            className="w-full rounded-lg border border-neutral-800 bg-neutral-900 p-3 font-mono text-xs text-neutral-100 focus:border-neutral-600 focus:outline-none"
            spellCheck={false}
          />
        </section>

        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={verify}
            disabled={loading || !cert.trim() || !pubKey.trim()}
            className="rounded-lg bg-sky-500 px-4 py-2 text-sm font-medium text-neutral-950 disabled:opacity-50"
          >
            {loading ? "Verifying…" : "Verify"}
          </button>
          <button
            type="button"
            onClick={() => {
              setCert("");
              setPubKey("");
              setResult(null);
              setError(null);
            }}
            className="rounded-lg border border-neutral-800 px-4 py-2 text-sm text-neutral-300"
          >
            Clear
          </button>
        </div>

        {error && (
          <div className="rounded-lg border border-red-900/60 bg-red-950/40 p-4 text-sm text-red-200">
            {error}
          </div>
        )}

        {result && (
          <div
            className={`rounded-lg border p-4 text-sm ${
              result.valid
                ? "border-emerald-900/60 bg-emerald-950/30 text-emerald-200"
                : "border-amber-900/60 bg-amber-950/30 text-amber-200"
            }`}
          >
            <div className="text-base font-semibold">
              {result.valid ? "✓ Certificate is valid" : "✗ Certificate is NOT valid"}
            </div>
            <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 font-mono text-xs">
              <dt className="text-neutral-400">run_id</dt>
              <dd>{result.run_id}</dd>
              <dt className="text-neutral-400">alg</dt>
              <dd>{result.alg}</dd>
              <dt className="text-neutral-400">key_id</dt>
              <dd>{result.key_id}</dd>
              <dt className="text-neutral-400">merkle_root</dt>
              <dd className="break-all">{result.merkle_root}</dd>
              <dt className="text-neutral-400">leaves</dt>
              <dd>{result.leaf_count}</dd>
            </dl>
          </div>
        )}
      </div>
    </div>
  );
}
