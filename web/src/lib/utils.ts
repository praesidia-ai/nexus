import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Return the URL if it is an http(s) URL, otherwise null.
 * Use to guard `<a href>` / `<iframe src>` against `javascript:` or `data:` URLs
 * that come from backend/user-controlled data.
 */
export function safeHttpUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const u = new URL(url, typeof window !== "undefined" ? window.location.href : "http://localhost");
    if (u.protocol === "http:" || u.protocol === "https:") {
      return u.toString();
    }
    return null;
  } catch {
    return null;
  }
}

/**
 * Build the URL to use when opening a generated app's preview.
 *
 * Preference order:
 *   1. Explicit URL from the backend (emitted on the Complete SSE event).
 *   2. `{protocol}//{hostname}:{port}` using the current window origin — so the
 *      preview works whether the user is on localhost, a Tailscale host, or a
 *      production domain reverse-proxying the generated app.
 *   3. `http://localhost:{port}` as a last-resort fallback (SSR / test).
 *
 * Never hardcode `http://localhost:${port}` directly at call sites — a preview
 * URL that only works locally silently breaks every non-localhost deployment.
 */
/**
 * Copy text to the clipboard, falling back to a legacy `document.execCommand`
 * path when `navigator.clipboard` is unavailable (non-secure contexts: bare-IP
 * LAN deployments, `http://` tunnels, older browsers). Returns `true` on
 * success, `false` otherwise — never throws. Callers should always surface a
 * user-visible toast on `false`.
 */
export async function safeCopyToClipboard(text: string): Promise<boolean> {
  try {
    if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* fall through to legacy path */
  }
  if (typeof document === "undefined") return false;
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

export function buildAppPreviewUrl(
  port: number | null | undefined,
  explicitUrl?: string | null,
): string | null {
  const explicit = safeHttpUrl(explicitUrl);
  if (explicit) return explicit;
  if (!port || port <= 0) return null;
  if (typeof window !== "undefined" && window.location && window.location.hostname) {
    const protocol = window.location.protocol === "https:" ? "http:" : window.location.protocol;
    return `${protocol}//${window.location.hostname}:${port}`;
  }
  return `http://localhost:${port}`;
}
