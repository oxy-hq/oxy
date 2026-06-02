# @oxy-hq/sdk

React SDK for building **customer-app bundles** on the [Oxy](https://oxygen-hq.com)
platform. A bundle is a normal Vite + React app that reads from its linked oxy
project — raw SQL, the semantic layer, agents, and procedures — through a
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
| `useSemanticQuery({ topic, dimensions, measures, … })` | Semantic-layer query compiled by airlayer. |
| `useAgentRun({ agentId })` | `.ask(question)` starts an analytics agent run; streams events over SSE; `.cancel()`. |
| `useProcedureRun({ procedureId })` | Start a long-running procedure, poll, cancel (beta). |
| `<OxyChat agentId="…" />` | Drop-in chat UI over `useAgentRun`. |
| `<OxyAnswer … />` | Renders markdown + SQL artifacts + thread link. URL schemes are allowlisted (rejects `javascript:` etc.). |
| `OxyApiError` | Structured `{ message, code? }` server-error envelope. |

Hooks fail loudly if called outside `<OxyAppProvider>`. The default fetcher
sends `credentials: "include"` so same-origin (served-by-oxy) calls carry the
session cookie automatically.

## Docs

- Hands-on dev + deploy guide: `docs/local-development.md` in the
  [`oxy-hq/customer-apps`](https://github.com/oxy-hq/customer-apps) repo.
- SDK flow reference: `docs/sdk-flow.md` in that repo.
- Platform internals: `internal-docs/customer-apps.md` in oxygen-internal.

## License

MIT
