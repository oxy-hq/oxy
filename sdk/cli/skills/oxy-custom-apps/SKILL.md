---
name: oxy-custom-apps
description: >-
  Use when building, debugging, reviewing, shipping, or verifying an Oxy
  custom-app bundle (a code-first React/Vite app served by Oxy at
  /customer-apps/<org>/<slug>/), including writing Oxy Functions, driving them
  from the UI, and checking a published app in the browser with the Playwright
  MCP server (no CI checks a published bundle). Covers the oxy-app.json
  contract, @oxy-hq/sdk and @oxy-hq/vite-plugin, where app data goes
  (ctx.airhouse, ctx.oltp, ctx.storage), pitfalls, performance guardrails, load
  speed and usage tracking (service worker, Activity tab, useTrackEvent), the
  self-serve oxyc publish ship flow, dev-<handle> sandboxes and workspace
  previews, how an AI agent gets its own short-lived credential (oxyc tokens
  create --sandbox-agent or --agent, never the engineer's oxyc login), and
  reading a deployment's live data with oxyc api and oxyc proxy (which replaced
  oxy api and the Rust oxy login, publish and proxy).
---

# Custom-app development

Custom-app bundles are arbitrary JS apps served by Oxy at
`https://<env>/customer-apps/<org>/<slug>/`. Oxy is a data gateway:
serves static assets, gates cookie auth, and proxies queries. The
bundle is opaque above the data plane — no per-app server config.

