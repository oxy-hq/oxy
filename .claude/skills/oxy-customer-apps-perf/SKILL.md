---
name: oxy-customer-apps-perf
description: Use when adding or modifying a customer-app serving route (crates/app/src/server/api/customer_apps_serve.rs, the /customer-apps/{*path} handler in serve.rs) or a customer-app data endpoint (projects/query.rs, projects/semantic_query.rs). Encodes the serve-plane + data-plane performance guardrails so every customer app stays fast. Triggers on "customer-app cache", "Cache-Control", "ETag", "compression on customer-apps", "result cache", "project-scoped cache", or a new per-request read on the customer-app hot path.
---

# Customer-apps performance (serve + data plane)

The perf baseline every customer-app request must keep, established by PR #2634
(full rationale, threat model, and future options: `internal-docs/customer-apps-performance.md`).
When you touch the serve or data path, preserve these — they are cheap to keep and
expensive to retrofit under load.

## Serve plane — `customer_apps_serve.rs`, `cli/commands/serve.rs`

- **Content-hashed assets → immutable Cache-Control.** URLs under `assets/`
  (Vite / Astro / Rsbuild / SvelteKit) and `_next/static/` (Next) change only when
  their bytes change → `public, max-age=31536000, immutable`. HTML and
  unfingerprinted root files → `no-cache` so a new deploy is picked up. This is
  `cache_control_for`; a new hashed-asset dir means adding its prefix there. Without
  it, every chunk re-runs the full auth + membership walk in `serve_inner`.
- **HTML → weak ETag + `If-None-Match` 304.** Injected HTML is a transform, so use a
  weak `W/"…"` ETag over the *final* bytes (`etag_for` / `if_none_match` in
  `serve_from_s3_build`); a matching `If-None-Match` returns 304 and skips the
  re-inject.
- **Compression is SSE-safe.** `CompressionLayer` on `/customer-apps/{*path}` relies
  on `DefaultPredicate`, which skips `text/event-stream` and already-encoded bodies —
  so the `/fn` SSE stream and the v0 proxy are untouched. If you add a streaming
  response under this route, confirm it stays `text/event-stream` (or keep it off the
  layer); never compress a stream.

## Data plane — `projects/query.rs`, `projects/semantic_query.rs`, `projects/result_cache.rs`

- **Short-TTL result cache, keyed by `project_id` FIRST.** The cache key is
  `(project_id, namespace, database, sql)`. `project_id` is the multi-tenant
  isolation boundary — a shared process-global cache without it **leaks results
  across tenants**. `result_cache.rs` has a `miss_on_different_project_or_sql_or_db`
  test asserting exactly this; keep it. Namespace by endpoint (`query` vs `semantic`).
- **Read the cache AFTER auth.** The `result_cache::get` must sit *below*
  `check_customer_app_gates` in the handler, or a cached hit bypasses authorization.
  Cache only successful responses (never error bodies).
- **Honor `?refresh`.** Every cached endpoint parses `?refresh` (or `refresh=`) and
  bypasses the cache so callers can force-run.

## Litmus test before merging

A new serving route under `/customer-apps/**` with no `Cache-Control`/compression, or
a new cached data endpoint whose key does **not** start with `project_id`, is read
**before** the auth gate, or ignores `?refresh` — should be challenged through this
skill. This complements `oxy-route-classification` (which decides *where* the route
runs); this skill governs *how fast* it answers.
