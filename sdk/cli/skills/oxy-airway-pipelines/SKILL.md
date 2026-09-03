---
name: oxy-airway-pipelines
description: Use when writing, reviewing, or debugging an Oxy Airway ELT pipeline (`.airway.yml`) — sources, the `kind` values Oxy actually wires, credentials, reset and retry — or when one misbehaves: "unsupported source kind", a pipeline that parses then fails at run time, credentials that do not resolve, a *pipeline* run landing `completed_with_errors` (an *automation* run with that status is `oxy-automations`), a schema that will not change, or duplicate rows. Airway has no published docs; this skill is the reference.
---

# Oxy Airway pipelines

**Unlike every other Oxy subsystem, Airway has no published documentation** — no
`airway.mdx`, no `.airway.yml` reference page, no JSON schema. So this skill
carries content instead of pointing at a doc, which also makes it the
fastest-rotting thing in this set. Treat it as a map, not the territory.

## Version anchor, and what is actually authoritative

Everything below was read on 2026-08-17/18 against:

| | |
|---|---|
| `oxy-internal` workspace version | **0.5.126** (`Cargo.toml:522`) |
| `airway` engine dependency | git tag **0.1.30** (declared in `Cargo.toml`, the `airway = { git = …, tag = … }` line — `:509` today) resolving to rev `7c1737af9e69e0f9ad4e7a26a7969e4d1309538a` (the rev lives **only** in `Cargo.lock`, `[[package]] name = "airway"` — `:731` today; `Cargo.toml` never names it) |
| Real corpus | Poke House's 19 `oxy/pipelines/*.airway.yml` |

**Bumped since.** On 2026-08-19 `Cargo.lock` reads airway **0.1.32**
(`55e2794490ebcfffdf69a9693a6044efe313d702`). Everything below was read at
0.1.30 and wants re-checking against the live `source_factory.rs`. One error has
since been corrected in place (the `replacing` buffer's location, under
duplicate rows) — note that it was **not** caused by the bump: it was wrong at
0.1.30 too. A stale claim here is at least as likely to be a misreading as it is
to be drift; check which before blaming the pin.

Airway is an **external engine fetched by git tag**, not a vendored copy —
`crates/agentic/airway` is only Oxy's wiring around it. So a dependency bump can
change which `kind`s work, and how they behave, with no oxy-internal commit at
all. Re-check `Cargo.lock` before trusting any list here.

**Two files are authoritative. When they disagree with this skill, they win:**

- **`crates/agentic/airway/src/config.rs`** — the parser. Which keys exist,
  which are required, what the defaults are.
- **`crates/agentic/airway/src/source_factory.rs`** — the dispatch. Which
  `kind` values Oxy can actually build. (Changing that crate rather than using
  it? Read its `CLAUDE.md` first.)

**How the "no published doc" claim was established, so you can redo it:** the
live docs sitemap — <https://www.oxygen-hq.com/docs/sitemap.xml> — carries 146
URLs, **zero** mentioning airway, and `docs/build/data-infrastructure/` covers
three of the four similarly-named engines (Airform, airlayer, Airhouse) but not
this one. The single docs page whose URL says "pipeline",
<https://www.oxygen-hq.com/docs/guide/build/agents/pipeline>, is a same-word
false positive documenting the **agentic reasoning FSM**, not ELT. Do not send
anyone there for Airway.

## Anatomy

`AirwayPipelineSpec` (`config.rs`) is `#[serde(deny_unknown_fields)]`, so an
unrecognised top-level key is a **hard parse error**, not a silent no-op. That
is the friendly half; the traps below are the rest.

