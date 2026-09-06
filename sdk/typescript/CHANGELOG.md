# Changelog

All notable changes to the Oxy TypeScript SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **`ctx.user.email` is `string | null`.** It was typed `string`; the runtime
  has sent `null` for a frontline worker — a crew member enrolled by PIN, with
  no mailbox — since workers existed. Source-breaking for an app that hands
  `ctx.user.email` to something that wants a `string` (the `functions`
  scaffold's `notify.ts` did, and now answers 400 when there is nobody to send
  to). That null is the one field that tells the crew from the office inside a
  function.

## [2.10.0] - 2026-09-03

### Changed

- **A function's HTTP status now reaches the caller.** `readFunctionSseStream`
  throws a `FunctionStatusError` when a function returns a non-2xx, carrying
  `status`, the parsed `body`, and the logs captured before it. Previously the
  server hardcoded `{"status": 200}` in the terminal frame and dropped the
  isolate's status, so a handler answering 400, 403 or 409 resolved as an
  ordinary success and every caller had to infer rejection from the body's
  shape.

  **This is a behaviour change.** An app that reads `{ error: … }` out of a
  resolved value will now see a rejection instead:

  ```ts
  try {
    await invoke({ pathId });
  } catch (e) {
    const err = e as FunctionError;
    if (err.status === 409) { /* everyone already holds it */ }
  }
  ```

  Two things keep it from breaking apps that do not return a non-2xx: a missing
  or non-numeric status is treated as success, so an app talking to a server
  that predates this is unaffected; and the check is a 2xx range rather than
  `!== 200`, so 201 and 204 stay successes.

  Requires an oxy server carrying the matching change; against an older one the
  behaviour is unchanged.

### Added

- `FunctionError`, `FunctionLog` and `FunctionResult` are exported. They were
  the SSE reader's types and had never been re-exported, so a consumer could not
  name the value a `catch` gives them.

## [2.9.1] - 2026-08-21

Review corrections to 2.9.0, which had not shipped yet — folded in rather than
left as a footnote, since the wording they fix is the contract itself.

- **`kind: "system"` also covers a manual *Run now***, not just cron and Airway.
  2.9.0 documented `"system"` as "no human behind it"; an operator clicking Run
  now in the admin console is a human, and the job trigger deliberately routes it
  down the same owner-identity path. The field now means *no caller to attribute
  this to* — which is the honest claim, and the one a function can act on. The
  triggering operator is still unreachable; carrying them through the task
  payload is a follow-up on the server.
- `ctx.user.teams` is now sorted **case-insensitively**. Byte order put every
  lowercase team name after every capitalised one, which renders as unsorted.

## [2.9.0] - 2026-08-21

Fills out `ctx.user`. A function could previously answer "who is calling" with an
id, an email, and a per-app role — enough to gate on, not enough to greet, label,
or explain with, and with no way to tell a human apart from a cron tick.

- **`ctx.user.name` / `ctx.user.picture`** — display identity, server-side. The
  workaround was passing them up from the client, which is forgeable and so
  defeats the point of reading identity from `ctx` at all.
