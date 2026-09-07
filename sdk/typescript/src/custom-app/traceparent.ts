// A W3C `traceparent` minted in the browser, one per `useFunction().invoke()`.
//
// The server adopts an inbound `traceparent` as the parent of its request
// span, so a call that carries one lands in a trace whose id the page knows
// *before* the response arrives. That is what lets a failed invoke, or an
// uncaught rejection that followed it, name the exact server-side trace — the
// isolate, every `ctx.*` op, the function's own warnings — in the operator's
// HyperDX, with nothing but the id.
//
// This is not a tracing SDK: nothing is exported from the browser, no
// ingestion key ships in the bundle. Ids only.

/** `{ header, traceId }` for one outbound call. */
export interface Traceparent {
  /** The `traceparent` header value: `00-<trace>-<span>-01`. */
  header: string;
  /** 32 lowercase hex chars — what to paste into HyperDX. */
  traceId: string;
}

const HEX = "0123456789abcdef";

function randomHex(bytes: number): string {
  const buf = new Uint8Array(bytes);
  const c = (globalThis as { crypto?: { getRandomValues?: (a: Uint8Array) => Uint8Array } }).crypto;
  if (c && typeof c.getRandomValues === "function") {
    c.getRandomValues(buf);
  } else {
    // A non-secure fallback is fine here: these ids group spans, they gate
    // nothing.
    for (let i = 0; i < bytes; i++) buf[i] = Math.floor(Math.random() * 256);
  }
  let out = "";
  for (let i = 0; i < bytes; i++) out += HEX[buf[i] >> 4] + HEX[buf[i] & 15];
  return out;
}

/** Mint a fresh, sampled `traceparent`. All-zero ids are invalid per the spec;
 *  the loop guards the astronomically unlikely draw. */
export function newTraceparent(): Traceparent {
  let traceId = randomHex(16);
  while (/^0+$/.test(traceId)) traceId = randomHex(16);
  let spanId = randomHex(8);
  while (/^0+$/.test(spanId)) spanId = randomHex(8);
  return { header: `00-${traceId}-${spanId}-01`, traceId };
}

/**
 * Stamp the ids of a failed invoke onto whatever was thrown, so the app (and
 * the platform's error beacon, which reads `traceId`) can name the trace and
 * the server-minted request id. Non-objects are returned untouched.
 */
export function withInvocationIds<E>(err: E, traceId: string, requestId?: string | null): E {
  if (err && typeof err === "object") {
    const target = err as { traceId?: string; requestId?: string };
    if (!target.traceId) target.traceId = traceId;
    if (requestId && !target.requestId) target.requestId = requestId;
  }
  return err;
}
