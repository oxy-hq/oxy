# Wiring Agentic Analytics into `crates/app`

This checklist tracks every layer that must be connected when wiring
`agentic-analytics` into the existing `oxy-app` Axum server (backed by
SeaORM + **PostgreSQL**). Each item is a concrete task; check it off when
the code compiles and behaves correctly.

**Key differences from a greenfield setup:**

- The app uses **PostgreSQL** only (via `OXY_DATABASE_URL`). There is no SQLite
  in the main server path. `PRAGMA journal_mode=WAL`, `DB_PATH`, `FRESH_DB=1`,
  and `sqlx-sqlite` features are **not applicable**.
- DB connections come from `oxy::database::client::establish_connection()` which
  returns a pooled `DatabaseConnection` via a global `OnceCell`. You do **not**
  open a new connection yourself.
- Migrations are run once at server start by calling `Migrator::up(&db, None)`
  in `run_database_migrations()` inside `crates/app/src/cli/commands/serve.rs`.
  The `agentic-db` crate has its own `Migrator` (SQLite-targeted), which must
  **not** be run directly — its migration logic must be ported into
  `crates/migration` instead.
- `agentic-http`'s `db.rs` currently calls `agentic_db::db::get_db()` (a global
  `OnceLock`) in every function. This pattern is **replaced**: all `db::*`
  functions will be refactored to accept an explicit `&DatabaseConnection`
  parameter so callers can pass the connection obtained from
  `oxy::database::client::establish_connection()`. This eliminates the
  `init_db` / `get_db` global entirely (see Section 2b).
- `tracing` and CORS are already initialised globally — no duplicate setup needed.
- Routes are served under `/api/…`; analytics is mounted inside the project
  scope at `/api/{project_id}/analytics/…` (see Section 5).

---

## 1. Crate Dependencies (`crates/app/Cargo.toml`)

- [ ] Add `agentic-http` to `crates/app/Cargo.toml`. There is **no** `duckdb`
      feature flag on `agentic-http` itself — it already unconditionally enables
      the `duckdb` feature on `agentic-analytics` in its own `Cargo.toml`:
      `toml
    agentic-http = { path = "../agentic/http" }
    `
- [ ] Do **not** add `sea-orm` with `sqlx-sqlite` to `oxy-app`; the workspace
      `sea-orm` dependency already targets PostgreSQL (`sqlx-postgres`,
      `runtime-tokio-rustls`).
- [ ] Update **`agentic-db/Cargo.toml`** to use the workspace `sea-orm`
      dependency (PostgreSQL + `runtime-tokio-rustls`) instead of its current
      `sqlx-sqlite` / `runtime-tokio-native-tls` features:
      `toml
    sea-orm = { workspace = true }
    sea-orm-migration = { workspace = true }
    `
- [ ] Update **`agentic-http/Cargo.toml`** similarly — it also pins its own
      `sea-orm` with SQLite features, and its `axum` is pinned to `"0.7"`
      while the workspace uses `"0.8"`. Switch both to workspace deps:
      `toml
    axum = { workspace = true }
    sea-orm = { workspace = true }
    `
      Also add a dependency on `oxy` (the core crate) so that
      `agentic-http/src/routes.rs` can import `establish_connection` and
      `ProjectManager`:
      `toml
    oxy = { path = "../../core" }
    `

---

## 2. Database Setup (PostgreSQL, not SQLite)

- [ ] **Do not** open a new database connection or set `DB_PATH`. The pool is
      already managed by `establish_connection()` in `oxy::database::client`.
- [ ] **Do not** execute any `PRAGMA` statements — PostgreSQL handles WAL internally.
- [ ] **Do not** call `agentic_db::db::init_db` — it is made obsolete by the
      refactor in Section 2b.
- [ ] The three agentic tables are created by migrations in `crates/migration`
      (see Section 2a below), not by `agentic_db::migration::Migrator`.

### 2a. Agentic Migrations in `crates/migration`

The `agentic-db` crate ships its own `Migrator` that targets SQLite and must
**not** be run against PostgreSQL directly. Instead, port the table-creation
logic into two new files in `crates/migration/src/` and register them in
`Migrator::migrations()`:

- [ ] Create `m20260317_000001_create_agentic_tables.rs` — creates
      `agentic_runs`, `agentic_run_events` (with unique index on
      `(run_id, seq)`), and `agentic_run_suspensions` with FK cascades.
      Use `.json_binary()` (maps to `JSONB` on PostgreSQL) for `payload`,
      `suggestions`, and `resume_data` columns.
