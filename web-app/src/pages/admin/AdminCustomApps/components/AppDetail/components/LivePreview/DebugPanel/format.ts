import type { OxyRequestEntry } from "../useOxyRequestLog";

/** Wall-clock time of the request, 24h, for the row's leading column. */
export function fmtTime(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleTimeString([], { hour12: false });
}

/** Short status cell: numeric code, `ERR` on failure, `···` while in flight. */
export function statusLabel(e: OxyRequestEntry): string {
  if (e.status != null) return String(e.status);
  if (e.error) return "ERR";
  return "···";
}

/** Status colour: destructive for ≥400 / errors, pulsing grey while pending. */
export function statusTone(e: OxyRequestEntry): string {
  if (e.error || (e.status != null && e.status >= 500)) return "text-destructive";
  if (e.status != null && e.status >= 400) return "text-destructive/80";
  if (e.status == null) return "animate-pulse text-muted-foreground/50";
  return "text-muted-foreground";
}

/** Case-insensitive header lookup (fetch lowercases, XHR may not). */
export function getHeader(
  headers: Record<string, string> | undefined,
  name: string
): string | undefined {
  if (!headers) return undefined;
  const lower = name.toLowerCase();
  for (const [k, v] of Object.entries(headers)) {
    if (k.toLowerCase() === lower) return v;
  }
  return undefined;
}

/** Pretty-print a body as JSON when it parses (or the content-type says so),
 *  otherwise return it verbatim. */
export function prettyBody(body: string | null | undefined, contentType?: string): string {
  if (!body) return "";
  const looksJson = contentType?.includes("json") || /^\s*[[{]/.test(body);
  if (looksJson) {
    try {
      return JSON.stringify(JSON.parse(body), null, 2);
    } catch {
      // Fall through to raw.
    }
  }
  return body;
}
