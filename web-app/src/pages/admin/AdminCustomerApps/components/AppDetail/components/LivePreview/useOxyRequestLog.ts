import { useCallback, useRef, useState } from "react";

/** One captured request the previewed app made to oxy's API. */
export interface OxyRequestEntry {
  id: number;
  method: string;
  /** Path + query (origin stripped) for display. */
  path: string;
  /** Full URL, for the row tooltip + detail view. */
  url: string;
  /** `null` while in flight. */
  status: number | null;
  /** Round-trip ms; `null` while in flight. */
  ms: number | null;
  ok: boolean | null;
  error?: string;
  at: number;
  /** Outgoing request headers (best-effort; lowercased for fetch). */
  reqHeaders?: Record<string, string>;
  /** Serialized request body; `null` when the request had none. */
  reqBody?: string | null;
  reqBodyTruncated?: boolean;
  /** Response headers, filled on completion. */
  resHeaders?: Record<string, string>;
  /** Response body text; filled asynchronously after headers for fetch. */
  resBody?: string | null;
  resBodyTruncated?: boolean;
}

const API_MARKER = "/api/";
const PATCH_FLAG = "__oxyReqLogPatched";
/** Cap stored bodies so a large download can't blow up React state. */
const MAX_BODY = 100_000;

/**
 * Capture the customer-app preview's requests to oxy by instrumenting the
 * (same-origin) iframe's `fetch` + `XMLHttpRequest` on each load. We only
 * record calls whose URL contains `/api/` (the oxy API surface) — asset
 * fetches are noise. Pure client-side: no SDK or server changes, works for
 * any previewed build.
 *
 * Each entry carries enough to inspect the call the way DevTools would:
 * request/response headers, request payload, and response body (cloned so the
 * app still consumes its own response). Query-string params are parsed from
 * `url` in the UI rather than stored.
 *
 * Limitation: SSE streams (agent runs use `EventSource`) aren't captured —
 * only request/response calls. Requests fired before the iframe `load` event
 * are also missed, but the SDK's data calls run in React effects post-load.
 */
export function useOxyRequestLog() {
  const [entries, setEntries] = useState<OxyRequestEntry[]>([]);
  const [available, setAvailable] = useState(true);
  const idRef = useRef(0);

  const clear = useCallback(() => setEntries([]), []);

  const start = useCallback((e: OxyRequestEntry) => {
    setEntries((prev) => [...prev, e]);
  }, []);
  const finish = useCallback((id: number, patch: Partial<OxyRequestEntry>) => {
    setEntries((prev) => prev.map((e) => (e.id === id ? { ...e, ...patch } : e)));
  }, []);

  /** `onLoad` handler for the preview iframe. (Re)patches its window and
   *  resets the log so each navigation starts clean. */
  const handleLoad = useCallback(
    (iframe: HTMLIFrameElement | null) => {
      if (!iframe) return;
      let win: (Window & typeof globalThis) | null = null;
      try {
        win = iframe.contentWindow as (Window & typeof globalThis) | null;
        // Touch a same-origin property to surface a cross-origin SecurityError
        // here rather than mid-request.
        void win?.location.href;
      } catch {
        setAvailable(false);
        return;
      }
      if (!win) return;
      setAvailable(true);
      setEntries([]);
      const nextId = () => {
        idRef.current += 1;
        return idRef.current;
      };
      instrumentFetch(win, { start, finish, nextId });
      instrumentXhr(win, { start, finish, nextId });
    },
    [start, finish]
  );

  return { entries, clear, handleLoad, available };
}

interface Sink {
  start: (e: OxyRequestEntry) => void;
  finish: (id: number, patch: Partial<OxyRequestEntry>) => void;
  nextId: () => number;
}

function isApiUrl(url: string): boolean {
  return url.includes(API_MARKER);
}

function toPath(url: string, base: string): string {
  try {
    const u = new URL(url, base);
    return u.pathname + u.search;
  } catch {
    return url;
  }
}

function clip(text: string): { body: string; truncated: boolean } {
  if (text.length <= MAX_BODY) return { body: text, truncated: false };
  return { body: text.slice(0, MAX_BODY), truncated: true };
}

function headersToRecord(h: Headers): Record<string, string> {
  const out: Record<string, string> = {};
  h.forEach((value, key) => {
    out[key] = value;
  });
  return out;
}

/** Parse the raw `\r\n`-joined string from `XMLHttpRequest.getAllResponseHeaders`. */
function parseRawHeaders(raw: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of raw.trim().split(/[\r\n]+/)) {
    const idx = line.indexOf(":");
    if (idx > 0) out[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
  }
  return out;
}

/** Merge a `Request`'s headers (if any) with the explicit `init.headers`. */
function mergeReqHeaders(input: RequestInfo | URL, init?: RequestInit): Record<string, string> {
  const merged = new Headers();
  if (input instanceof Request) {
    input.headers.forEach((v, k) => {
      merged.set(k, v);
    });
  }
  if (init?.headers) {
    new Headers(init.headers).forEach((v, k) => {
      merged.set(k, v);
    });
  }
  return headersToRecord(merged);
}

/** Best-effort, synchronous serialization of an outgoing body for display. */
function serializeReqBody(body: BodyInit | null | undefined): string | null {
  if (body == null) return null;
  if (typeof body === "string") return body;
  if (body instanceof URLSearchParams) return body.toString();
  if (body instanceof FormData) {
    const parts: string[] = [];
    body.forEach((value, key) => {
      parts.push(`${key}=${typeof value === "string" ? value : "(file)"}`);
    });
    return parts.join("&");
  }
  if (body instanceof Blob) return `(blob, ${body.size} bytes)`;
  if (body instanceof ArrayBuffer) return `(binary, ${body.byteLength} bytes)`;
  return "(stream)";
}