This file is the contract. The detail lives in `references/` — see
[Read next](#read-next) for which file answers what.

## The Oxy App Kit (use it)

Two small packages remove the footguns — default to them rather than wiring a
bundle by hand:

- **`@oxy-hq/sdk`** — the data layer (`useQuery`, `useSemanticQuery`,
  `useAgentRun`, `<OxyChat>`); see rule 2 and the `@oxy-hq/sdk` README.
- **`@oxy-hq/vite-plugin`** (`oxyApp()` in `vite.config.ts`) — bakes the asset
  **base path** from the manifest, copies `oxy-app.json` into `out/`, proxies
  `/api` in dev, and injects the `window.__OXY_APP__` shim. Dropping it
  re-introduces the #1 bug — asset 404s → blank dashboard (rule 3).

Scaffold both with `pnpm create @oxy-hq/oxy-app my-app` (templates `vite` — the
default — `functions`, `dashboard`, `single-store`; `-t functions` adds Oxy
Functions). The platform is kit-agnostic (any bundle with a correct base path
works), but hand-rolling means owning the base-path + dev-proxy wiring yourself.

## Three rules

1. **`oxy-app.json` (project root, not `public/`) is identity only.**
   Schema:

   ```json
   {
     "schemaVersion": 2,
     "slug": "url-slug",
     "orgSlug": "your-org",
     "name": "Display Name"
   }
   ```

   Required: `schemaVersion` (must be `2`), `slug` (non-empty).
   Strongly recommended: `orgSlug` (lets `@oxy-hq/vite-plugin` derive
   the asset base path automatically, so plain `pnpm build` produces
   a bundle ready to drop into the linked org with no env var).
   Optional: `name` (display), `projectId` (dev-time hint).

   At serve time, oxy injects the authoritative org + project from
   the admin row via `window.__OXY_APP__` — the manifest's `orgSlug`
   and `projectId` are advisory hints only, never enforcement.

   Do not invent `products`, `writers`, or `from:`-blocks. Those
   don't exist in v2 and the probe rejects them.

   File location: **`<root>/oxy-app.json`**, sibling to
   `vite.config.ts`. The vite-plugin reads it from there, and so does
   `oxyc publish` (only from the directory it runs in); legacy bundles
   that kept it under `public/` get neither the plugin's base-path
   derivation nor the publish's `slug` / `orgSlug`.

2. **All data flows through SDK hooks → oxy HTTP endpoints.** The
   surface is intentionally small:

   | Hook / component | Endpoint | When |
   | ---- | -------- | ---- |
   | `useQuery({sql})` | `POST /api/projects/:id/query` | Raw SQL, 10k row cap |
   | `useSemanticQuery({topic, …})` | `POST /api/projects/:id/semantic-query` | Reference topics/measures by name |
   | `useAgentRun({agentId})` | `POST /api/projects/:id/agents/:id/asks` + SSE | Streaming chat / Q&A |
   | `useProcedureRun({procedureId})` | `POST /api/projects/:id/procedures/:id/runs` ⚠️ beta | Long-running batch |
   | `useFunction(name)` | `POST /customer-apps/<org>/<slug>/fn/<name>` + SSE | Invoke a server-side **Oxy Function** (see below) |
   | `<OxyChat agentId="…"/>` | wraps `useAgentRun` | Drop-in chat UI |
   | `<OxyAnswer {...run}/>` | renders any run | Drop-in answer renderer |

   ```tsx
   import { OxyChat, useQuery } from "@oxy-hq/sdk";
   const { rows } = useQuery({
     sql: "SELECT col FROM t WHERE x = {{ params.x | sqlquote }}",
   }, { params: { x: someState } });
   // ...
   <OxyChat agentId="analytics" />
   ```

   See the `@oxy-hq/sdk` README for the full reference, including
   `OxyApiError` parsing and beta-surface caveats.

   **Server-side work → Oxy Functions, not the browser.** `useQuery` &
   friends run in the client; for work the browser shouldn't do
   (warehouse writes, ELT kick-offs, external API calls, secret-bearing
   logic) ship a **TypeScript handler under `functions/`** and invoke it
   with `useFunction(name).invoke(body?)`. Declare it in `oxy-app.json`:

   ```json
   { "schemaVersion": 2, "slug": "my-app",
     "functions": { "daily-rollup": { "route": true, "timeoutSeconds": 30 } } }
   ```

   ```ts
   // functions/daily-rollup.ts — runs on Oxy's isolate runtime
   export default async (req, ctx) => {
     const { rows } = await ctx.query("SELECT … FROM …");   // data-plane ctx
     return Response.json({ rows });
   };
   ```

   `ctx` bridges to the data plane: `ctx.query` / `ctx.queryStream`,
   `ctx.semantic.query`, `ctx.warehouse.query`,
   `ctx.warehouse.{insert,exec,upsert}` (to Airhouse, or to a customer
   warehouse the function names in `customerWarehouseWrites`), `ctx.airway.run`,
   `ctx.fetch` (SSRF-allowlisted), `ctx.env`, `ctx.user`, `ctx.log`.
   What the app STORES picks its surface by shape — facts through
   `ctx.airhouse`, records through `ctx.oltp`, files through
   `ctx.storage` — and customer warehouses are read-only. See
   **Where data goes** below before writing anything.
   `oxyc publish` bundles `functions/*.ts` with
   esbuild — no separate backend to stand up. Author-facing types:
   `OxyFunctionContext` / `OxyFunctionRequest` from `@oxy-hq/sdk`; a worked
   example ships in the `functions` template (`pnpm create @oxy-hq/oxy-app
   my-app -t functions`).

3. **`OXY_APP_BASE_PATH` must match the path the bundle is linked
   under.** `@oxy-hq/vite-plugin` resolves it from (in order):
   `OXY_APP_BASE_PATH` env → `orgSlug + slug` in `oxy-app.json` →
   `/`. `oxyc publish` sets that env to `/customer-apps/<org>/<slug>/`
   for the build automatically, and the vite-plugin derives the same
   from the manifest for `pnpm dev` / `pnpm build`. Mismatch → asset
   404s → blank dashboard.

## Where data goes (the shape of the data picks the store)

| The data | Store | In a function | In `oxy-app.json` |
| --- | --- | --- | --- |
| **Facts** — something happened and won't change: an order, a completed checklist, a delivery, a reading | The workspace's Airhouse, in your schema `app_<writer>` | `ctx.airhouse.append(table, rows)` · `ctx.airhouse.query(sql)` · `ctx.airhouse.exec(sql)` | function: `"airhouse": { "enabled": true }` · app: `"airhouseMigrations": { "dir": "airhouse-migrations" }` |
| **Records** your app edits — bookings, assignments, templates, counts | The org's OLTP Postgres, in your schema `app_<writer>` | `ctx.oltp.query/exec(sql, params)` · `ctx.oltp.tx(async (tx) => …)` | function: `"oltp": { "enabled": true }` · app: `"migrations": { "dir": "migrations" }` |
| **Files** — photos, PDFs, exports | Your app's storage silo | `ctx.storage` | function: `"storage": { "read": true, "write": true }` |
| **The customer's warehouse** — ClickHouse, Snowflake, BigQuery, their Postgres | Theirs | `ctx.query` · `ctx.warehouse.query(db, sql)` | read-only: a write is refused unless the function declares `"customerWarehouseWrites": { "<db>": "<why>" }` |

Three questions, in order: **Is it bytes?** → storage (keep the key in a record
or fact). **Does it change after it's written?** No → a fact; yes → a record;
both when you need history (the record says what is true now, the fact says
what changed). **Is it already in the customer's warehouse?** → read it there.

`<writer>` is your slug with `-` → `_` (`my-app` → `app_my_app`); a slug
with `_` can't back a schema. Both stores are written **as the app**, whoever
invoked the function, so a scheduled run writes like a click.

**Facts, in practice.** Airhouse is DuckLake: no `PRIMARY KEY`, `UNIQUE`,
indexes or foreign keys — publish refuses a migration carrying one. So:

- Give every fact the id its source assigned and a `recorded_at`. A retried
  function appends twice; readers keep one row per id:
  `QUALIFY row_number() OVER (PARTITION BY completion_id ORDER BY recorded_at DESC) = 1`.
- A correction is a new fact (`voided`, `amended`), not an `UPDATE`.
- Name tables `app_<writer>.<table>` in migrations and in `exec`/`query`
  (`ctx.airhouse.schema` has the name). `append` takes the bare table name.
- One statement per call, nothing bound: request data goes through `append`,
  never into `exec` SQL. `exec` runs no DDL — tables come from migrations.

**Records, in practice.** Bound parameters (`$1`, `$2`) always. Several
statements that must land together go in one `ctx.oltp.tx`. The store is
provisioned per org by an operator; until then calls fail naming that step.

**Not a store:** `ctx.secrets` holds credentials you rotate. A cursor, a
counter or a JSON blob is a record. `oxyc validate` warns about it, about
keys in Airhouse migrations, and lists every customer-warehouse write exception.

```ts
// A checklist is finished: the record changes, and a fact is appended.
async function completeChecklist(ctx, runId: string, storeGuid: string) {
  await ctx.oltp.tx(async (tx) => {
    await tx.exec("UPDATE checklist_runs SET status = 'done', completed_at = now() WHERE id = $1", [runId]);
    await tx.exec("DELETE FROM open_reminders WHERE run_id = $1", [runId]);
  });
  await ctx.airhouse.append("checklist_completions", [
    { completion_id: runId, store_guid: storeGuid, completed_by: ctx.user.id, recorded_at: new Date().toISOString() },
  ]);
}
```

## Telling people something

Name **people**; the platform reaches them. Don't build a recipient table, a
"sent" log or a Slack webhook helper:

```ts
await ctx.notify({
  to: { role: "Store Manager", location: storeId }, // or { userId } · { team }
  kind: "list_overdue",                             // declared in oxy-app.json
  title: "Opening list is 30 minutes late",
  link: `/lists/${listId}`,                         // a path inside the app
  email: true,                                      // optional; slack: true too
  idempotencyKey: `overdue:${occurrenceId}`,        // retry-safe
});
```

`oxy-app.json`: `"notify": { "kinds": { "list_overdue": {} } }` on the function
(`{ "critical": true }` for a kind people must not mute). Everyone lands in
their Oxygen inbox — frontline crew too, who have no email — and is pushed,
emailed or Slacked where they can be. Needs the org's OLTP store. Use
`ctx.email.send` only for addresses **outside** the org (a vendor, an
accountant), and give it an `idempotencyKey` in a scheduled function.

The app opens in its own tab, so show the inbox **in the app** — people who
live in it (crew on a kiosk) never see Oxygen's bell:

```tsx
import { NotificationsBell } from "@oxy-hq/sdk";

<NotificationsBell onNavigate={(link) => navigate(link)} /> // in the header
```

It lists this app's notifications (`scope="all"` for the whole inbox); a tap
hands the `link` you passed to `ctx.notify` to `onNavigate` (without it, the
page loads that path under the app's base). It renders nothing where there is
no inbox (signed out, an older server, `pnpm dev`). `useNotifications()` gives
`{ items, unread, markRead, markAllRead, refresh, … }` to draw your own. Theme
it with `--oxy-notifications-*` CSS variables. Push sign-up and mutes stay in
Oxygen's Settings → Notifications — don't build them. In tests,
`createTestInbox()` from `@oxy-hq/sdk/testing` is a fake inbox to pass as
`<OxyAppProvider fetcher={inbox.fetcher}>`.

## Four pitfalls

- **CORS errors are usually wrong-URL errors in disguise.** The SDK's
  base URL must be `${OXY_URL}/api`, not `OXY_URL` bare. The API
  router (which carries CORS headers) lives under `/api/*`. Hitting
  `/<projectId>/...` lands in the static fallback and the missing
  headers look like a CORS failure.

- **Cookie auth requires `credentials: "include"`.** The SDK's
  `useQuery` sets this. If you fetch directly, set it yourself or the
  `oxy_session` cookie won't be forwarded.

- **`projectId` in the manifest is advisory, not enforcement.** The
  SDK uses it to construct the query URL. Authorization is the user's
  cookie → user → org_member of the project's org, decided
  server-side. A bundle that ships a stolen `projectId` can't read
  data the user can't already read.

- **Cloud mode is the target.** Don't branch on local vs. cloud; don't
  rely on the nil UUID `00000000-...` as a default project; don't
  assume `OXY_STATE_DIR` is writable.

Two more that read as bugs — `useTrackEvent("oxy-…")` is refused (`400`,
the prefix is reserved), and a hovered-but-never-opened app records no view —
are in [references/performance-and-usage.md](references/performance-and-usage.md).

## Publishing

**These commands are a person's.** `oxyc login` is the engineer's own browser
login, and a publish to a channel or a promote is refused under a sandbox
agent token. As an agent, publish into your own sandbox
([references/sandbox-agent-loop.md](references/sandbox-agent-loop.md)) and
leave the ship step to your engineer.

Ship with `oxyc publish` (`npm i -g @oxy-hq/cli`) — **no CI step, no S3 sync,
no folder picker**. From the app directory:

```bash
oxyc login --env production    # once; browser flow, caches a token,
                               # and prints whether you're an app-admin
oxyc validate                           # data-placement and manifest checks first
oxyc publish --env production           # build → bundle functions → tar → upload (draft channel)
oxyc publish --env production --promote # …straight to live
```

- **Identity**, for each half, is `--app` / `--org`, then `OXY_APP` /
  `OXY_ORG`, then the manifest's `slug` + `orgSlug`, then an
  `apps/<org>/<app>/` working directory. `--org` takes a slug or a UUID; pass
  the slug whenever `oxyc` runs the build, because the asset base path is
  baked from what you pass. With no org from any of those, the build stops
  with `missing org: the app's base path needs it` (exit `2`): add
  `--org <org-slug>`, and `--app <slug>` when the directory has no manifest.
  The server upserts the app row — there is no separate create step.
- **First publish of a new app** needs `--project <workspace-uuid>` (unless
  the app was registered in the admin UI): the build-config lookup can't
  resolve a project until the app exists. A pinned workspace also supplies
  the org. Drop the flag after the first publish — it's auto-resolved from
  then on.
- **`--env` targets:** `local` → a local Oxy web-app Vite dev server `:5173`
  (it serves `/cli-auth` and proxies `/api` to oxy on `:3000`), `dev` →
  `aip.dev.oxy.tech`, `staging` → `aip.staging.oxy.tech`, `production` (the
  default) → `app.oxygen-hq.com`. A value with a `.`, `/` or `:` is read as a
  URL.
- **Function lint.** Before the build, each Oxy Function's source is checked
  for what the host would refuse at the first call: a `ctx.*` call whose
  capability the manifest lacks, a global the isolate does not have
  (`Buffer`, `TextEncoder`, `process`, …), a write outside `destinations`. A finding fails
  the publish (exit `1`); `--allow-function-lint` publishes past a false
  positive.
- `--dir out` skips the build and uploads a prebuilt directory (declared
  functions are still bundled into it); `--build-only` builds and bundles
  without a credential or an upload; `--prebuilt` (with `--dir`) uploads a
  directory whose functions are already bundled. Server warnings print on
  stderr; `--json` prints the result.
- `.env.local` / `.env` are loaded from the app directory and its parents
  (never overriding what is already set), so a stray `OXY_PROJECT` there
  retargets a publish.
- **Credential:** `OXY_TOKEN`, then `oxyc login`'s cache — or, in a GitHub
  Actions job with `id-token: write`, the job's OIDC identity.
  `oxyc init-ci` writes a workflow that publishes with no stored secret.
- Promote / roll back / unpublish from the oxy admin app-detail console;
  every publish is audited (who shipped each build, when).
- **What is live now:** `oxyc apps show <org>/<app>` prints the live build, the
  commit it was built from, a newer draft if there is one, and its health;
  `oxyc apps builds <org>/<app>` is the history. `oxyc apps drift <org>/<app>`
  lists the commits in your checkout that touch the app after the live build's
  commit — `unknown` there means it could not compare, not that nothing changed.

The build is not the check: after a publish, drive the page
([references/verifying-in-the-browser.md](references/verifying-in-the-browser.md)).

## Sandboxes — try a change on real data before shipping it

A sandbox (`dev-<handle>`, 1-12 lowercase letters/digits/hyphens, no leading,
trailing or double hyphen) is a named, short-lived environment of one app: its
own build pointer, storage silo, secrets and Airhouse sibling schema. Reads
hit production; most writes are isolated or *held* (recorded, not performed —
`ctx.oltp`/`ctx.warehouse` stay shared; a held write's `note` field in
`invocations held` names the manifest key that would make it land instead).
Schedules and webhooks do not fire in a sandbox: call the function. It starts
with no build, costs nothing idle, and is gone after 7 idle days or an
explicit delete — safe to use for every iteration that would otherwise touch
staging. Limit: 20 per app, counting ones still being deleted.

```bash
oxyc env create acme/store dev-x --env dev --json          # starts with no build
oxyc publish --env dev --app-env dev-x --json               # build + publish to it
oxyc env secret set acme/store KEY --app-env dev-x --env dev --value-env VAR  # optional: its own secret
oxyc fn call acme/store submit-order --app-env dev-x --env dev --data '{}' --json   # call it
oxyc checks run acme/store --app-env dev-x --env dev --json # run its checks
oxyc invocations list acme/store --app-env dev-x --env dev --json  # what ran; a row's id is `id`
oxyc invocations held acme/store <invocation-id> --env dev --json  # what it held, not wrote
oxyc logs acme/store --app-env dev-x --env dev --json       # its function logs
oxyc env delete acme/store dev-x --env dev --yes --wait --json     # done; tears its homes down
```

(`--data` has no short form — don't write `-d`.)

Iterate from `publish` as the code changes — each publish is a new build, and
the sandbox always serves the newest. A sandbox build is **never promoted**:
ship by publishing the same tree to staging, then promoting from the console.
Staff only (needs `develop_apps` + `manage_apps`) — or a sandbox agent token
such staff approved; a publish token is refused on every sandbox operation,
so CI cannot use one.

**An agent runs this loop on its own sandbox agent token**, never a person's
login, with `--token-env OXY_TOKEN` on every command — the mint, exit codes,
rules and what to hand back are in
[references/sandbox-agent-loop.md](references/sandbox-agent-loop.md). Read it
before an agent runs any `oxyc` command against a deployment.

**A sandbox isolates the app, not the workspace.** The semantic model,
automations and Airway pipelines it reads are `main`'s. To try a workspace
branch, open a workspace preview (staff only, `oxyc preview …`), or publish to
the sandbox with `--semantic-branch <branch>` — see
[references/sandbox-agent-loop.md](references/sandbox-agent-loop.md#a-sandbox-isolates-the-app-not-the-workspace).

## Read next

| File | Read it when |
| --- | --- |
| [references/sandbox-agent-loop.md](references/sandbox-agent-loop.md) | You are an AI agent about to touch a deployment: minting the sandbox agent token, the loop, exit codes, rules, what to hand back, when you cannot get a token, the MCP tools, workspace previews. |
| [references/verifying-in-the-browser.md](references/verifying-in-the-browser.md) | You changed something a person sees (`src/**`, or a function a card calls) and need to prove it on the published page. |
| [references/reading-data.md](references/reading-data.md) | You need a deployment's live data while building: an agent's own `--agent` token, `oxyc api` / `routes` / `schema`, and `oxyc proxy` for `pnpm dev` against cloud data. |
| [references/performance-and-usage.md](references/performance-and-usage.md) | You write an Oxy Function, a bulk action, caching or idempotency keys — or need to know what Oxy does for load speed and usage tracking, and the opt-outs. |
| [references/minimal-starter.md](references/minimal-starter.md) | You are writing an app by hand instead of scaffolding it, or need the smallest correct layout. |