- [ ] Create `m20260317_000002_rename_legacy_agentic_tables.rs` — renames
      old singular table names (`agentic_run` → `agentic_runs`, etc.) if they
      exist, for environments that ran an earlier version of `agentic-db`.
- [ ] Add both migrations to the `vec![…]` in `Migrator::migrations()` in
      `crates/migration/src/lib.rs`, after the last existing entry.
- [ ] **Do not** call `agentic_db::migration::Migrator::up()` anywhere.

### 2b. Refactor `agentic-http/src/db.rs` — Explicit `&DatabaseConnection`

Every helper in `db.rs` currently calls `agentic_db::db::get_db()` to obtain
the connection, coupling it to a global singleton. Replace all of them with an
explicit `db: &DatabaseConnection` first parameter, and remove the
`use agentic_db::db::get_db;` import.

- [ ] Change every public function signature in `agentic-http/src/db.rs`:
      ```rust
      // before
      pub async fn insert_run(run_id: &str, ...) -> Result<(), DbErr>

      // after
      pub async fn insert_run(db: &DatabaseConnection, run_id: &str, ...) -> Result<(), DbErr>
      ```
      Functions affected: `insert_run`, `update_run_done`, `update_run_failed`,
      `update_run_suspended`, `update_run_running`, `insert_event`,
      `batch_insert_events`, `get_events_after`, `upsert_suspension`,
      `get_suspension`.

- [ ] Replace every `get_db()` call inside those functions with the local `db`
      parameter.
- [ ] Remove `use agentic_db::db::get_db;` from `db.rs` — it is no longer used.
- [ ] In `agentic-http/src/routes.rs`, `create_run` already obtains `db` and
      passes it to `run_pipeline` as described in Section 3's combined signature
      snippet. The `db::insert_run(&db, ...)` call happens before the
      `tokio::spawn`, and `db` is moved into the spawn closure:
      `rust
    db::insert_run(&db, &run_id, &body.agent_id, &body.question).await;
    // ...
    tokio::spawn(async move {
        run_pipeline(state2, db, config, base_dir, run_id2, question, answer_rx).await;
    });
    `

- [ ] Update `run_pipeline` to accept `db: DatabaseConnection` and forward
      `&db` to every `db::*` call inside the function body and the bridge
      task closure. The bridge task closure must capture `db` by move (since it
      lives in its own `tokio::spawn`); clone it before spawning:
      `rust
  async fn run_pipeline(
      state: Arc<AgenticState>,
      db: DatabaseConnection,
      config: AgentConfig,
      // ...
  ) {
      // ...
      let db2 = db.clone(); // for the bridge task
      let bridge_handle = tokio::spawn(async move {
          // all db::batch_insert_events(&db2, ...) calls here
      });
      // all db::update_run_* / db::upsert_suspension calls use &db
  }
  `
- [ ] Update `stream_events` to call `establish_connection().await` and pass
      `&db` to `db::get_events_after`.
- [ ] `agentic_db::db` (`init_db` / `get_db`) is now dead code; it can be
      deleted from `agentic-db/src/db.rs` and removed from
      `agentic-db/src/lib.rs`.

---

## 3. Agent Config Loading

`agentic-http` loads agent configs lazily per request using
`AgentConfig::from_file(&configs_dir/{agent_id}.agentic.yml)`.
`AgentConfig` here is `agentic_analytics::config::AgentConfig` — a separate,
simpler type from `oxy::config::model::AgentConfig`.

The configs directory is **not** passed via env var. It is derived per-request
from `ProjectManager`, which is already injected into request extensions by
`project_middleware` when analytics routes are mounted inside the project scope
(see Section 5). Use `project_manager.config_manager.project_path()` to get the
project root — analytics agent YAMLs live in that same tree.

- [ ] Modify `agentic-http/src/routes.rs` `create_run` to extract both
      `ProjectManager` (for the project path) and the DB connection (Section 2b).
      The final combined signature:
      ```rust
      use oxy::adapters::project::manager::ProjectManager;
      use oxy::database::client::establish_connection;
      use axum::Extension;

      pub async fn create_run(
          State(state): State<Arc<AgenticState>>,
          Extension(project_manager): Extension<ProjectManager>,
          Json(body): Json<CreateRunRequest>,
      ) -> Response {
          let db = match establish_connection().await {
              Ok(c) => c,
              Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                                format!("db error: {e}")).into_response(),
          };
          let base_dir = project_manager.config_manager.project_path().to_path_buf();
          let config_path = base_dir.join(format!("{}.agentic.yml", body.agent_id));
          // … rest: db::insert_run(&db, ...), tokio::spawn(run_pipeline(state2, db, ...))
      }
      ```

