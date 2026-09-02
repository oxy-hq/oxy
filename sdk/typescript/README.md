# @oxy-hq/sdk

React SDK for building **custom-app bundles** on the [Oxy](https://oxygen-hq.com)
platform. A bundle is a normal Vite + React app that reads from its linked oxy
project — raw SQL, the semantic model, agents, and procedures — through a
small set of hooks, plus a couple of drop-in components.

> **v2 is a complete rewrite.** The v1 stack (`OxyClient` / `OxySDK`, the
> Parquet/DuckDB-WASM reader, postMessage auth) is gone. Bundles now talk to
> `/api/projects/:id/*` exclusively. See `CHANGELOG.md`.

## Install

```bash
pnpm add @oxy-hq/sdk @oxy-hq/vite-plugin
```

`react` (^19) is a peer dependency. `@oxy-hq/vite-plugin` wires the served
base path, copies `oxy-app.json` into the build, and injects the dev identity
shim — drop it into `vite.config.ts`:

```ts
import oxyApp from "@oxy-hq/vite-plugin";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({ plugins: [react(), oxyApp()] });
```

## Quick start

Wrap your tree in `<OxyAppProvider>` (it resolves the app's identity), then
read data with hooks:

```tsx
import { OxyAppProvider, useQuery, OxyChat } from "@oxy-hq/sdk";

function Dashboard() {
  const { rows, isLoading, error } = useQuery({
    sql: "SELECT Store, SUM(Weekly_Sales) AS sales FROM oxymart GROUP BY 1 ORDER BY 2 DESC LIMIT 5"
  });
  if (isLoading) return <p>Loading…</p>;
  if (error) return <p>{error.message}</p>;
  return (
    <>
      <table>{rows.map((r) => <tr key={r.Store}><td>{r.Store}</td><td>{r.sales}</td></tr>)}</table>
      <OxyChat agentId="analytics" />
    </>
  );
}

export function App() {
  return (
    <OxyAppProvider fallback={<p>Loading…</p>}>
      <Dashboard />
    </OxyAppProvider>
  );
}
```

## Identity (`oxy-app.json`)

Every bundle ships an identity-only manifest at its project root (next to
`vite.config.ts`, **not** under `public/`):

```json
{ "schemaVersion": 2, "slug": "store-pulse", "orgSlug": "acme", "name": "Store Pulse" }
```

When oxy serves the bundle it injects the authoritative identity as
`window.__OXY_APP__`; `OxyAppProvider` reads injection first and the manifest
second. There is **no API key in the bundle** — requests are authorized by the
viewer's oxy session (same-origin cookie) or, in cross-origin local dev, a
bearer token the dev proxy attaches. A bundle can't read data its viewer
couldn't already read.

## API

| Export | What it does |
| --- | --- |
| `OxyAppProvider` | Resolves identity, provides it via context. `fallback` renders while loading; `errorFallback` gets a structured error report. |
| `useQuery({ sql })` | Inline SQL → rows. `SELECT`/`WITH` only, 10k-row cap. |
| `useSemanticQuery({ topic, dimensions, measures, … })` | Semantic-model query compiled by airlayer. |
| `useAgentRun({ agentId })` | `.ask(question)` starts an analytics agent run; streams events over SSE; `.cancel()`. |
| `useProcedureRun({ procedureId })` | Start a long-running procedure, poll, cancel (beta). |
| `useFunction(name)` | `.invoke(body?)` runs a server-side **Oxy Function** (`functions/<name>.ts`) on oxy's isolate runtime; returns its JSON `Response`. For work the browser shouldn't do — warehouse writes, ELT, external APIs. |
| `<OxyChat agentId="…" />` | Drop-in chat UI over `useAgentRun`. |
| `<OxyAnswer … />` | Renders markdown + SQL artifacts + thread link. URL schemes are allowlisted (rejects `javascript:` etc.). |
| `OxyApiError` | Structured `{ message, code? }` server-error envelope. |

### World Model & analysis hooks

The same airlayer analyses the IDE's **World Model** and **Metric Tree** run,
exposed as hooks so a bundle can do RCA, opportunity sizing, and driver
exploration itself. Each fetches when enabled and its input is present; pass
`null` for a request/id to keep a hook idle until the user makes a selection.

| Export | What it does |
| --- | --- |
| `useWorldModel()` | The entity/measure graph — entities, their measures, and how measures promote across the hierarchy (edges). |
| `useWorldModelInstances(entityId, { search?, limit? })` | Searchable listing of an entity's instances (primary key + display label). |
| `useMetricTree({ root? })` | The metric tree (measures + component/driver edges), or the subtree at `root`. |
| `useSensitivity(measureId)` | Ranked **drivers** of a measure — "what moves this?" |
| `usePredict(changes)` | **What-if**: propagate hypothetical `(measure, delta)` changes upward (pure tree walk, no warehouse). |
| `useExplain(request)` | **RCA**: period-over-period root-cause decomposition. |
| `useOpportunity(request)` | Segment **opportunity sizing** — addressable upside vs a benchmark peer. |
| `useDistribution(request)` | Single-period distribution against an auto-derived prior baseline. |
| `useTimeDimensions()` | Valid time dimensions per view — the period axis for the ops above. |
| `useMeasureBreakdown(entityId, key, measure)` | Per-instance **driver tree** (SSE) — node values fill in as they resolve. |

```tsx
import { OxyAppProvider, useExplain, useOpportunity } from "@oxy-hq/sdk";

function RootCause() {
  // Pass `null` instead of the request object to defer until the user picks a period.
  const { data, loading, error } = useExplain({
    target: "financials.operating_profit",
    time_dimension: "financials.month",
    current_period: ["2025-09-01", "2025-09-30"],
    previous_period: ["2025-08-01", "2025-08-31"]
  });
  if (loading) return <p>Explaining…</p>;
  if (error) return <p>{error.message}</p>;
  return <p>Δ {data?.target_delta} — {((data?.coverage ?? 0) * 100).toFixed(0)}% explained</p>;
}

function Upside() {
  const { data } = useOpportunity({
    target: "orders.net_revenue",
    time_dimension: "orders.order_date",
    period: ["2025-04-01", "2025-06-30"]
  });
  return <>{data?.dimensions.map((d) => <p key={d.dimension}>{d.dimension}: +{d.total_upside}</p>)}</>;
}
```

A fuller worked example (graph + opportunity + RCA + streaming driver tree) is
in [examples/world-model-analysis.tsx](examples/world-model-analysis.tsx).

### Metric Tree (client-class)

For non-React / API-key callers, the programmatic `MetricTreeClient` and
`AnomaliesClient` (and all related types) are available from the package root
— see [metricTree.ts](src/metricTree.ts), [anomalies.ts](src/anomalies.ts), and
[examples/metric-tree.ts](examples/metric-tree.ts).

Hooks fail loudly if called outside `<OxyAppProvider>`. The default fetcher
sends `credentials: "include"` so same-origin (served-by-oxy) calls carry the
session cookie automatically.

### Workspace shell (`@oxy-hq/sdk/shell`)

The Oxygen workspace chrome — the 48px icon rail and universal top bar the
main web-app renders — as reusable components, so your app reads as part of
the same product. The main web-app consumes these exact components.

```tsx
import { OxyAppProvider } from "@oxy-hq/sdk";
import { OxyShell } from "@oxy-hq/sdk/shell";
import "@oxy-hq/sdk/shell.css";

export function App() {
  return (
    <OxyAppProvider>
      <OxyShell>
        <Dashboard />
      </OxyShell>
    </OxyAppProvider>
  );
}
```

`OxyShell` bootstraps from `GET /api/projects/:id/shell-context` (workspace
identity, sibling apps, host-aware navigation URLs) and degrades gracefully:
if the endpoint is unavailable (older server), your app renders unchromed.

| Export | What it does |
| --- | --- |
| `OxyShell` | Wired frame: rail + top bar + content column around your app. Slots: `topBarExtra`, `railBottom`, `hideTopBar`, `pageLabel`. |
| `useShellContext()` | The raw shell bootstrap payload (`{ data, loading, error }`). |
| `ShellRail`, `RailItem` | Presentational icon rail — props only, router-free. |
| `TopBar`, `Breadcrumb`, `SystemIndicator`, `WorkspaceClock` | Presentational top bar pieces. |
| `WorkspaceTile`, `OxyMark`, `OxygenFactoryMark` | Branding primitives. |
| `workspaceLogoUrl(apiBaseUrl, wsId, version?)` | Workspace logo endpoint URL builder. |

Styling: `shell.css` is namespaced (`oxy-shell-*`) — no Tailwind required, no
global styles leak into your app. It follows your design tokens when present
(`--sidebar-background`, `--foreground`, …) and falls back to the Oxygen
defaults. Dark mode: put a `.dark` class on any ancestor.

## Who is using the app

Two identity surfaces, and the difference between them is the difference between
a decision and a greeting.

**`ctx.user`, inside an Oxy Function — authoritative.** Assembled server-side per
invocation from the authenticated session, so nothing on it is client-supplied.
This is where a check that matters goes:

```ts
import type { OxyFunctionContext, OxyFunctionRequest } from "@oxy-hq/sdk";

export default async function exportAll(req: OxyFunctionRequest, ctx: OxyFunctionContext) {
  if (ctx.user.appRole !== "admin") {
    return Response.json({ error: "forbidden" }, { status: 403 });
  }
  return Response.json({ rows: await dump(ctx) });
}
```

| Field | Notes |
| --- | --- |
| `id`, `email`, `orgId` | Always present. `orgId` is the tenant boundary for anything you query. |
| `name`, `picture` | Display identity. Absent on schedule/Airway runs. |
| `appRole` | `"admin"` \| `"member"` \| absent. **The one to gate on** — an app grant (direct or via a team), with org-officer / Oxy-staff break-glass. Fails closed. |
| `orgRole` | `"owner"` \| `"admin"` \| `"member"` \| absent. Informational: explain ("ask your org admin"), label, route. Not a gate — org standing and app standing are different things. |
| `teams` | Org teams they belong to, name-sorted, scoped to this org. Descriptive — a team only grants anything through an app team grant, which `appRole` already reflects. |
| `kind` | `"user"` \| `"system"`. |

`teams` and `kind` are typed optional because a server older than 2026-08-21
doesn't send them — use `ctx.user.teams?.some(...)`. For `kind` there is no safe
inference on such a server (`=== "system"` misses a cron, `!== "user"` misfires
on a person), so if you support one, mark the schedule's configured `input`
instead of guessing.

**Background runs have no caller to attribute them to.** A schedule tick, an
Airway step, and an operator's manual *Run now* all run under the org owner's
`id` with `kind: "system"`, every caller field absent, and a synthetic
`schedule+<fn>@system.oxy` email — but `appRole` still reads `"admin"`, since
they carry owner authority. Note the manual case: a person did click, and there
is still nobody to reach, because the triggering operator isn't carried through
the job queue. A function wired to both a route and a background trigger must
branch on `kind`, not on the email:

```ts
if (ctx.user.kind === "system") return runRollup(ctx);   // no one to email
await ctx.email.send({ to: ctx.user.email, subject: `Hi ${ctx.user.name}`, html });
```

**`useShellContext()`, in the bundle — display only.** `data.user` is
`{ name, email, picture } | null`, and it deliberately carries no role at all.
Use it for an avatar or a greeting. Hiding a tab with it is fine; the endpoint
behind that tab is what actually has to say no.

## Docs

- Hands-on dev + deploy guide: `docs/local-development.md` in the
  [`oxy-hq/customer-apps`](https://github.com/oxy-hq/customer-apps) repo.
- SDK flow reference: `docs/sdk-flow.md` in that repo.
- Platform internals: `internal-docs/customer-apps.md` and
  `internal-docs/custom-apps-user-identity.md` in oxygen-internal.


## License

MIT