function instrumentFetch(win: Window & typeof globalThis, sink: Sink) {
  const orig = win.fetch;
  if (!orig || (orig as unknown as Record<string, unknown>)[PATCH_FLAG]) return;

  const patched: typeof fetch = async (input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.toString()
          : (input as Request).url;
    const method = (
      init?.method ||
      (typeof input === "object" && "method" in input ? (input as Request).method : "GET") ||
      "GET"
    ).toUpperCase();

    if (!isApiUrl(url)) return orig(input as RequestInfo, init);

    const id = sink.nextId();
    const t0 = performance.now();
    const reqBody = clipBody(serializeReqBody(init?.body));
    sink.start({
      id,
      method,
      path: toPath(url, win.location.href),
      url,
      status: null,
      ms: null,
      ok: null,
      at: Date.now(),
      reqHeaders: mergeReqHeaders(input, init),
      reqBody: reqBody.body,
      reqBodyTruncated: reqBody.truncated
    });
    try {
      const res = await orig(input as RequestInfo, init);
      sink.finish(id, {
        status: res.status,
        ok: res.ok,
        ms: Math.round(performance.now() - t0),
        resHeaders: headersToRecord(res.headers)
      });
      // Read a clone so the app still consumes its own body; fill in async.
      readResClone(res, (resBody, truncated) =>
        sink.finish(id, { resBody, resBodyTruncated: truncated })
      );
      return res;
    } catch (err) {
      sink.finish(id, {
        status: null,
        ok: false,
        ms: Math.round(performance.now() - t0),
        error: err instanceof Error ? err.message : "network error"
      });
      throw err;
    }
  };
  (patched as unknown as Record<string, unknown>)[PATCH_FLAG] = true;
  win.fetch = patched;
}

/** `null`-safe wrapper around `clip`. */
function clipBody(text: string | null): { body: string | null; truncated: boolean } {
  if (text == null) return { body: null, truncated: false };
  return clip(text);
}

function readResClone(res: Response, done: (body: string, truncated: boolean) => void) {
  try {
    res
      .clone()
      .text()
      .then((text) => {
        const { body, truncated } = clip(text);
        done(body, truncated);
      })
      .catch(() => {});
  } catch {
    // Body already disturbed / not cloneable — leave resBody undefined.
  }
}

interface XhrMeta {
  method: string;
  url: string;
  headers: Record<string, string>;
}

function instrumentXhr(win: Window & typeof globalThis, sink: Sink) {
  const proto = win.XMLHttpRequest?.prototype;
  if (!proto || (proto as unknown as Record<string, unknown>)[PATCH_FLAG]) return;
  const origOpen = proto.open;
  const origSetHeader = proto.setRequestHeader;
  const origSend = proto.send;

  proto.open = function (this: XMLHttpRequest, method: string, url: string, ...rest: unknown[]) {
    (this as unknown as Record<string, unknown>).__oxyReq = {
      method,
      url,
      headers: {}
    } satisfies XhrMeta;
    // @ts-expect-error variadic passthrough to the native signature
    return origOpen.call(this, method, url, ...rest);
  };
  proto.setRequestHeader = function (this: XMLHttpRequest, name: string, value: string) {
    const meta = (this as unknown as Record<string, unknown>).__oxyReq as XhrMeta | undefined;
    if (meta) meta.headers[name] = value;
    return origSetHeader.call(this, name, value);
  };
  proto.send = function (this: XMLHttpRequest, body?: Document | XMLHttpRequestBodyInit | null) {
    const meta = (this as unknown as Record<string, unknown>).__oxyReq as XhrMeta | undefined;
    if (meta && isApiUrl(meta.url)) {
      const id = sink.nextId();
      const t0 = performance.now();
      const reqBody = clipBody(serializeReqBody(body as BodyInit | null | undefined));
      sink.start({
        id,
        method: meta.method.toUpperCase(),
        path: toPath(meta.url, win.location.href),
        url: meta.url,
        status: null,
        ms: null,
        ok: null,
        at: Date.now(),
        reqHeaders: meta.headers,
        reqBody: reqBody.body,
        reqBodyTruncated: reqBody.truncated
      });
      this.addEventListener("loadend", () => {
        const res = readXhrResponse(this);
        sink.finish(id, {
          status: this.status || null,
          ok: this.status >= 200 && this.status < 400,
          ms: Math.round(performance.now() - t0),
          error: this.status === 0 ? "network error" : undefined,
          resHeaders: parseRawHeaders(this.getAllResponseHeaders()),
          resBody: res.body,
          resBodyTruncated: res.truncated
        });
      });
    }
    return origSend.call(this, body ?? null);
  };
  (proto as unknown as Record<string, unknown>)[PATCH_FLAG] = true;
}

function readXhrResponse(xhr: XMLHttpRequest): { body: string | null; truncated: boolean } {
  try {
    const type = xhr.responseType;
    if (type === "" || type === "text") return clip(xhr.responseText ?? "");
    if (type === "json") return clip(safeStringify(xhr.response));
    return { body: `(${type})`, truncated: false };
  } catch {
    return { body: null, truncated: false };
  }
}

function safeStringify(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}