| Key | Required | Default | Notes |
|---|---|---|---|
| `name` | **yes** | — | validated non-empty |
| `source` | **yes** | — | `{kind: String, config: <free-form>}` |
| `destination` | **yes** | — | `{database, dataset_name, schema_separator?}` |
| `description` | no | — | free text; `>` block scalar is house style |
| `resources` | no | `[]` = all | subset of the source's advertised resources |
| `concurrency` | no | **`1`** | validated, capped at `MAX_CONCURRENCY = 16` |
| `allow_concurrent_runs` | no | `false` | single-flight; see run semantics below |
| `streaming` | no | `true` | falls back to the bulk path transparently |
| `channel_capacity` | no | `2 * concurrency` | streaming path only |

The corpus's smallest file (`quickbooks_financials.airway.yml`; IDs redacted and
comments stripped — the real file's comments cover `database:` resolving
credentials at run time via `airhouse_managed` ephemeral creds, plus commented
`resources:` / `streaming:` exemplars):

```yaml
name: quickbooks_financials
source:
  kind: quickbooks
  config:
    client_id: <literal>          # not a secret; see credentials below
    client_secret_var: QB_CLIENT_SECRET
    refresh_token_var: QB_REFRESH_TOKEN
    realm_id: "<literal>"
destination:
  database: pokehouse
  dataset_name: quickbooks_financials
concurrency: 1
```

**All 19 real files use that `destination: {database, dataset_name}` reference
form**, resolved by the `agentic-pipeline` executor at run time. `config.rs` also
accepts an `Inline{kind, config}` destination, but its own doc comment calls that
a test-fixture / post-resolution shape — no real file uses it. Field presence
across the 19: `name`/`source`/`destination` 19, `concurrency` 14, `description`
12, `schema_separator` 1, and `resources` / `allow_concurrent_runs` / `streaming`
/ `channel_capacity` **0** — those four are off the beaten path.

**The extension is `.airway.yml`, and it is load-bearing.** All 19 use it; none
uses a bare `.yml`. The compile-boundary walker
(`crates/oxy-compile/src/walker.rs:170`) globs literally `**/*.airway.yml` to
produce `FileKind::AirwayPipeline` — a `pipelines/foo.yml` is not a broken
pipeline, it is *invisible to compilation*, with no error to explain it.

## The wired `kind` set — the trap that costs the most time

