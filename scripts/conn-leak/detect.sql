-- Detect the Postgres connection exhaustion described in the 2026-09-01 incident.
--
-- Read-only. Every statement is a SELECT; nothing here mutates state, so it is
-- safe to run against production during an incident.
--
-- WHAT THIS LOOKS FOR. The incident has two independent halves, and the fix for
-- one does nothing for the other, so they are reported separately:
--
--   FUEL (section A) — `find_pending_global_runs` selects root runs that have a
--   `queued` + `scope_owned=false` queue entry and no live driver lease. The
--   cloud latency worker (`tick_cloud`) then resolves each row's workspace to
--   build a context. `agentic_runs.workspace_id` is a SOFT fk -- a plain UUID
--   column with no constraint (crates/agentic/runtime/src/migration.rs, mig 17)
--   -- and `workspaces::Entity::delete_by_id` is a hard delete. So a deleted
--   workspace leaves runs pointing at nothing. `tick_cloud` logs and `continue`s
--   WITHOUT touching the row, so the identical row is re-selected on the next
--   poll, forever. This section counts that permanently-unresolvable set.
--
--   AMPLIFIER (section C) — sqlx keeps idle connections in a crossbeam
--   `ArrayQueue`, which is FIFO, and `PoolInner::release()` stamps
--   `idle_since = Instant::now()` every time a connection goes back. The reaper
--   (period = min(max_lifetime, idle_timeout) = 300s) pops each idle connection,
--   finds it under the 300s threshold, and releases it -- resetting the very
--   clock it just measured. Any steady poller therefore rotates the whole idle
--   set and keeps it alive: at N connections it takes only N/300 acquires/sec to
--   prevent every reap. For N=80 that is 0.27/sec, and a single 500ms tailer
--   supplies 2/sec. The pool ratchets to a high-water mark and stays there.
--
-- WHAT THIS CANNOT PROVE. Postgres sees backends, not pools. A high count here
-- is consistent with the ratchet but does not by itself distinguish "leaked" from
-- "legitimately busy" -- section C's `touched_recently` column is what separates
-- them, because a genuinely busy pool has connections in `active`, while a
-- ratcheted one is almost entirely `idle` yet constantly re-touched.

\set QUIET on
\pset pager off
\timing off

\echo ''
\echo '================================================================'
\echo ' A. SPIN FUEL -- unresolvable rows in find_pending_global_runs'
\echo '================================================================'
\echo ''

-- Mirrors crates/agentic/runtime/src/orchestrator/crud/recovery.rs
-- `find_pending_global_runs`, verbatim, with DRIVER_LEASE_TTL_SECS = 90
-- (crates/agentic/runtime/src/lifecycle/crud/mod.rs:48).
CREATE TEMP VIEW _pending AS
SELECT r.id, r.workspace_id, r.task_status, r.source_type,
       r.created_at, r.updated_at
FROM agentic_runs r
WHERE r.parent_run_id IS NULL
  AND r.task_status IN ('running', 'delegating', 'waiting_on_child',
                        'waiting_on_children', 'needs_resume', 'shutdown')
  AND (r.driver_id IS NULL
       OR r.driver_heartbeat_at IS NULL
       OR r.driver_heartbeat_at < now() - make_interval(secs => 90))
  AND EXISTS (
      SELECT 1 FROM agentic_task_queue q
      WHERE (q.task_id = r.id OR q.task_id LIKE r.id || '.%')
        AND q.queue_status = 'queued'
        AND q.scope_owned = false
  );

SELECT
  CASE
    WHEN w.id IS NULL   THEN 'ORPHAN (workspace row deleted)'
    WHEN w.path IS NULL THEN 'ORPHAN (workspace has no path)'
    ELSE                     'resolvable (drains normally)'
  END                                   AS classification,
  count(*)                              AS runs,
  count(DISTINCT p.workspace_id)        AS workspaces,
  min(p.created_at)                     AS oldest_run,
  max(date_trunc('second', now() - p.updated_at)) AS longest_untouched
FROM _pending p
LEFT JOIN workspaces w ON w.id = p.workspace_id
GROUP BY 1
ORDER BY 1;

