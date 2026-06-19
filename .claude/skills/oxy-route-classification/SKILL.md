---
name: oxy-route-classification
description: Use when adding, moving, or renaming an HTTP route under `crates/app/src/server/router/` (any `.route(...)` / `.nest(...)` mount), or writing a request handler that reads the workspace working copy, `.git`, or the local state dir (`config_manager.workspace_path()`, `ConfigManager::resolve_*`, `resolve_state_dir()`, `fs::read*(workspace_path…)`, a `glob` of the workspace dir, a `GitClient`). Also triggers on "add an endpoint", "new API route", "421", "x-oxy-required-role", "served on serve but the file isn't there", "role_manifest", "IdeOnly", "self-routing", and the HIGH-AVAILABILITY side: "FLEET_OK_READ_PATTERNS", "high availability", "thread/conversation won't load when the ide is down", "a read is pinned to the singleton", "viewing needs the factory". Every route that touches node-local disk MUST be classified IdeOnly or it 404s/421s on the stateless serve fleet — and conversely, every persisted-data READ must stay FleetOk (any replica) or HA hinges on one instance.
---

# Classify every workspace-FS route IdeOnly

Oxy runs as a split fleet (see `internal-docs/multi-instance-fleet.md`):

- **ide** — a singleton StatefulSet that owns the workspace working copy + `.git` + the local state dir.
- **serve** — a stateless fleet of replicas reading Postgres + S3 only. **No working copy.**
- **worker** — a TaskSpec drainer.

`crates/app/src/server/role_manifest.rs` is the single source of truth for which routes need the ide. `enforce_role` classifies every request: an `IdeOnly` route that lands on a serve replica is reverse-proxied to the ide; an **unlisted route defaults to `FleetOk` and is served locally on serve** — where the working copy doesn't exist.

So a new FS route you forget to classify doesn't fail at compile time — it 404s/500s at runtime, **only on the fleet**. That's exactly how `GET /apps/source/<file>` shipped: `get_source_file` reads `workspace_path()`, it wasn't in the manifest, and it returned `404 File not found` served by `serve@…` (no `x-oxy-forwarded-via`).

## The rule

**If a handler reads or writes node-local disk, its route MUST have an `IdeOnly` entry in `IDE_ONLY_PATTERNS` (role_manifest.rs).** Node-local disk =

- the **workspace working copy** — `config_manager.workspace_path()`, `ConfigManager::resolve_*`, `fs::read_to_string(workspace_path…)`, `glob::glob(workspace_path…)`;
- **`.git`** — any `GitClient` / shelling out to git;
- the **local state dir** — `resolve_state_dir()` (the serve env strips `OXY_STATE_DIR`).

If the handler reads only Postgres / S3 / the compile boundary / an LLM, leave it `FleetOk` (the default) — that's what keeps the fleet horizontally scalable. **When unsure, choose IdeOnly**: a wasted hop to the ide beats a 404 on the fleet.

## The recipe

1. Add a `ManifestEntry { method, path_pattern, role: RouteRole::IdeOnly }` to `IDE_ONLY_PATTERNS`. `path_pattern` is the FULL path including the `/api/{workspace_id}` prefix **and** the nest prefix. A route `/source/{pathb64}` mounted by `build_app_routes` (nested at `/apps`) → `/api/{workspace_id}/apps/source/{pathb64}`. `{name}` matches one segment, `{*name}` one-or-more, `method: "*"` any verb.
2. **Test it:**
   - Route in a **fully-FS builder** (`build_git_routes`, `build_file_routes`, `build_data_repo_routes`)? The `fully_fs_builder_routes_classify_ide_only` test already covers it — it parses the route paths from the router source, so a new route is checked automatically.
   - Route in a **mixed builder** (`build_app_routes` — `/run` is fleet-served)? Add an explicit per-route assertion (see `app_data_and_source_file_reads_are_ide_only`). The auto-derived test deliberately can't blanket-assert mixed builders, so this is on you + the reviewer.
3. **No external route table to touch.** The manifest IS the routing authority — a serve replica self-proxies an `IdeOnly` request to the Factory via `ide_proxy` (`OXY_IDE_UPSTREAM`). There is no ALB/Envoy `ideRoutes` list to keep in sync (that drift-prone table caused three outages and was removed). Add the manifest entry + the test; routing follows automatically.

## The other half: reads must stay HA — even under an IdeOnly wildcard

The split exists so that **reads scale and survive the ide going down**; only writes + live execution are pinned to the singleton. So the rule has a second half:

**A handler that reads only persisted data (Postgres / S3 / compile boundary) MUST be served on the fleet (`FleetOk`) — even when it sits under a broad `IdeOnly` `{*rest}` wildcard.** Otherwise *viewing* data needs the singleton and dies when it restarts — the HA bug this design exists to prevent, in reverse.

Some surfaces are pinned IdeOnly with a wildcard for *execution* safety — `/analytics/{*rest}`, `/agentic-workflows/{*rest}`, `/agentic-airway/{*rest}` (a live run executes in-process on the ide and touches the FS). That wildcard also swallows the **run-history READS** sitting next to the execution endpoints (`list_runs_by_thread`, `get_workflow_run`, `list_runs_for_pipeline`, …), which are pure `state.db` reads. Loading a thread is *data*, not a live run — pinning it to the ide made conversations unviewable whenever the ide restarted.

**Recipe — carve the read back out to FleetOk:** add a `ManifestEntry { method: "GET", path_pattern, role: RouteRole::FleetOk }` to **`FLEET_OK_READ_PATTERNS`** in `role_manifest.rs`. `classify` checks that list *before* `IDE_ONLY_PATTERNS`, so a FleetOk read wins over the IdeOnly wildcard.

**STRICT RULE (the const carries it too):** only carve out after verifying the handler touches **no** workspace FS / `.git` / local state dir and is **not** a live in-process stream — a pure Postgres / S3 read. Anything that executes, edits, or streams a live run stays `IdeOnly`. Mind the segment counts: the live `/runs/{id}/events` SSE has one MORE segment than a carved-out `/runs/{id}`, so it isn't shadowed — assert both directions in a test (see `agentic_run_history_reads_are_fleet_ok`).

**Mental model:** writes + execution + live streams → `IdeOnly` (the singleton). Reads of persisted data → `FleetOk` (any replica). If loading or *viewing* something needs the ide, that's an HA bug — find the read and carve it out. Full stateful-vs-HA matrix: `internal-docs/multi-instance-fleet.md` §4.

## What's NOT in scope

- A route that reads only the compile boundary / Postgres / S3 — `FleetOk` by design; do **not** pin it to the singleton. If it 404s on serve because the data isn't compiled, the fix is the compile boundary (`oxy-compile-boundary` skill), not IdeOnly. If it's a read buried under an IdeOnly wildcard, carve it out to `FLEET_OK_READ_PATTERNS` (above).
- Pure IDE-editor / git-write routes — already IdeOnly; nothing to do.

## Related

- `internal-docs/multi-instance-fleet.md` — the fleet guide (model, the stateful-vs-HA matrix, self-routing, `super_read_only`, code map).
- `oxy-compile-boundary` skill — when the right fix is "compile it to Postgres" instead of "pin it to the ide".
- `role_manifest.rs` module docs — the segment-by-segment matching semantics.