**Oxy wires a strict subset of the engine's sources.** The upstream airway crate
registers roughly **40** source modules; `source_factory.rs` gives **14** of them
a concrete arm. YAML has no `kind` enum — `SourceConfig.kind` is a plain `String`
— so `kind: shopify` parses perfectly and only then fails at *run time*, in the
`other =>` fallback arm of the dispatch `match` inside
`source_factory.rs`'s `build_source_connector_inner`, with
``unsupported source kind `shopify`. Wire it up in
agentic_airway::source_factory::build_source_connector — …`` (cited by symbol,
not line: that arm is the *last* one in the table, so every source added above
it shifts it. Two did on 2026-08-17 alone — `ubereats` (#2937) and `netsuite`
(#2939).)

The 14 wired kinds, and how hard the real corpus exercises each:

| `kind` | Real pipelines using it |
|---|---|
| `http_file` | 5 (census LODES + gazetteer pulls) |
| `quickbooks` | 5 (see below — not one pattern) |
| `rest_api` | 3 (`census_acs`, `nces_schools`, `yelp_fusion`) |
| `clickhouse` | 2 (`clickhouse_ingest`, `clickhouse_mirror`) |
| `besttime` | 1 |
| `overpass` | 1 (`osm_augmentation`) |
| `overture` | 1 (`overture_places`) |
| `toast` | 1 (`restaurant_analytics`) |
| `filesystem`, `sql_database`, `postgres_cdc`, `weather`, `netsuite`, `ubereats` | 0 — wired, never exercised here |

Registered upstream but **not** wired here, so unavailable today: `shopify`,
`stripe`, `github`, `hubspot`, `salesforce`, `slack`, `jira`, `notion`,
`zendesk`, `google_ads`, `google_analytics`, `google_sheets`, `facebook_ads`,
`airtable`, `mongodb`, `kafka`, `kinesis`, and more. **Reading airway's own
source list and concluding a `kind` works here is the most expensive mistake
this file format offers.** Check the live `match` arms instead.

**`rest_api` is the generic escape hatch.** Per `source_factory.rs`'s own doc
comment, the vendor-specific helpers (`shopify`, `github`, `stripe`, …) are all
built on `RestApiSource` upstream, so most can be expressed directly as a
`rest_api` config; per-vendor sugar is deferred until a real consumer asks. A
source with no dedicated arm is a `rest_api` config, not a blocker.

Destinations are narrower still — `destination_factory.rs` wires exactly three:
`memory` (test fixture), `airhouse` (production; where all 19 real pipelines
land), and `postgres`.

### `quickbooks` is not a normal match arm

It is dispatched by an **early return before the dispatch table**
(`source_factory.rs:229`), ahead of the `match` at line 236, because it is the
one source needing plumbing the host must supply: the `RefreshTokenSink` and
`AccessTokenSource` traits (`source_factory.rs:61` / `:106`), threaded in as
`QuickBooksTokens`. Two mutually exclusive custody modes fall out of that, and
`build_quickbooks` **fails closed** if a config mixes them:

- **Rotating custody** — `client_secret_var` + `refresh_token_var`: the pipeline
  refreshes against Intuit itself and writes the new token back through the sink.
- **Read-only custody** — `access_token_var` alone: some *other* component owns
  rotation, and the value is re-read per request rather than substituted once,
  since a 60-minute token pinned at the start of a long backfill expires mid-run.

**Why that check exists, and why it matters far beyond this file:** Intuit voids
the old refresh token whenever it issues a new one, so two components rotating
the same grant deadlock into `invalid_grant` — each invalidating the other's —
until a human re-authorises by hand. Exactly one component may ever hold rotating
custody; `access_token_var` is how you keep a pipeline out of that role. The
source comment says it outright: a second refresher "would fork this grant's
rotation chain."

Hence the five real QuickBooks pipelines are **not one pattern**. The base
`quickbooks_financials.airway.yml` is rotating; the four per-location files
(`_clovis`, `_eastbay`, `_lakeoswego`, `_santarosa`) are read-only, fenced from
Intuit's token endpoint because a scheduled Oxy Function owns rotation. Copying
"the QuickBooks pipeline" as a template for a new company means first knowing
which custody mode that company's secret was provisioned under.

## Credentials live in the secret manager, never in the YAML

The convention is a `*_var` suffix naming a secret **key**, never a value;
`crates/agentic/pipeline/src/executor/mod.rs` (`resolve_airway_source_secrets`)
holds the per-kind `(field, var_key)` table.

- Flat configs. **The per-kind sets are disjoint — read them per row, never as
  a shared menu:**

  | `kind` | `*_var` keys it manages |
  |---|---|
  | `toast` | `client_secret_var`, `client_id_var` |
  | `quickbooks` | `client_secret_var`, `refresh_token_var` (plus `access_token_var` — see below, resolved differently) |
  | `netsuite` | `private_key_var` |
  | `clickhouse` | `password_var` |
  | `weather`, `besttime` | `api_key_var` |

  A `kind` with no arm in that `match` carries no managed credentials at all.
  Borrowing across rows fails, and not gently: put `client_id_var` on a
  quickbooks config and the executor leaves it in place (its quickbooks arm
  strips only the two it owns), so it reaches `QuickBooksParams` — which is
  itself `#[serde(deny_unknown_fields)]` and has no such field. Unlike the
  top-level `deny_unknown_fields` at anatomy above, this one bites when the
  connector is **built**, i.e. at run time: a spec that parsed cleanly dies on
  `invalid quickbooks config: unknown field`. Same for `refresh_token_var` on a
  toast config.
- **`rest_api` nests differently** — auth lives under `config.auth`, handled by
  a separate `resolve_rest_api_auth_secrets`, as `token_var` (bearer) or
  `key_var` (`api_key` header/query). Real, from `census_acs.airway.yml`:

  ```yaml
  auth:
    type: api_key_query
    key_var: CENSUS_API_KEY
    param: key
  ```

The executor resolves `*_var` → literal and then **strips the `_var` key**, so
the connector factory (and airway itself) only sees the resolved value under the
plain name — `client_secret_var` becomes `client_secret`. Hence **`password_var`
is not an accepted field on `ClickHouseParams`**, nor any other pair: the `_var`
form is valid *only* at the YAML layer.

## Run semantics

- **Partial failures are their own status.** A run that loaded some resources
  and failed others records as **`completed_with_errors`**, not as a failure.
  This is airway-engine behaviour Oxy inherits — it lives in the pinned engine
  (`src/pipeline/mod.rs:743,1316,2993`, `src/airstack/mod.rs:313,352`), not in
  the wiring crate — so a dashboard or alert that only matches "failed" reports
  it as clean success.
- **Schema migration is additive-only, structurally.** `schema/evolution.rs`
  (`diff_schemas` / `diff_tables`) emits only four changes: `TableAdded`,
  `ColumnAdded`, `VariantColumnCreated` (a type conflict creates a *new* variant
  column, leaving the old one alone), `WriteDispositionChanged` — no "column
  dropped", no "type changed in place". The enum cannot represent a narrowing
  change, so **a pipeline sitting on a wrong schema never self-heals, however
  many times you rerun it.**
- **Reset schema is the explicit escape**, and a real mechanism:
  `crates/agentic/airway/src/reset.rs` drops the pipeline's destination tables
  and deletes its `airway_pipeline_state` row, so the next run re-infers a fresh
  schema. Exposed as an HTTP route (`crates/agentic/http/src/routes/airway.rs`)
  and a UI button (`web-app/src/components/airway/ResetSchemaButton.tsx`).
  Destructive on purpose.
- **A retry resumes from a cursor; it does not re-pull history.** There are two
  cursors, and confusing them wastes time. The **pipeline-global** one is
  `airway_pipeline_state` (`state_store.rs`), keyed by `pipeline_name`: ordinary
  incremental runs advance it, ordinary retries reuse it, and Reset is the only
  sanctioned way to wipe it. Two runs racing that single row is half of why
  `allow_concurrent_runs` exists — see below for the other half. The **per-run**
  one is `airway_run_extensions.resume_state` (beside `retry_count`), used only
  by **backfill** runs; retry there is *reset-in-place*, re-driving the same
  `run_id` rather than cloning a new run (Toast:
  `with_resumable_backfill_window`).

### Duplicate rows in a `replacing` table

`write_disposition: replacing` never writes the served table directly — it
appends to a buffer that a merge-on-read fold rebuilds latest-wins, chosen to
avoid the O(target) `MERGE INTO` that OOMs the data plane (the
`WriteDispositionLabel::Replacing` doc comment in `source_factory.rs`). **That
fold runs twice over: inline at the end of every successful load, and again on
the scheduled airhouse vacuum, from identical SQL.** Either can be the one that
promotes a given row, which is why the failure below is about overlap rather
than about the vacuum alone.

**That doc comment names the buffer `<table>_raw`, and it is wrong** — the
buffer is a sibling *schema*, `<schema>_raw.<table>`. So was an earlier version
of this skill; the correction is not a version bump, since the migration
predates every airway rev oxy has pinned, including 0.1.30. `oxy-data-architecture`
carries the location, the stale comments, and where the buffer sits in the wider
picture. That shape is where duplicates come from:
each run ends with a merge-on-read fold, and **two folds whose snapshots overlap
each purge against a base the other has not committed yet, so both versions of a
changed row survive** (`extension/migration.rs:432`). Measured, not
hypothetical — pokehouse, 2026-08-05: 34 excess rows in `toast_pos.orders`, 104
in `order_selections`, **every duplicate pair spanning two `_aw_load_id`s**
(`migration.rs:435`). That span is the diagnostic, so check it *before* auditing
your own config: a pair straddling two load ids is this bug, not yours — and if
it doesn't straddle, this section does not explain your duplicates.

The guard is the single-flight lease `airway_pipeline_leases` — at most one
active run per `(workspace_id, pipeline_name)`, taken by one atomic
`INSERT … ON CONFLICT DO UPDATE … WHERE expires_at < now()` so the database
resolves the race rather than application code (`extension/pipeline_lease.rs:1`,
`:16`). A process-local mutex was rejected explicitly: Oxy runs multiple
`oxy-serve`/`oxy-worker` replicas, and the engine's own `COMPACTION_GATE` rests
on a single-writer assumption that stopped holding once the data plane scaled
past one replica (`pipeline_lease.rs:7`). Two consequences follow.
**`allow_concurrent_runs: true` turns the guard off** — `config.rs:44` says to
set it only on a pipeline owning no cursor and writing no `replacing` table,
since on a `replacing` pipeline it re-admits exactly this bug. And **a
dead-lettered run never releases its lease**: the 6-hour `LEASE_TTL_SECS` is a
crash backstop, not a reclaim policy, and the generic reaper cannot touch the
table without leaking a domain table into `agentic-runtime`
(`pipeline_lease.rs:73`, `:104`) — so a pipeline stuck "busy" with nothing
running is that gap, and wants the manual release, not a six-hour wait.

The lease stops *new* overlapping folds; being an admission guard, it cannot
repair rows that already landed. Those need Reset schema.

## Traps

**The `$schema` header several real pipelines carry is dead, and its failure is
silent.** Four of the 19 (`quickbooks_financials_clovis`, `_eastbay`,
`_lakeoswego`, `_santarosa`) open with a `# yaml-language-server: $schema=`
comment pointing at
`raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/pipeline.json`
(scheme omitted deliberately — the link is dead and this repo's link check fails
any non-200). Curled: **404**. `oxy-hq/oxy` was renamed to `oxy-hq/oxygen` and
the path 404s there too; that repo's `json-schemas/` holds 8 files and **no
`pipeline.json` — no schema for `.airway.yml` exists at all.** Start from a real
pipeline and you inherit an editor integration that validates nothing, warns
about nothing, and looks like it works. Delete the line. (The other 15 carry no
pointer at all, so IDE validation is inconsistent across the corpus even before
the link being dead.)

**Omitting `concurrency` means `1`, not "unlimited".** The default is forced
sequential and nothing announces it: the 5 files that omit it (the census
`http_file` pulls) run serially, while the 14 that set it use 2–4. A pipeline
that just forgets the key is not misconfigured — it is quietly slow.

**Two `clickhouse` source footguns, both silent.** (a) `ClickHouseParams`' own
doc comment admits that `port: 8443` with no `secure:` "silently uses plaintext
on a TLS port… left as-is deliberately" — tolerable only because the wizard emits
both keys together, and these are hand-edited files. State `secure:` explicitly
and match it to the port, as `clickhouse_mirror.airway.yml` does (`8123`/`false`).
(b) Socket timeouts belong in `settings:`, never a SQL `SETTINGS` clause:
ClickHouse fixes the response socket's timeout at request setup, *before* parsing
the query body, so an inline `SETTINGS http_send_timeout=…` is accepted and
ignored — exactly what someone reaches for when tuning a slow mirror.

**A `kind` claim is only as good as the pin it was checked against, and in-tree
comments do not self-update.** `overture_places.airway.yml` still carries a
header saying the connector has not landed and the airway rev must be pinned to
a build registering `kind: overture` — which at 0.1.30 it does, so the comment
has been stale for a while with nothing to flag it. Read that as the standing
warning about this whole document: verify against `source_factory.rs` at the rev
you are running.