\echo ''
\echo '-- the orphans themselves (these are re-selected on every poll, forever)'
SELECT p.id AS run_id, p.workspace_id, p.task_status, p.source_type,
       date_trunc('second', now() - p.created_at) AS age,
       CASE WHEN w.id IS NULL THEN 'workspace deleted' ELSE 'path is null' END AS why
FROM _pending p
LEFT JOIN workspaces w ON w.id = p.workspace_id
WHERE w.id IS NULL OR w.path IS NULL
ORDER BY p.created_at;

\echo ''
\echo '-- proof the orphans can never retire on their own.'
\echo '-- reap_stale_tasks() only touches queue_status = ''claimed'' (both its'
\echo '-- dead-letter and requeue arms), and defer_task() -- the only other'
\echo '-- dead-letter door -- requires a held claim. tick_cloud skips these runs'
\echo '-- BEFORE drive_pending, so they are never claimed. claim_count = 0 and'
\echo '-- first_deferred_at IS NULL below confirm no door is reachable.'
SELECT q.queue_status, q.claim_count, q.max_claims,
       (q.first_deferred_at IS NULL) AS never_deferred,
       (q.last_heartbeat IS NULL)    AS never_claimed,
       count(*) AS queue_rows
FROM agentic_task_queue q
JOIN _pending p ON (q.task_id = p.id OR q.task_id LIKE p.id || '.%')
LEFT JOIN workspaces w ON w.id = p.workspace_id
WHERE (w.id IS NULL OR w.path IS NULL)
  AND q.queue_status = 'queued' AND q.scope_owned = false
GROUP BY 1,2,3,4,5
ORDER BY 1;

\echo ''
\echo '================================================================'
\echo ' B. SPIN EVIDENCE -- how often the two loops actually ran'
\echo '================================================================'

SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements')
  AS has_pgss \gset

\if :has_pgss
\echo ''
\echo '-- calls/sec is the load-bearing column. The latency worker at'
\echo '-- OXY_LATENCY_WORKER_INTERVAL_MS=300 is 3.33/sec PER DRIVER PROCESS;'
\echo '-- the world-model tailer is 2.0/sec per pod. Divide calls/sec by those'
\echo '-- rates to recover how many processes are polling.'
SELECT
  CASE
    WHEN query ILIKE '%agentic_task_queue%scope_owned%' THEN 'latency worker (find_pending_global_runs)'
    WHEN query ILIKE '%world_model_events%'             THEN 'world-model tailer'
    ELSE left(regexp_replace(query, '\s+', ' ', 'g'), 60)
  END                                                       AS loop,
  s.calls,
  s.rows,
  round((s.calls / GREATEST(extract(epoch FROM now() - g.stats_reset), 1))::numeric, 2)
                                                            AS calls_per_sec,
  round((s.total_exec_time / 1000)::numeric, 1)             AS total_exec_s,
  round(s.mean_exec_time::numeric, 3)                       AS mean_ms
FROM pg_stat_statements s, pg_stat_statements_info g
WHERE s.query ILIKE '%agentic_task_queue%scope_owned%'
   OR s.query ILIKE '%world_model_events%'
ORDER BY s.calls DESC
LIMIT 10;
\else
\echo ''
\echo '-- pg_stat_statements not installed; skipping. Without it, infer the poll'
\echo '-- rate from section C instead (a driver process holds a connection whose'
\echo '-- state_change is never older than its poll interval).'
\endif

\echo ''
\echo '================================================================'
\echo ' C. THE LEAK -- connection census and the no-reap signature'
\echo '================================================================'
\echo ''
\echo '-- headroom. "FATAL: sorry, too many clients already" is thrown when'
\echo '-- used >= max_connections - reserved.'
SELECT
  (SELECT setting::int FROM pg_settings WHERE name = 'max_connections')                AS max_connections,
  (SELECT setting::int FROM pg_settings WHERE name = 'superuser_reserved_connections') AS reserved,
  (SELECT count(*) FROM pg_stat_activity)                                              AS used,
  (SELECT setting::int FROM pg_settings WHERE name = 'max_connections')
    - (SELECT setting::int FROM pg_settings WHERE name = 'superuser_reserved_connections')
    - (SELECT count(*) FROM pg_stat_activity)                                          AS headroom;

