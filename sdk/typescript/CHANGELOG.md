# Changelog

All notable changes to the Oxy TypeScript SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.2.0] - 2026-07-09

Publishes the customer-app **Oxy Functions** platform and the **metric-tree /
anomalies** clients that accumulated on `main` since 2.1.0. All additive — no
public export was removed or renamed.

### Added

- **Oxy Functions** (oxy-internal #2521) — `useFunction(name)` for invoking
  server-side TypeScript handlers shipped in a bundle's `functions/` dir, and the
  `functions` block on the app manifest (`route`, `timeoutSeconds`, `cache`,
  `destinations`).
- **Scheduled functions + secret writes** (#2685) — `schedule` (cron) +
  `timezone`, `airwayStep`, and `secrets: { write }` on `OxyAppFunctionManifest`,
  each with manifest validation. A function can now run on a cron schedule and
  persist app-scoped secrets via `ctx.secrets.set`.
- **Metric tree + anomalies** (#2407) — `MetricTreeClient` and `AnomaliesClient`
  and their result types (metric nodes/edges, driver attribution, anomaly
  list/scan).
- **Result caching + function SSE** (#2634) — client-side query-result cache
  (dedup + TTL) and the streaming transport for function invocations.

## [2.1.0] - 2026-06-08

Adds engineer-tagged usage events to the SDK so a bundle can record
which features its users actually exercise. Pairs with the per-app
**Activity** tab in the Oxy admin UI (oxy-internal #2465 / §13 of
`internal-docs/customer-apps.md`).

### Added

- `useTrackEvent()` — fire-and-forget hook returning
  `(name: string, payload?: object) => void`. Batches every 1s and
  flushes on `pagehide` via `sendBeacon` so a click that fires
  immediately before a navigation isn't lost. Server-validated
  `event_name` regex (`^[a-z][a-z0-9-]{0,63}$`) + 4 KiB payload cap
  + 60-events-per-minute rate limit per (user, app).

  ```tsx
  import { useTrackEvent } from "@oxy-hq/sdk";

  const track = useTrackEvent();
  <button
    onClick={() => {
      track("export-clicked", { format: "csv", rowCount });
      doExport();
    }}
  >Export</button>
  ```

### Notes

- View events (page loads) are recorded automatically by the oxy
  backend on every HTML serve — no SDK code required. The SDK hook
  is only for engineer-tagged interactions inside the bundle.
- Dev-mode caveat: `sendBeacon` carries cookies but bypasses the
  `OxyAppProvider` fetcher wrapper, so the `OXY_TOKEN` bearer
  injected by the vite-plugin proxy in cross-origin `pnpm dev` is
  missing on these requests. Cookie-served prod is unaffected.

## [2.0.0] - 2026-05-29

Complete rewrite: `@oxy-hq/sdk` is now a **React-only, customer-app-only**
SDK. A bundle wraps its tree in `<OxyAppProvider>` and reads from its linked
oxy project through hooks; identity is resolved from `oxy-app.json` +
`window.__OXY_APP__` (injected by oxy at serve time), and requests are
authenticated by the session cookie (same-origin) or a bearer token
(cross-origin dev).

### Added

- `OxyAppProvider` — resolves app identity and provides it via context.
- Hooks: `useQuery` (inline SQL), `useSemanticQuery` (semantic layer),
  `useAgentRun` (agent chat over SSE), `useProcedureRun` (long-running
  procedures, beta).
- Drop-in components: `<OxyChat>` and `<OxyAnswer>` (markdown + SQL
  artifacts; URL-scheme allowlist guards against `javascript:` injection).
- `OxyApiError` structured error envelope.
- Pairs with `@oxy-hq/vite-plugin` (base path, manifest copy, dev shim) and
  `create-oxy-app` scaffolding.

### Removed (BREAKING)

- The entire v1 stack: `OxyClient` / `OxySDK` / `OxyProvider`, the
  Parquet/DuckDB-WASM reader, and postMessage-based auth. Apps now talk to
  `/api/projects/:id/*` exclusively.
- `listApps` / `getAppData` / `runApp` / `getDisplays` / `getFile` /
  `getFileUrl`.

## [0.1.0] - 2025-01-01

### Added

- Initial release of the Oxy TypeScript SDK
- Core `OxyClient` with methods for app data fetching
- Configuration management with environment variable support
- Parquet file reading with DuckDB-WASM integration
- `ParquetReader` class for SQL queries on Parquet data
- Helper functions for quick Parquet data access
- Full TypeScript type definitions
- Comprehensive examples for Node.js, React, and v0 integration
- Documentation and API reference

### Features

- `listApps()` - List all apps in a project
- `getAppData()` - Fetch app data with caching
- `runApp()` - Run app and get fresh data
- `getDisplays()` - Get display configurations
- `getFile()` - Fetch files from state directory
- `getFileUrl()` - Get direct file URLs
- Parquet reading and SQL querying capabilities
- Support for both CommonJS and ES modules
- Browser and Node.js compatibility