- [ ] Remove `configs_dir: PathBuf` from `AgenticState` — it is no longer
      stored globally; each request derives it freshly from its own
      `ProjectManager`. Update `AgenticState::new()` accordingly.
- [ ] Ensure each analytics agent has a `{agent_id}.agentic.yml` file in the
      project root (alongside `config.yml`, agent YAMLs, etc.).
      Minimum required fields (note: `type: duckdb` requires the `duckdb`
      feature; `type: sqlite` is always available):
      `yaml
  databases:
    - type: duckdb            # or sqlite
      path: ./data/warehouse.duckdb
  llm:
    model: claude-opus-4-6
    max_tokens: 4096
  `
- [ ] `AgentConfig::from_file` is called per `POST /runs` request and returns
      a `400 BAD_REQUEST` on failure — no startup pre-validation needed.
- [ ] `config.build_solver(&base_dir)` inside `run_pipeline` should receive
      `configs_dir` (i.e. the project path) as an argument rather than reading
      from `state.configs_dir`. Pass it through from `create_run`.

---

## 4. Shared Axum State (`AgenticState`)

- [ ] `AgenticState` no longer holds `configs_dir` (removed in Section 3).
      Construct it with `AgenticState::new()` (no arguments) once in
      `api_router()` or `start_server_and_web_app()`. No `init_db` call is
      needed (that global was eliminated in Section 2b).
- [ ] Wrap it in `Arc<AgenticState>`.
- [ ] `agentic_http::router(state)` calls `.with_state(state)` internally and
      returns a `Router<()>`, so it can be directly nested into any
      `Router<AppState>` without state conflicts.

The three in-memory maps and their lifetimes:

| Field        | Purpose                                                 | Lifecycle                                                                                                  |
| ------------ | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `notifiers`  | `Notify` per run_id; SSE handlers park here             | inserted by `register`, removed by `deregister`                                                            |
| `answer_txs` | `mpsc::Sender<String>` for user answers                 | same                                                                                                       |
| `statuses`   | `RunStatus` cache (Running / Suspended / Done / Failed) | inserted by `register`, updated throughout, **not** removed on deregister (late SSE clients still read it) |

---

## 5. Router Mounting (`crates/app/src/server/router.rs`)

- [ ] Build the analytics sub-router:
      ```rust
      use std::sync::Arc;
      use agentic_http::{AgenticState, router as agentic_router};

      let agentic_state = Arc::new(AgenticState::new());
      let analytics_routes = agentic_router(agentic_state);
      ```

- [ ] Nest it inside the **project middleware scope** (inside
      `build_project_routes()`), because `create_run` needs the `ProjectManager`
      extension that `project_middleware` injects:
      `rust
  fn build_project_routes(agentic_state: Arc<AgenticState>) -> Router<AppState> {
      Router::new()
          // … existing routes …
          .nest("/analytics", agentic_router(agentic_state))
  }
  `
      The full path will be `/api/{project_id}/analytics/…`.
- [ ] Pass `agentic_state` down from `api_router()` to `build_project_routes()`
      and thread it through to the nest call. Since `agentic_router` consumes
      an `Arc` it is cheap to clone.
- [ ] **No separate CORS layer** — the global `CorsLayer` in `api_router` already
      covers all routes.
- [ ] Verify the three routes are reachable:
  - `POST /api/{project_id}/analytics/runs` — starts a pipeline run
  - `GET  /api/{project_id}/analytics/runs/:id/events` — SSE stream
  - `POST /api/{project_id}/analytics/runs/:id/answer` — delivers user answer on suspension

---

## 6. Pipeline Execution Flow

- [ ] Confirm `create_run` inserts the run row via `db::insert_run` **before**
      spawning the Tokio task so SSE clients that connect immediately can find
      the run in the DB.
- [ ] Verify `run_pipeline` spawns the event-bridge task first (mpsc receiver →
      batch PostgreSQL writes → `state.notify()`), then drives the `Orchestrator`.
- [ ] Ensure the bridge task uses a periodic 20 ms flush tick (`MissedTickBehavior::Skip`)
      plus immediate flush on terminal/`awaiting_input` events so streaming
      tokens appear smoothly.
- [ ] Confirm `Orchestrator::new(solver).with_handlers(build_analytics_handlers())`
      is called with the correct solver; `solver.with_events(event_stream.clone())`
      must be called so LLM token events are emitted.
