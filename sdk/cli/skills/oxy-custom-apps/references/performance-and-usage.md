# Performance and usage

Two halves: what you keep in your own code (the guardrails), and what Oxy does
for every published app without any (load speed and usage tracking).

## Performance guardrails

Any app that ships an Oxy Function or drives a batch of them should keep these —
cheap up front, hard to retrofit.

**In an Oxy Function — count the round-trips.** Each isolate run is stateless and
pays a fresh network hop per `ctx.query` / `ctx.warehouse.*`, so serial *dependent*
calls dominate latency.
- Run independent data-plane calls with `Promise.all`, not sequential `await`
  (e.g. resolve reference data **and** provision tables concurrently — only the
  final step needs both).
- Fold a check into a query that already runs: a duplicate/existence guard is a
  scalar subquery on the main `SELECT`, not a second round-trip.
- `Promise.all` idempotent DDL (`CREATE TABLE IF NOT EXISTS`); better, keep
  provisioning off the hot path if a migration can own it.
- A "cheap" extra `ctx.query`/`ctx.log` is N× the cost across N invocations.

**Driving many function calls from the UI (bulk actions) — don't go serial.**
- Independent calls run with **bounded concurrency** (~3–4) via a worker-pool over a
  queue, not one-at-a-time — turns an O(N) wall-clock batch into a few waves.
- If the calls share mutable server state that races under concurrency (a rotating
  token, a lazily-created table), the best fix is to **remove the shared mutation**
  (e.g. a single-writer scheduled refresher owns the token; requests only read).
  When you can't, warm the state with one call first, then fan out.
- Don't refetch derived state (a status ledger) after *every* item — the progress UI
  is the live feedback; **throttle** the refetch (~1/s) and flush once at the end.
- Guard `setState` after the component may have unmounted mid-batch (`mounted` ref).

**Trust the server, not the client body.** Environment / tenant / which-secret
selection is derived server-side (a function secret, a server-side mapping table,
the authed identity), never from the request body — validate or override anything
the client sends that picks a target.

**Reuse the SDK's data-plane caching.** `useQuery` already dedups concurrent identical
queries and short-caches results — don't hand-roll a fetch that bypasses it. Mark
read-only functions `cache: { ttlSeconds }` in the manifest; **never** cache a
side-effecting one (it silently drops writes). `idempotencyKey` = a fresh UUID
per ATTEMPT (click), never a stable per-subject key: the server stores each
key's response indefinitely and replays it WITHOUT running the function, so a
`store:period` key silently breaks any post → compensate (delete) → post-again
cycle — the second post "succeeds" with the stale response and writes nothing.
The key shields one request from transport duplication; cross-attempt dedup
belongs in the function itself (a server-side guard over real state, e.g. a
posted-ledger row the compensating action clears).

**Watch-outs when optimizing (each surfaced in review):**
- **Clear UI running-state in `finally`.** An async batch driver must reset its
  `running`/loading flag (and do its final refetch) in a `finally`, not after the
  `await` — a stray throw otherwise leaves the bar stuck at "…" with the button
  disabled.
- **Parallelizing can move a side effect ahead of a gate.** Running an independent
  side-effecting call concurrently with a validation query means it can now fire
  even when validation would reject. Fine if the side effect is benign — just know
  you traded the ordering for the latency.
- **Additive migration for a scoped guard → backfill legacy rows to their TRUE value,
  not a constant.** When a dedup/uniqueness guard starts keying on a new column,
  `ADD COLUMN … DEFAULT <constant>` silently defeats the guard for pre-existing rows.
  Derive the real value from an existing column (e.g. a DocNumber prefix) so old rows
  stay correctly scoped.

## Load speed & usage tracking (automatic — know what you get, and the opt-outs)

**You do not wire load performance; Oxy does it at publish and serve time.**
Every published app gets, with no bundle code:

- **Preload hints.** At publish, Oxy parses your built `index.html` and emits
  `Link: rel=preload`/`modulepreload` on the shell, so the browser fetches your entry
  chunks while the HTML is still in flight. This is why the **base-path wiring in
  [rule 3](../SKILL.md#three-rules) is load-bearing twice over**: a bundle whose
  entry `<script>`/`<link>` tags are real, hashed, same-origin assets (what
  `@oxy-hq/vite-plugin` produces) gets hinted; one that inlines everything into a
  single HTML file has nothing to hint, which is fine but flat.
- **A service worker** (`__oxy/sw.js`) that precaches those entry assets and serves
  content-hashed files cache-first — a repeat load is near-instant and works offline.
- **Usage instrumentation** — SPA pageviews, Core Web Vitals, engagement time, and
  uncaught-error counts, posted to `__oxy/beacon` and shown on the app's Activity tab.
  This is *in addition to* `useTrackEvent` (your own named events), which is unaffected.

**The two opt-outs, in `oxy-app.json` (both default on — omit to keep them):**

```jsonc
{ "schemaVersion": 2, "slug": "…",
  "performance": { "serviceWorker": false },  // ships its own SW, or must never cache
  "analytics": false }                         // silences the platform's client events only
```

Typed as `OxyAppManifest["performance"]` / `["analytics"]` in `@oxy-hq/sdk`. `analytics:
false` stops only the *client* runtime — Oxy still records one view row per navigation
server-side, so the Activity tab never goes dark. Reach for these rarely: the defaults are
the fast, observable path.

### A tab left open across a publish (automatic — and what it can cost you)

Oxy's injected runtime also watches for a newer build. Every app gets this with no bundle
code, so an app that keeps work in memory has to know about it:

- **It notices a publish within ~5 minutes.** A visible tab polls `__oxy/build`, and checks
  again when it comes back to the foreground or online. When the live build differs, it
  fires a cancelable `oxy:update-available` event on `window`.
- **A broken lazy chunk after a publish reloads the page — in every mode except the
  platform's `off`.** A route the tab never opened asks for a chunk the new build no
  longer has; rather than white-screen, the runtime checks `__oxy/build` and, if the
  build moved, reloads onto it. That reload **discards in-memory state** unless the app
  holds `useBusyWhile` / `holdUpdates`, an input is focused, or it cancels the
  `oxy:chunk-stale` event.
- **Whether a tab reloads by itself is `performance.autoUpdate`.** The platform default is
  set by the operator (announce only, today). Set it per app:

```jsonc
{ "performance": { "autoUpdate": true } }   // reload at a safe moment: a navigation,
                                            // 2 idle minutes with nothing focused, or a
                                            // return after 10+ minutes away
{ "performance": { "autoUpdate": false } }  // never reload by itself; offer it with useAppUpdate
{ "performance": { "autoUpdate": "prompt" } } // shell shows "Update ready"; reloads at the
                                              // next safe moment if nobody taps it in 24 h
```

**The rule: wrap unsaved in-memory work.** A checklist half filled, a form not yet saved,
a write in flight — hold updates while it is dirty, and the runtime will not reload under
it (a held token older than 12 h is ignored, so a leak cannot pin a tab forever):

```tsx
import { useAppUpdate, useBusyWhile } from "@oxy-hq/sdk";

useBusyWhile(isDirty, "checklist not saved");   // or: const release = holdUpdates("…")

const { available, apply } = useAppUpdate();    // offer the update in your own UI
{available && <button onClick={() => apply()}>Update now</button>}
```

`useFunction` already holds updates for the life of each call. `UpdatePill` (from
`@oxy-hq/sdk/shell`) is the ready-made "Update ready" chip; `useKioskIdle` signs a kiosk
out after the device's idle timeout, which is also a full reload onto the live build.
An app's own `beforeunload` "leave this page?" prompt still applies. When the viewer
chooses to stay, the runtime does not reload onto that build by itself again, but
`apply()` and the chip still work.

**Two gotchas that read as bugs:**
- **`oxy-` is a reserved event-name prefix.** `useTrackEvent("oxy-…")` is refused with a
  400 — that namespace is the platform's own instrumentation. Name your events anything
  else. (It is a collision guard, not a security boundary: the beacon rides your app's own
  auth gate, so it is *your* rows that are protected from accidental merging, not made
  unforgeable.)
- **A prefetch is not a view.** A launcher may warm an app on hover; Oxy checks
  `Sec-Purpose`/`Purpose`/`X-moz` and does not record a view or start a session for a
  speculative load. So a hovered-but-never-opened app shows no phantom traffic.
