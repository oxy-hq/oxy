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
  /**
   * The caller's role **within this app**, derived server-side from app
   * membership (with org-owner / Oxy-staff break-glass). Absent when they hold
   * no membership.
   *
   * This is the value to gate a privileged surface on — it cannot be forged by
   * the client, unlike a query param or a client-side flag:
   *
   * ```ts
   * if (ctx.user.appRole !== "admin") {
   *   return Response.json({ error: "forbidden" }, { status: 403 });
   * }
   * ```
   *
   * Note it is deliberately NOT the org role: an app admin administers one app
   * without holding org-Admin (which also carries billing and member management).
   */
  appRole?: "admin" | "member";
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
  /**
   * Files to attach. Max 20 per send, and **10 MiB decoded in total** — SES
   * caps a whole message near 40 MB, so for anything larger store the file with
   * {@link OxyStorageApi} and email a presigned link instead of inlining it.
   */
  attachments?: EmailAttachment[];
}

/** One attachment on {@link EmailSendInput}. */
export interface EmailAttachment {
  /** Filename shown to the recipient. Required; path separators are stripped. */
  filename: string;
  /** File bytes, **base64-encoded** (the only way binary crosses the isolate boundary). */
  content: string;
  /** MIME type; defaults to `application/octet-stream`. */
  contentType?: string;
  /** Render inline (e.g. an image referenced as `cid:<contentId>`) instead of as a download. */
  inline?: boolean;
  /** Content-ID for an inline part, referenced from the HTML body as `cid:<contentId>`. */
  contentId?: string;
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

// ── Storage ───────────────────────────────────────────────────────────────────

/** Input to `ctx.storage.getUploadUrl`. */
export interface StorageUploadUrlInput {
  /**
   * Destination path inside the app's silo, e.g. `"uploads/q1-report.pdf"`.
   * Segments are sanitized server-side and cannot escape the silo. Omit to use
   * `filename`, which is placed under `uploads/`.
   */
  pathname?: string;
  /** Shorthand for `pathname: "uploads/<filename>"`. */
  filename?: string;
  /** MIME type; bound into the presigned PUT signature. Inferred when omitted. */
  contentType?: string;
  /**
   * Exact byte length of the upload, bound into the signature — S3 rejects a
   * body of any other size. Capped by the server's upload ceiling (100 MiB by
   * default).
   */
  contentLength: number;
  /** Presign lifetime in seconds (default 900; max 604800 — SigV4's own limit). */
  expiresInSeconds?: number;
}

/** A minted presigned upload. */
export interface StorageUploadUrl {
  /** Presigned PUT — the browser uploads the file bytes directly to this URL. */
  url: string;
  /**
   * The stored key. Record it (e.g. on a row in your warehouse) — it is how you
   * fetch, list or link to the asset later. A random suffix is added so two
   * people uploading `report.pdf` don't collide.
   */
  key: string;
  /** ISO-8601 expiry of the presigned URL. */
  expiresAt: string;
}

/** A minted presigned download. */
export interface StorageDownloadUrl {
  url: string;
  expiresAt: string;
}

/** One asset in the app's silo. */
export interface StorageObject {
  key: string;
  size: number;
  contentType?: string | null;
  /** ISO-8601. */
  lastModified?: string | null;
}

/** One page of {@link OxyStorageApi.list}. */
export interface StorageListPage {
  objects: StorageObject[];
  /** Pass back as `cursor` to fetch the next page; `null` when complete. */
  cursor: string | null;
  hasMore: boolean;
}

/** Options for {@link OxyStorageApi.put}. */
export interface StoragePutOptions {
  /** MIME type. Inferred from the pathname's extension when omitted. */
  contentType?: string;
  /**
   * How `body` is encoded. `"base64"` is what makes **binary** generated assets
   * (PDF, PNG, Parquet) possible — a UTF-8 string would corrupt them.
   */
  encoding?: "utf8" | "base64";
  /** Append a short random component before the extension to avoid collisions. */
  addRandomSuffix?: boolean;
  /**
   * Replace an existing asset at this path. Defaults to `false` — writing over
   * an asset by accident is worse than an error, so this is opt-in.
   */
  allowOverwrite?: boolean;
  /** `Cache-Control: max-age=<seconds>` stored on the object. */
  cacheControlMaxAge?: number;
}

/** Result of a `put` (and of `copy`). */
export interface StoragePutResult {
  key: string;
  size: number;
  contentType: string;
}

/**
 * `ctx.storage` — this app's **asset store**, covering both kinds of file an app
 * produces, in one silo (`customer-app-storage/<app_id>/`):
 *
 * - **Uploaded** — a human picks a file; `getUploadUrl` mints a presigned PUT and
 *   the browser uploads **straight to S3**, so uploads aren't bounded by the
 *   request-body limit and the bytes never pass through your function.
 * - **Generated** — your function produces the file (a rendered PDF, a CSV
 *   export, a chart PNG) and writes it with `put`, using
 *   `{ encoding: "base64" }` for binary.
 *
 * Gated by the fail-closed `storage.read` / `storage.write` capabilities in
 * `oxy-app.json`. Every asset is private; reads are always presigned and
 * time-boxed. Keys are confined to your app — another app's key is rejected.
 *
 * ```ts
 * // Uploaded: mint a URL, browser PUTs to it, then record `key`.
 * const { url, key } = await ctx.storage.getUploadUrl({
 *   filename: "q1-report.pdf", contentType: "application/pdf", contentLength: size,
 * });
 *
 * // Generated: write a CSV your function just built.
 * const { key } = await ctx.storage.put("generated/jan.csv", csv);
 *
 * // Either way: email a link that outlives the request.
 * const { url: link } = await ctx.storage.getDownloadUrl(key, {
 *   expiresInSeconds: 604800, download: true,
 * });
 * ```
 */
export interface OxyStorageApi {
  /** Mint a presigned PUT for a browser upload (requires `storage.write`). */
  getUploadUrl(input: StorageUploadUrlInput): Promise<StorageUploadUrl>;
  /**
   * Mint a presigned GET (requires `storage.read`). `download: true` forces a
   * save-as via `Content-Disposition`, which is what an emailed link wants.
   */
  getDownloadUrl(
    key: string,
    opts?: { expiresInSeconds?: number; download?: boolean }
  ): Promise<StorageDownloadUrl>;
  /**
   * Write a generated asset (requires `storage.write`). Capped at 6 MiB — for
   * anything larger, mint a presigned upload URL and stream to it.
   */
  put(pathname: string, body: string, opts?: StoragePutOptions): Promise<StoragePutResult>;
  /** Read an asset back; `null` when absent (requires `storage.read`). */
  get(
    key: string,
    opts?: { encoding?: "utf8" | "base64" }
  ): Promise<{ body: string; contentType: string | null; size: number; encoding: string } | null>;
  /** Metadata without the body; `null` when absent (requires `storage.read`). */
  head(key: string): Promise<StorageObject | null>;
  /**
   * One page of assets (requires `storage.read`). Paginated deliberately — pass
   * the returned `cursor` back to walk a large silo without loading it all.
   */
  list(opts?: { prefix?: string; limit?: number; cursor?: string }): Promise<StorageListPage>;
  /**
   * Delete one or many assets (requires `storage.write`). Idempotent — deleting
   * an absent key is a no-op success. `deleted` is the number of keys **accepted**
   * for deletion (an absent key counts too), not a count of keys that existed.
   */
  delete(keyOrKeys: string | string[]): Promise<{ deleted: number }>;
  /** Server-side copy within the app's silo (requires `storage.write`). */
  copy(
    fromKey: string,
    toPathname: string,
    opts?: { allowOverwrite?: boolean }
  ): Promise<StoragePutResult>;
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
  storage: OxyStorageApi;
}

/** Signature of a function's default export: `export default async (req, ctx) => Response`. */
export type OxyFunctionHandler = (
  req: OxyFunctionRequest,
  ctx: OxyFunctionContext
) => Promise<Response> | Response;