- **`ctx.user.orgRole`** (`"owner" | "admin" | "member"`) — their standing in the
  owning org, absent for a staff break-glass visitor. Informational by design:
  gate on `appRole`, use this to explain ("ask your org admin to connect a
  warehouse"), label, or route.
- **`ctx.user.teams`** — the org teams they belong to, name-sorted and scoped to
  this app's org. Descriptive: a team confers something on an app only through an
  app team grant, which `appRole` already folds in.
- **`ctx.user.kind`** (`"user" | "system"`) — the supported way to tell a routed
  invocation from a background one. `"system"` means *no caller to attribute
  this to*, which also covers an operator's manual **Run now**: those run under
  the org owner's id with every caller field absent, so they still read
  `appRole: "admin"`, and the triggering operator is not reachable. The
  email-sniffing check this replaces (`endsWith("@system.oxy")`) was never a
  contract.
- **Fixed: `ctx.user.orgId` was `undefined`.** The host serialized the field as
  `org_id` while these types and every doc said `orgId` — so a tenant filter
  written against it compared against nothing. The host now sends `orgId`, and
  the runtime keeps `org_id` populated so functions written against the shipped
  behaviour keep working. **If a function of yours filters SQL on the org id,
  re-read that line.**
- New exported types: `OxyOrgTeam`, `OxyIdentityKind`.
- `teams` and `kind` are **optional** on the type. A server older than this
  release doesn't send them, and a required type would have promised otherwise:
  `ctx.user.teams.some(...)` throws there, and `kind === "system"` silently reads
  `false` — routing a cron tick down the human branch, the exact bug `kind`
  exists to prevent. There is no safe inference for an absent `kind` in either
  direction; mark the schedule's `input` if you must support one.

Full contract, including what is deliberately withheld from apps:
`internal-docs/custom-apps-user-identity.md`.

## [2.8.0] - 2026-08-02

Makes email attachments actually reachable from an Oxy Function. `content` was
documented as base64, but the Functions isolate is bare `deno_core` — no
`deno_web`, and a V8 predating `Uint8Array.prototype.toBase64` — so `btoa` was
`undefined` and there was no way to produce that base64 at all.

- **`bytesToBase64` / `base64ToBytes`** — new named exports for binary. Plain
  bundled JS, so they behave identically in the isolate, in Node/vitest, and in
  a browser; and because they take bytes, `bytesToBase64(u8)` cannot be misread
  the way `btoa(u8)` is (the spec stringifies it to `"37,80,68,70"` and encodes
  that text instead of your file).
- `btoa` / `atob` now exist in the isolate (polyfilled in the runtime
  bootstrap), following WHATWG semantics so a helper unit-tested under Node
  behaves the same there. Passing bytes to `btoa` throws and points at
  `bytesToBase64`. These globals are declared for the functions tsconfig only —
  deliberately **not** added to this package's browser type graph, where `btoa`
  is the real DOM one with no polyfill behind it.
- `EmailAttachment.encoding` (`"base64"` default, or `"utf8"`) — attach
  generated text (CSV/JSON/HTML) with no encoder and byte-exact for non-ASCII.
- `ctx.fetch` takes `encoding: "base64"` (`OxyFetchInit`) and echoes it on the
  result. The default UTF-8 decode is lossy, so fetching a binary to attach
  previously returned a silently corrupt file.

## [2.7.0] - 2026-07-27

Completes the anomaly inbox for standalone custom apps. `scan()` and
`explain()` were already in the SDK but 404'd against `/external/api` —
the surface a bundle served from its own origin has to use — because only
the read half of the inbox was mounted there. Both verbs are now exposed,
so `client.anomalies.*` behaves the same from a custom app as from the IDE.

### Added

- **`explain(id, { refresh: true })`** — busts the cached `ExplainResult`
  and recomputes. The server already accepted `?refresh=true`; the option
  was simply unreachable from the SDK.
- **`ScanResponse.pending` and `ScanResponse.failures`** — a scan that
  outruns the 55 s synchronous window (or is debounced within 60 s of a
  previous one) returns `pending: true` with **all counts zeroed**. Those
  zeros are not a "nothing found" result; check `pending` before reading
  them, then refetch with `list()`. `failures` carries per-monitor errors.
- **`Anomaly.dimension_key` / `Anomaly.filters`** — the segment a row
  belongs to. Already on the wire, previously absent from the type, so
  per-segment anomalies couldn't be told apart from chain-wide ones.
- **Exported types** `AnomalyFilter`, `ScanFailure`, `ExplainOptions`.

### Changed

- **`Anomaly.filters`, `ScanResponse.pending` and `ScanResponse.failures`
  are required-nullable, not optional.** The server serializes all three
  unconditionally, so the previous `?` overstated absence — `filters` is
  `null` for a chain-wide monitor and `failures` is `[]` on a clean scan,
  but neither is ever missing. Reading code gets strictly better types;
  code that **constructs** these objects (test fixtures, mocks) must now
  supply the fields.

## [2.6.0] - 2026-07-23

Adds a custom-app **asset store**, **email attachments**, and a
**per-app role** on `ctx.user` — three pieces of one story: an app can
now accept a file, keep it, show it back, email it, and restrict who
sees any of that.

### Added

- **`ctx.storage`** — the app's asset store, covering both kinds of file
  an app produces, in one per-app silo:
  - **Uploaded**: `getUploadUrl()` mints a presigned PUT and the browser
    uploads **straight to S3**, so uploads aren't bounded by the request
    body limit and the bytes never pass through the function or oxy.
  - **Generated**: `put(pathname, body, { encoding: "base64" })` writes a
    file the function itself produced — binary (PDF, PNG, Parquet) is
    first-class, not text-only.

    Full surface: `getUploadUrl`, `getDownloadUrl`, `put`, `get`, `head`,
    `list`, `delete`, `copy`. Gated by new fail-closed
    `storage: { read, write }` capabilities in `oxy-app.json`.

    Notable defaults, and why: **`allowOverwrite` is false** (silently
    clobbering an asset is worse than an error — enforced atomically via
    an S3 conditional write, not a racy check-then-put); **`list` is
    cursor-paginated** (a silo with 100k assets must not become one
    unbounded walk); **download links can live up to 7 days**, SigV4's own
    limit, because a link emailed to a human outlives a 15-minute upload
    window. Every asset is private — reads are always presigned and
    time-boxed; there is no public-access mode by design.

    Keys are confined to the calling app: another app's key is rejected on
    every operation, not just on read.

- **Email attachments** — `ctx.email.send({ attachments: [...] })` with
  `filename`, base64 `content`, optional `contentType`, and `inline` +
  `contentId` for `cid:`-referenced images. Max 20 per send and 10 MiB
  decoded in total; past that, store the file with `ctx.storage` and email
  a presigned link instead (which is the better shape anyway, since the
  file is usually retained regardless).

- **`ctx.user.appRole`** — `"admin"`, `"member"`, or absent, derived
  server-side from per-app membership (with org-owner / Oxy-staff
  break-glass). This is what a privileged in-app surface should gate on:
  unlike a query param or a client-side flag, the client cannot forge it.
  Deliberately *not* the org role — an app admin administers one app
  without holding org-Admin, which also carries billing and member
  management.

## [2.5.0] - 2026-07-22

Universalizes the custom-app shell: a bundle served from any origin can
now drive the wired shell (shell-context, Ask Oxygen) and theme its chrome
to match the host app.

### Added

- **`backendUrl` prop on `OxyAppProvider`** — the SDK resolves its relative
  `/api/*` requests against this origin, so a cross-origin bundle reaches
  the Oxy backend without a same-origin proxy. Opt-in: when unset, the
  fetcher is unchanged.
- **`chromeBackground` / `chromeForeground` props on `OxyShell`** — theme
  the rail + top bar + AskDock by overriding the host tokens
  (`--sidebar-background`, `--foreground`, `--muted-foreground`), which
  every shell sub-scope re-derives from.
- **AskDock history panel** — shows title + relative time ordered by
  `created_at`, keeps and highlights the active chat, and adds a search box
  and a "Show more" control.

### Removed

- **The built-in Settings rail item.** The shell adds no built-in rail
  entries — a bundle that wants a Settings link supplies it via
  `railBottom`.

### Fixed

- `dev` / `build:watch` now emit `dist/shell.css` too (tsdown `onSuccess`
  plus a CSS watcher), so live CSS edits are reflected instead of going
  stale until the next full `build`.

## [2.4.0] - 2026-07-21

Ships the Oxygen workspace shell — the same 48px icon rail + universal
top bar the main web-app renders — as a reusable subpath export, so a
custom app reads as part of the HQ. The web-app consumes these exact
components (oxygen-internal `internal-docs/2026-07-06-sdk-shell-universalization-design.md`).

### Added

- `@oxy-hq/sdk/shell` entry point: wired `OxyShell` + `useShellContext`
  (bootstraps from the new bundle-gated `GET /api/projects/:id/shell-context`;
  degrades to chrome-less rendering on older servers), and presentational
  `ShellRail`/`RailItem`, `TopBar`, `Breadcrumb`, `SystemIndicator`,
  `WorkspaceClock`, `WorkspaceTile`, `ShellTooltip`, `OxyMark`,
  `OxygenFactoryMark`, `workspaceLogoUrl`.
- `@oxy-hq/sdk/shell.css` — namespaced (`oxy-shell-*`) stylesheet; no
  Tailwind required. Follows host design tokens when present, falls back
  to the Oxygen defaults; dark mode via a `.dark` ancestor class.
- `useOxyApp()` is now exported (low-level identity + fetcher access
  without requiring the manifest to be ready).
- **Reasoning trace in the Ask dock** — the same trace the main web-app
  renders: header meta (LLM calls · total time · steps), step rows with
  status, indented tool rows with input previews + durations, streamed
  thinking text, and query rows with row counts. Expanded while
  streaming, auto-collapses when the run settles. Exported standalone as
  `ReasoningTrace` + `buildTraceSteps`/`aggregateLlmStats`.
- **Interactive charts in the Ask dock** — `AnswerChart` renders
  `chart_rendered` blocks with **ECharts** (the same library the main
  web-app uses) when the host app has it installed: axis/item tooltips,
  hover, legend, resize. `echarts` is an optional peer (dynamic import);
  bundles without it fall back to the dependency-free SVG render. Table
  type always uses the SVG/HTML table.
- **Native charts in the Ask dock** — `chart_rendered` blocks from the
  analytics pipeline render as dependency-free SVG (bar/line/pie, table
  fallback) via the exported `AnswerChart`; the duplicate QUERY artifact
  block is gone from the dock (the trace + chart carry that info).
- **API-backed chat history** — the Ask dock's History now lists the
  viewer's persistent threads from the bundle-gated
  `GET /api/projects/:id/threads`, merged with the richer in-session
  conversations. Opening a server thread fetches
  `GET /api/projects/:id/threads/:tid` and rebuilds the transcript (trace,
  charts, answer) by replaying the thread's persisted run events through
  the same processor the live stream uses; follow-ups resume the thread.
  Exported: `useThreadHistory`, `fetchThreadTranscript`. Requires an oxy
  server that stamps `user_id` on bundle threads (same release); older
  servers fall back to session-only history.
- **New chat + history in the Ask dock header** — a `＋` action starts a
  fresh conversation (new server thread) and a clock action opens
  session-scoped history: chats started in this dock session, each
  restorable (follow-ups resume its server thread via the SSE resume).
  Bundles have no list-threads endpoint, so history is per-session, not
  persisted across reloads.
- **Ask Oxygen dock** in the shell: when the app manifest binds an agent
  (`ask.agent`), the top bar shows the Ask Oxygen button (⌘K/Ctrl+K
  toggles) and a right-side chat dock opens as a flex sibling —
  compacting the app, not covering it. Multi-turn over `useAgentRun`
  (follow-ups reuse the thread), suggested-question chips from
  `ask.suggestedQuestions`, streamed answers with SQL artifacts, and an
  "open in Oxygen" header link to the product Chat surface. Also
  exported standalone as `AskDock`.
- The shell rail's bottom cluster shows a **Settings** entry linking to
  the product's Unified Settings Dialog (via the SPA's new
  `?settings=<section>` deep link, returned as `links.settings` by
  shell-context; hidden on servers that don't send it).

### Fixed

- `OxyAnswer` / `OxyChat` follow the host's design tokens when rendered
  inside the shell scope (dark mode included) — hardcoded light-theme
  colors remain only as fallbacks for standalone bundles. The spinner's
  `oxy-spin` keyframes are now actually defined (injected once), so it
  spins.

### Changed

- New runtime dependency: `@radix-ui/react-tooltip` (rail tooltips).
- New optional peer: `react-dom` (only needed by the shell's tooltip
  portal; data-only consumers are unaffected).

## [2.3.0] - 2026-07-20

### Added

- **Customer-app email sending** (oxy-internal `feat/customer-app-email-send`) —
  Oxy Functions can send email via `ctx.email.send({ to, subject, html|text, ... })`,
  backed by AWS SES. The **platform controls the `from` address**; the function
  sets `replyTo` only. Gated by a new fail-closed `email: { send: true }`
  capability on `OxyAppFunctionManifest`.
- **`@oxy-hq/sdk/email`** — a new subpath export shipping
  `render(Component, props)` (preact-render-to-string) so functions can author
  email bodies as **preact** components and render them to HTML inside the
  Functions isolate. `preact` / `preact-render-to-string` are optional peer
  deps, so the main SDK bundle is unaffected. (React Email / react-dom can't run
  in the isolate: its node build needs `node:stream`, its browser build needs
  Web Streams the isolate lacks.)
- **`OxyFunctionContext`** — the server-side function `ctx` is now typed
  (`user`, `env`, `log`, `query`, `queryStream`, `fetch`, `warehouse`, `secrets`,
  `semantic`, `airway`, `email`), exported from the custom-app entry.

## [2.2.0] - 2026-07-09

Publishes the custom-app **Oxy Functions** platform and the **metric-tree /
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