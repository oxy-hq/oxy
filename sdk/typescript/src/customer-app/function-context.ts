// Author-facing types for **Oxy Functions** — the server-side TypeScript
// handlers bundled inside a customer app (`functions/<name>.ts`, declared in
// `oxy-app.json`). Each function is `export default async (req, ctx) => Response`.
//
// Until now functions were written untyped: the `ctx` object is assembled
// entirely by the Rust host (`__buildCtx` in
// `crates/app/src/server/api/customer_apps_functions/runtime.rs`) and never had
// a TypeScript counterpart. `OxyFunctionContext` below is that counterpart —
// it mirrors the host-provided `ctx.*` members one-for-one so function authors
// get autocomplete + type-checking. Import it as:
//
// ```ts
// import type { OxyFunctionContext, OxyFunctionRequest } from "@oxy-hq/sdk";
//
// export default async function notify(req: OxyFunctionRequest, ctx: OxyFunctionContext) {
//   const html = render(Welcome, { name });
//   const { messageId } = await ctx.email.send({ to, subject, html });
//   return Response.json({ ok: true, messageId });
// }
// ```

// ── Request ──────────────────────────────────────────────────────────────────

/**
 * The request passed as the first argument to a function's default export.
 *
 * The host hands the isolate the raw request body as a string (see
 * `req_json` in `runtime.rs`); parse it yourself, e.g.
 * `JSON.parse(req.body || "{}")`. This is intentionally *not* a full Web
 * `Request` — there is no `.json()` / headers object in v1.
 */
export interface OxyFunctionRequest {
  /** Raw request body as received (JSON string for a JSON POST). */
  body: string;
}

// ── ctx sub-APIs (mirror `__buildCtx`) ────────────────────────────────────────

/** A single row from a `ctx.query` / `ctx.queryStream` result. */
export type OxyFunctionRow = Record<string, unknown>;

/** Identity of the invoking user (route) or the system identity (schedule/airway). */
export interface OxyFunctionUser {
  id: string;
  email: string;
  orgId: string;
}

/** Result of a `ctx.fetch` call. */
export interface OxyFetchResult {
  status: number;
  body: string;
}

/** `ctx.warehouse.*` — writes to one of the app's configured destination databases. */
export interface OxyWarehouseApi {
  insert(database: string, table: string, rows: OxyFunctionRow[]): Promise<unknown>;
  exec(database: string, sql: string): Promise<unknown>;
  upsert(
    database: string,
    table: string,
    rows: OxyFunctionRow[],
    conflictColumns: string[]
  ): Promise<unknown>;
}

/** `ctx.secrets` — write app-scoped secrets (gated by the `secrets.write` capability). */
export interface OxySecretsApi {
  set(key: string, value: string): Promise<void>;
}

/** `ctx.semantic` — airlayer-compiled semantic queries (inherits the pre-agg fast path). */
export interface OxySemanticApi {
  query(spec: Record<string, unknown>): Promise<unknown>;
}

/** `ctx.airway` — seed/await an Airway ELT pipeline run. */
export interface OxyAirwayApi {
  run(pipelineRef: string, variables?: Record<string, unknown> | null): Promise<{ runId: string }>;
}

// ── Email ─────────────────────────────────────────────────────────────────────

/**
 * Input to `ctx.email.send`. Platform-injected: the sender mailbox (`from`) is
 * platform-controlled and **not** an accepted field — passing it is a typed
 * error. Provide `html` and/or `text` as the body (render a template to HTML
 * with `render` from `@oxy-hq/sdk/email`).
 */
export interface EmailSendInput {
  /** Recipient address(es). Required. */
  to: string | string[];
  /** CC address(es). */
  cc?: string | string[];
  /** BCC address(es). */
  bcc?: string | string[];
  /** Reply-To address — the only sender-identity field an author may set. */
  replyTo?: string;
  /** Subject line. Required. */
  subject: string;
  /** HTML body. Provide at least one of `html` / `text`. */
  html?: string;
  /** Plain-text body. Provide at least one of `html` / `text`. */
  text?: string;
  /**
   * Optional idempotency key (≤256 chars). Accepted and validated in v1 but a
   * no-op until the persisted idempotency table lands — adopt it now so
   * background (retried) sends become exactly-once once it does.
   */
  idempotencyKey?: string;
}

/** Result of a successful `ctx.email.send`. */
export interface EmailSendResult {
  /** Provider (SES) message id of the sent message. */
  messageId: string;
}

/** `ctx.email` — send email (gated by the `email.send` capability). */
export interface OxyEmailApi {
  send(input: EmailSendInput): Promise<EmailSendResult>;
}

// ── ctx ───────────────────────────────────────────────────────────────────────

/**
 * The data-plane context passed as the second argument to a function's default
 * export. Mirrors the host-assembled `ctx` (`__buildCtx` in `runtime.rs`);
 * every member is a host-provided async function bridged to a Rust backend.
 */
export interface OxyFunctionContext {
  /** Invoking user (route) or system identity (schedule/airway). */
  user: OxyFunctionUser;
  /** Read-only view of the app's configured secrets (project-scoped). */
  env: Record<string, string>;
  /** Structured per-invocation logging (captured + surfaced with the response). */
  log(...args: unknown[]): void;
  /** Read-only SQL (SELECT/WITH only), function-scoped row cap. Resolves to the rows. */
  query(sql: string): Promise<OxyFunctionRow[]>;
  /** Read-only SQL with a higher row cap, yielded to the caller in batches. */
  queryStream(
    sql: string,
    opts?: { batchSize?: number }
  ): AsyncGenerator<OxyFunctionRow[], void, unknown>;
  /** SSRF-allowlisted outbound HTTP with a response-size cap. */
  fetch(url: string, init?: RequestInit): Promise<OxyFetchResult>;
  warehouse: OxyWarehouseApi;
  secrets: OxySecretsApi;
  semantic: OxySemanticApi;
  airway: OxyAirwayApi;
  email: OxyEmailApi;
}

/** Signature of a function's default export: `export default async (req, ctx) => Response`. */
export type OxyFunctionHandler = (
  req: OxyFunctionRequest,
  ctx: OxyFunctionContext
) => Promise<Response> | Response;
