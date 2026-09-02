# conn-leak

Read-only diagnostic for Postgres connection exhaustion. Built during the
2026-09-01 oxy-prod incident, where the homepage intermittently hung for 30s
and returned 500s because requests were queued on `sqlx`'s pool, not because
any handler was slow.

```bash
./detect.sh --url "postgres://…"
./detect.sh --context oxy-dev --pod oxy-dev-postgres-1 --password oxypass
```

Every statement is a `SELECT` plus one `CREATE TEMP VIEW`. Safe to run against
production during an incident.

## What each section answers

**§A — is anything spinning?** Replicates `find_pending_global_runs` verbatim
and classifies each selected row by whether its workspace can still be
resolved. A run whose workspace was hard-deleted is unrunnable by definition,
and `tick_cloud` used to skip it without a write, so it was re-selected on the
next poll forever. §A also asserts *why* such a row could never retire on its
own: both arms of `reap_stale_tasks` are gated on `queue_status = 'claimed'`
and `defer_task` needs a held claim, but the skip happens before the run is
ever claimed.

**§B — how often did the loops run?** Needs `pg_stat_statements`. `calls_per_sec`
is the load-bearing column: divide it by the known poll rates
(`OXY_LATENCY_WORKER_INTERVAL_MS`, and 2/sec for the world-model tailer) to
recover how many processes are polling.

**§C — how much headroom is left, and is the pool reaping?** `stale` counts
idle backends older than `idle_timeout`; `touched_recently` counts those whose
idle clock was reset inside that window. **Both high at once means the reaper is
being outrun, not that the pool is busy.**

**§D — which loop is holding each connection?** The most diagnostic view.
`state = idle` means the backend is running nothing, so `query` attributes the
connection to whatever last touched it. A *sequential* poller showing `n = 5..7`
is the FIFO signature: `sqlx`'s idle queue is a `crossbeam` `ArrayQueue` and
`release()` stamps `idle_since = Instant::now()`, so one loop round-robins the
whole idle set and keeps every connection's clock alive.

## The counter-intuitive part

`idle_timeout` cannot fire on a pool that anything polls. At *N* idle
connections you need only `N / idle_timeout` acquires per second to defeat
every reap — at our 80 ceiling and 300s timeout, **0.27/sec**, and a single
500ms poller supplies 2/sec. `max_lifetime` becomes the only thing that ever
closes a connection. A pool in this state churns but never contracts, so its
size reflects a high-water mark rather than steady demand.

This is why a connection count that climbs and then plateaus is the signature
to look for, and why "the pool is big" is not evidence that load is high.

## Connecting in each environment

| Env | Route |
| --- | ----- |
| oxy-dev | in-cluster Postgres: `--context oxy-dev --pod oxy-dev-postgres-1` |
| oxy-prod | RDS, not publicly accessible — needs an in-VPC pod. `postgres:17-alpine` is already pullable; credentials are in secret `oxy-rds-secret`, key `OXY_DATABASE_URL`. |