\echo ''
\echo '-- per client host. Each pod is one process = one sqlx pool, so this is'
\echo '-- effectively per-pod. A pool that has ratcheted shows a count near its'
\echo '-- OXY_DATABASE_MAX_CONNECTIONS ceiling (default 80) with almost all idle.'
SELECT client_addr,
       count(*)                                        AS backends,
       count(*) FILTER (WHERE state = 'active')        AS active,
       count(*) FILTER (WHERE state = 'idle')          AS idle,
       count(*) FILTER (WHERE state = 'idle in transaction') AS idle_in_txn,
       date_trunc('second', now() - min(backend_start)) AS oldest_backend
FROM pg_stat_activity
WHERE datname = current_database() AND client_addr IS NOT NULL
GROUP BY 1
ORDER BY backends DESC;

\echo ''
\echo '-- THE SIGNATURE. `stale` counts idle backends older than the 300s'
\echo '-- idle_timeout that should already have been reaped. `touched_recently`'
\echo '-- counts those whose idle clock was reset inside the last 300s -- which'
\echo '-- is what the FIFO rotation does. stale high AND touched_recently high'
\echo '-- means the reaper is being outrun, not that the pool is busy.'
SELECT client_addr,
       count(*)                                                                   AS idle_backends,
       count(*) FILTER (WHERE now() - backend_start > interval '300 seconds')      AS stale,
       count(*) FILTER (WHERE now() - state_change  < interval '300 seconds')      AS touched_recently,
       date_trunc('second', max(now() - backend_start)) AS oldest,
       date_trunc('second', min(now() - state_change)) AS most_recently_touched
FROM pg_stat_activity
WHERE datname = current_database() AND state = 'idle' AND client_addr IS NOT NULL
GROUP BY 1
HAVING count(*) > 5
ORDER BY idle_backends DESC;

\echo ''
\echo '-- connection refusals. sqlx_logging(false) means a rejected connect never'
\echo '-- reaches the app log as a query error, so the server log is the only'
\echo '-- place "too many clients" is recorded. This confirms whether the'
\echo '-- backend has been refusing connects at all.'
SELECT datname, numbackends, xact_commit, xact_rollback,
       blks_read, deadlocks, stats_reset
FROM pg_stat_database
WHERE datname = current_database();

\echo ''
\echo '================================================================'
\echo ' D. ROTATION -- which loop is holding each connection open'
\echo '================================================================'
\echo ''
\echo '-- The most diagnostic view of the set. `state = idle` means the backend'
\echo '-- is running nothing; `query` is the LAST statement it ran, so this'
\echo '-- attributes every held connection to the loop that touched it.'
\echo '--'
\echo '-- Read `since_state_change`: sqlx can only reap a connection that sat'
\echo '-- untouched for the full 300s idle_timeout. If this column reads 0-2s'
\echo '-- across the board, idle_timeout provably never fires and the pool can'
\echo '-- never contract, whatever its size.'
\echo '--'
\echo '-- Read `n` (connections stamped with the same query): a SEQUENTIAL poller'
\echo '-- showing n=5..7 is the FIFO signature. sqlx pops the idle queue from the'
\echo '-- head and pushes to the tail, so one single-threaded loop round-robins'
\echo '-- across the whole idle set, resetting each connection''s clock in turn.'
\echo '-- It is one caller keeping N connections alive, not N concurrent callers.'
SELECT client_addr,
       state,
       date_trunc('second', now() - backend_start) AS age,
       date_trunc('second', now() - state_change)  AS since_state_change,
       count(*) OVER (PARTITION BY client_addr,
                                   left(coalesce(query, ''), 40)) AS n,
       left(regexp_replace(coalesce(query, ''), '\s+', ' ', 'g'), 70) AS last_query
FROM pg_stat_activity
WHERE datname = current_database()
  AND client_addr IS NOT NULL
  AND pid <> pg_backend_pid()
ORDER BY client_addr, age DESC;