- [ ] Verify the suspension loop: `Orchestrator::resume(resume_data, answer)` is
      called after the user's answer arrives on `answer_rx`.
- [ ] Ensure `drop(orchestrator)` is called explicitly before
      `bridge_handle.await` so the event channel closes and the bridge drains
      cleanly before `state.deregister()`.

---

## 7. SSE Stream (Event Deserialization → UI Blocks)

- [ ] Confirm `sse::deserialize(event_type, payload)` covers all emitted
      `CoreEvent` variants and all `AnalyticsEvent` variants; add a branch for
      any new events that need to reach the frontend.
- [ ] Verify `sse::serialize_ui_block(block)` produces correct `event` / `data`
      pairs that the frontend can parse.
- [ ] Check `sse::is_terminal(event_type)` returns `true` for the events that
      close the stream (`error`, `done`, `answer`, …); add new terminal types
      here when the domain grows.
- [ ] Validate that `UiTransformState::new(analytics_step_label)` is constructed
      **per SSE connection** (not shared), so replaying events from `seq=0`
      on reconnect rebuilds the correct UI state.
- [ ] Confirm the `Last-Event-ID` header from the client is parsed and passed as
      `last_sent_seq`, enabling seamless reconnection without re-sending seen
      events.

---

## 8. SeaORM Entities Used by `agentic-http`

These entities in `agentic-db` target the three tables created by the
`crates/migration` additions above. They work transparently with PostgreSQL
since `Json` in sea-orm maps to `JSONB`.

- [ ] `agentic_run` — every run lifecycle helper (`insert_run`, `update_run_done`,
      `update_run_failed`, `update_run_suspended`, `update_run_running`).
- [ ] `agentic_run_event` — `insert_event` (single) and `batch_insert_events`
      (bulk transaction).
- [ ] `agentic_run_suspension` — `upsert_suspension` / `get_suspension`; stores
      serialised `resume_data` so the orchestrator can be resumed after a
      server restart.
- [ ] Verify `get_events_after(run_id, seq)` orders by `seq ASC` and returns
      `EventRow { seq, event_type, payload }` for the SSE handler.

---

## 9. Human-Input / Suspension Round-Trip

- [ ] Check `db::upsert_suspension` persists `prompt`, `suggestions`, and
      the raw `resume_data` bytes before the server responds to SSE with the
      `awaiting_input` event.
- [ ] Check `db::get_suspension` can reload the `SuspendedRunData` when the
      server restarts mid-run (future resilience work).
- [ ] Validate `POST /runs/:id/answer` rejects requests for non-suspended runs
      with `409 CONFLICT` and rejects runs with no active orchestrator task
      with `410 GONE`.

---

## 10. Logging & Observability

- [ ] **No `tracing_subscriber` init needed** — already done globally in
      `start_server_and_web_app`.
- [ ] Confirm `tracing::info!` spans for `"pipeline started"`, `"pipeline done"`,
      `"pipeline suspended"`, and `"resuming after user answer"` are present.
- [ ] Use `tracing::trace!` for `llm_token` / `thinking_token` events (high
      volume) and `tracing::debug!` for everything else in the bridge task.
- [ ] Confirm fatal errors are logged with `tracing::error!` and persisted to
      `agentic_runs.error_message` so callers can surface the reason.

---

## 11. End-to-End Smoke Test

- [ ] Start the server (with a running PostgreSQL reachable via `OXY_DATABASE_URL`):
      `sh
    OXY_DATABASE_URL=postgresql://... ANTHROPIC_API_KEY=sk-... \
      cargo run -p oxy-app -- serve
    `
      No `AGENTIC_CONFIGS_DIR` is needed — the project path comes from
      `ProjectManager` injected by the project middleware per request.
- [ ] POST a run and capture `run_id` (replace `{project_id}` with a real UUID):
      `sh
  curl -s -X POST http://localhost:3000/api/{project_id}/analytics/runs \
    -H 'Content-Type: application/json' \
    -d '{"agent_id":"analytics","question":"How many rows in the dataset?"}'
  `
      The `agent_id` value (`analytics` here) must match a file named
      `analytics.agentic.yml` in the project root.
- [ ] Stream events until `done` or `error`:
      `sh
  curl -sN http://localhost:3000/api/{project_id}/analytics/runs/{run_id}/events
  `
- [ ] Verify the PostgreSQL DB contains rows in all three agentic tables after
      the run.
- [ ] Disconnect the SSE client mid-run, reconnect with `Last-Event-ID: {seq}`,
      and confirm no events are replayed before that sequence number.
