//! In-memory write-coalescing buffer for high-frequency camera ingest.
//!
//! Edge handlers `push` rows and return immediately; a background flusher
//! drains each `(workspace, stream)` buffer to airhouse as ONE batched INSERT
//! per flush window, collapsing one-DuckLake-commit-per-POST into one per
//! window. Lossy-OK: a hard crash loses ≤ one flush interval (see the design
//! doc). The per-buffer cap drops oldest so a stalled airhouse can't OOM oxy.

use std::collections::HashMap;
use std::time::Duration;

use uuid::Uuid;

use super::ingest::{BoxHealthPayload, CameraHealthPayload, EventPayload};

const DEFAULT_FLUSH_INTERVAL_MS: u64 = 5000;
const DEFAULT_FLUSH_MAX_ROWS: usize = 5000;
const DEFAULT_BUFFER_MAX_ROWS: usize = 100_000;
/// Consecutive rejected flushes of the same `(stream, workspace)` before the
/// batch is discarded — 60 × the 5s flush interval, so a batch is retried for
/// ~5 minutes before it is judged poison. Comfortably rides out a DP rollout or
/// a tripped circuit breaker (~30s), while bounding a content-rejected batch to
/// minutes instead of the ~2.5h it wedged prod for on 2026-07-15. `0` restores
/// the old retry-forever behaviour.
const DEFAULT_MAX_FLUSH_ATTEMPTS: u32 = 60;

/// One in-memory buffer per stream, each keyed by workspace (tenant).
#[derive(Default)]
pub(crate) struct Buffers {
    pub events: HashMap<Uuid, Vec<EventPayload>>,
    pub camera_health: HashMap<Uuid, Vec<CameraHealthPayload>>,
    pub box_health: HashMap<Uuid, Vec<BoxHealthPayload>>,
    /// Rows dropped since the last flush (cap exceeded); surfaced as a WARN.
    pub dropped: u64,
    /// Consecutive failed flushes per `(stream, workspace)`. Cleared on the
    /// first success. Drives [`requeue_or_quarantine`]'s poison-batch bound.
    pub flush_failures: HashMap<(&'static str, Uuid), u32>,
    /// Rows discarded by the poison-batch bound; surfaced as an ERROR.
    pub quarantined: u64,
}

/// Append `rows` for `ws`; drop oldest beyond `cap` (counting into `dropped`).
/// Returns `true` when the buffer has reached `flush_at` (size trigger).
pub(crate) fn push_generic<T>(
    map: &mut HashMap<Uuid, Vec<T>>,
    dropped: &mut u64,
    ws: Uuid,
    rows: Vec<T>,
    cap: usize,
    flush_at: usize,
) -> bool {
    let buf = map.entry(ws).or_default();
    buf.extend(rows);
    if cap > 0 && buf.len() > cap {
        let overflow = buf.len() - cap;
        buf.drain(0..overflow); // drop oldest
        *dropped += overflow as u64;
    }
    flush_at > 0 && buf.len() >= flush_at
}

fn env_u64(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

fn env_usize(var: &str, default: usize) -> usize {
    env_u64(var, default as u64) as usize
}

// Tunables are resolved from the environment ONCE at first use and cached. These
// accessors sit on the per-push hot path (the PR sizes ingest at ~216M
// events/day) — `buffer_max_rows`/`flush_max_rows` are read inside the buffers
// `Mutex` critical section — so re-`std::env::var`ing (global env-lock + a
// `String` alloc) per request would add needless contention. Env overrides
// still apply; they're just read at startup, not per push.
static FLUSH_INTERVAL: LazyLock<Duration> = LazyLock::new(|| {
    Duration::from_millis(env_u64(
        "OXY_CAMERAS_FLUSH_INTERVAL_MS",
        DEFAULT_FLUSH_INTERVAL_MS,
    ))
});
static FLUSH_MAX_ROWS: LazyLock<usize> =
    LazyLock::new(|| env_usize("OXY_CAMERAS_FLUSH_MAX_ROWS", DEFAULT_FLUSH_MAX_ROWS));
static BUFFER_MAX_ROWS: LazyLock<usize> =
    LazyLock::new(|| env_usize("OXY_CAMERAS_BUFFER_MAX_ROWS", DEFAULT_BUFFER_MAX_ROWS));
static BUFFERING_DISABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("OXY_CAMERAS_INGEST_BUFFER_DISABLED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
});
static MAX_FLUSH_ATTEMPTS: LazyLock<u32> = LazyLock::new(|| {
    env_u64(
        "OXY_CAMERAS_INGEST_MAX_FLUSH_ATTEMPTS",
        DEFAULT_MAX_FLUSH_ATTEMPTS as u64,
    ) as u32
});

pub(crate) fn flush_interval() -> Duration {
    *FLUSH_INTERVAL
}

pub(crate) fn flush_max_rows() -> usize {
    *FLUSH_MAX_ROWS
}

pub(crate) fn buffer_max_rows() -> usize {
    *BUFFER_MAX_ROWS
}

pub(crate) fn buffering_disabled() -> bool {
    *BUFFERING_DISABLED
}

pub(crate) fn max_flush_attempts() -> u32 {
    *MAX_FLUSH_ATTEMPTS
}

// ── Global singleton buffers + flush notify ──────────────────────────────────

use std::sync::{LazyLock, Mutex, OnceLock};
use tokio::sync::Notify;

pub(crate) fn buffers() -> &'static Mutex<Buffers> {
    static B: OnceLock<Mutex<Buffers>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(Buffers::default()))
}

pub(crate) fn flush_notify() -> &'static Notify {
    static N: OnceLock<Notify> = OnceLock::new();
    N.get_or_init(Notify::new)
}

// ── Typed push API ───────────────────────────────────────────────────────────
//
// The returned `usize` is "rows accepted into the buffer", NOT a durability
// guarantee — buffered rows reach airhouse on the next flush, and a row may be
// dropped by the cap in the same call (lossy-OK), so the count is an upper
// bound on what will actually land. In the disabled (synchronous) path it's the
// real airhouse-accepted count.

pub async fn push_events(ws: Uuid, rows: Vec<EventPayload>) -> super::ServiceResult<usize> {
    if buffering_disabled() {
        return super::ingest::write_events(ws, &rows)
            .await
            .map(|r| r.accepted);
    }
    let n = rows.len();
    let signal = {
        let mut b = buffers().lock().unwrap_or_else(|p| p.into_inner());
        let Buffers {
            events, dropped, ..
        } = &mut *b;
        push_generic(
            events,
            dropped,
            ws,
            rows,
            buffer_max_rows(),
            flush_max_rows(),
        )
    };
    if signal {
        flush_notify().notify_one();
    }
    Ok(n)
}

pub async fn push_camera_health(
    ws: Uuid,
    rows: Vec<CameraHealthPayload>,
) -> super::ServiceResult<usize> {
    if buffering_disabled() {
        return super::ingest::write_camera_health(ws, &rows)
            .await
            .map(|r| r.accepted);
    }
    let n = rows.len();
    let signal = {
        let mut b = buffers().lock().unwrap_or_else(|p| p.into_inner());
        let Buffers {
            camera_health,
            dropped,
            ..
        } = &mut *b;
        push_generic(
            camera_health,
            dropped,
            ws,
            rows,
            buffer_max_rows(),
            flush_max_rows(),
        )
    };
    if signal {
        flush_notify().notify_one();
    }
    Ok(n)
}

pub async fn push_box_health(ws: Uuid, rows: Vec<BoxHealthPayload>) -> super::ServiceResult<usize> {
    if buffering_disabled() {
        return super::ingest::write_box_health(ws, &rows)
            .await
            .map(|r| r.accepted);
    }
    let n = rows.len();
    let signal = {
        let mut b = buffers().lock().unwrap_or_else(|p| p.into_inner());
        let Buffers {
            box_health,
            dropped,
            ..
        } = &mut *b;
        push_generic(
            box_health,
            dropped,
            ws,
            rows,
            buffer_max_rows(),
            flush_max_rows(),
        )
    };
    if signal {
        flush_notify().notify_one();
    }
    Ok(n)
}

// ── Sink trait + flusher ─────────────────────────────────────────────────────

use std::sync::Arc;

#[async_trait::async_trait]
pub(crate) trait Sink: Send + Sync {
    async fn write_events(&self, ws: Uuid, rows: &[EventPayload]) -> super::ServiceResult<()>;
    async fn write_camera_health(
        &self,
        ws: Uuid,
        rows: &[CameraHealthPayload],
    ) -> super::ServiceResult<()>;
    async fn write_box_health(
        &self,
        ws: Uuid,
        rows: &[BoxHealthPayload],
    ) -> super::ServiceResult<()>;
}

struct AirhouseSink;

#[async_trait::async_trait]
impl Sink for AirhouseSink {
    async fn write_events(&self, ws: Uuid, rows: &[EventPayload]) -> super::ServiceResult<()> {
        super::ingest::write_events(ws, rows).await.map(|_| ())
    }

    async fn write_camera_health(
        &self,
        ws: Uuid,
        rows: &[CameraHealthPayload],
    ) -> super::ServiceResult<()> {
        super::ingest::write_camera_health(ws, rows)
            .await
            .map(|_| ())
    }

    async fn write_box_health(
        &self,
        ws: Uuid,
        rows: &[BoxHealthPayload],
    ) -> super::ServiceResult<()> {
        super::ingest::write_box_health(ws, rows).await.map(|_| ())
    }
}

/// Re-prepend `failed` rows (oldest-first) ahead of any rows that arrived
/// during the write, then enforce the cap (drop oldest beyond it).
fn requeue<T>(map: &mut HashMap<Uuid, Vec<T>>, dropped: &mut u64, ws: Uuid, mut failed: Vec<T>) {
    let buf = map.entry(ws).or_default();
    failed.append(buf); // failed = [failed_old..., new...]
    std::mem::swap(buf, &mut failed);
    let cap = buffer_max_rows();
    if cap > 0 && buf.len() > cap {
        let overflow = buf.len() - cap;
        buf.drain(0..overflow);
        *dropped += overflow as u64;
    }
}

/// [`requeue`], bounded: once a `(stream, workspace)` has failed
/// `max_flush_attempts` times in a row, discard the batch instead of putting
/// it back.
///
/// Requeuing forever assumes every failure is transient. A batch that fails on
/// its *content* never stops failing, so it is re-sent every flush interval —
/// the same rows, the same rejection, indefinitely. The stream never drains,
/// and each retry is another backend failure feeding the DP's circuit breaker.
/// That is not hypothetical: on 2026-07-15 `oxy_cam_events` and
/// `oxy_cam_camera_health` each re-sent a rejected batch every 5s for ~2.5h
/// (~12 failures/min/stream), and only an `oxy-0` restart — which drops these
/// in-memory buffers wholesale — cleared it.
///
/// Discarding loses the batch, which the retry-forever path *also* does: the
/// buffer grows to `buffer_max_rows` and `requeue` silently drops the oldest
/// rows anyway. The difference is that this bounds the loss, says so at ERROR,
/// and lets the stream resume.
///
/// The counter resets after a quarantine so the next batch is judged on its own
/// merits — a poison batch shouldn't leave the stream permanently trigger-happy.
#[allow(clippy::too_many_arguments)]
fn requeue_or_quarantine<T>(
    map: &mut HashMap<Uuid, Vec<T>>,
    dropped: &mut u64,
    failures: &mut HashMap<(&'static str, Uuid), u32>,
    quarantined: &mut u64,
    stream: &'static str,
    ws: Uuid,
    failed: Vec<T>,
    max: u32,
) {
    let attempts = failures.entry((stream, ws)).or_insert(0);
    *attempts += 1;

    if max == 0 || *attempts < max {
        requeue(map, dropped, ws, failed);
        return;
    }

    let n = failed.len();
    *quarantined += n as u64;
    failures.remove(&(stream, ws));
    drop(failed);
    tracing::error!(
        workspace_id = %ws,
        stream,
        rows = n,
        attempts = max,
        "cameras ingest batch rejected {max} times in a row; discarding it. \
         The rows are lost. A batch that fails this consistently is being \
         refused on its content, not on a transient backend fault — check the \
         airhouse DP logs for the underlying error"
    );
}

/// Reset a `(stream, workspace)`'s consecutive-failure count after a flush
/// lands, so only an *unbroken* run of rejections trips the poison-batch bound.
/// Takes the lock only when there is something to clear — the steady state is a
/// successful flush with no entry.
fn clear_failures(buffers: &Mutex<Buffers>, stream: &'static str, ws: Uuid) {
    let mut b = buffers.lock().unwrap_or_else(|p| p.into_inner());
    b.flush_failures.remove(&(stream, ws));
}

/// Drain every buffer (swap-out under the lock) and write each batch outside
/// the lock; requeue a batch whose write fails.
pub(crate) async fn flush_all(buffers: &Mutex<Buffers>, sink: &dyn Sink) {
    // The guarded state is plain `Vec`/`HashMap`; recover from a poisoned lock
    // (`into_inner`) rather than panicking, so one unlucky panic can't wedge
    // every tenant's ingest for the process lifetime.
    let (events, camera_health, box_health, dropped, quarantined) = {
        let mut b = buffers.lock().unwrap_or_else(|p| p.into_inner());
        (
            std::mem::take(&mut b.events),
            std::mem::take(&mut b.camera_health),
            std::mem::take(&mut b.box_health),
            std::mem::take(&mut b.dropped),
            std::mem::take(&mut b.quarantined),
        )
    };
    if dropped > 0 {
        tracing::warn!(dropped, "cameras ingest buffer dropped rows (cap exceeded)");
    }
    if quarantined > 0 {
        tracing::error!(
            quarantined,
            "cameras ingest discarded rows from repeatedly-rejected batches"
        );
    }
    for (ws, rows) in events {
        let n = rows.len();
        let started = std::time::Instant::now();
        match sink.write_events(ws, &rows).await {
            Ok(()) => {
                tracing::info!(
                    workspace_id = %ws,
                    stream = "events",
                    rows = n,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "cameras ingest flushed"
                );
                clear_failures(buffers, "events", ws);
            }
            Err(e) => {
                tracing::warn!(
                    workspace_id = %ws,
                    stream = "events",
                    rows = n,
                    error = %e,
                    "cameras ingest flush failed; requeuing"
                );
                let mut b = buffers.lock().unwrap_or_else(|p| p.into_inner());
                let Buffers {
                    events,
                    dropped,
                    flush_failures,
                    quarantined,
                    ..
                } = &mut *b;
                requeue_or_quarantine(
                    events,
                    dropped,
                    flush_failures,
                    quarantined,
                    "events",
                    ws,
                    rows,
                    max_flush_attempts(),
                );
            }
        }
    }
    for (ws, rows) in camera_health {
        let n = rows.len();
        let started = std::time::Instant::now();
        match sink.write_camera_health(ws, &rows).await {
            Ok(()) => {
                tracing::info!(
                    workspace_id = %ws,
                    stream = "camera_health",
                    rows = n,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "cameras ingest flushed"
                );
                clear_failures(buffers, "camera_health", ws);
            }
            Err(e) => {
                tracing::warn!(
                    workspace_id = %ws,
                    stream = "camera_health",
                    rows = n,
                    error = %e,
                    "cameras ingest flush failed; requeuing"
                );
                let mut b = buffers.lock().unwrap_or_else(|p| p.into_inner());
                let Buffers {
                    camera_health,
                    dropped,
                    flush_failures,
                    quarantined,
                    ..
                } = &mut *b;
                requeue_or_quarantine(
                    camera_health,
                    dropped,
                    flush_failures,
                    quarantined,
                    "camera_health",
                    ws,
                    rows,
                    max_flush_attempts(),
                );
            }
        }
    }
    for (ws, rows) in box_health {
        let n = rows.len();
        let started = std::time::Instant::now();
        match sink.write_box_health(ws, &rows).await {
            Ok(()) => {
                tracing::info!(
                    workspace_id = %ws,
                    stream = "box_health",
                    rows = n,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "cameras ingest flushed"
                );
                clear_failures(buffers, "box_health", ws);
            }
            Err(e) => {
                tracing::warn!(
                    workspace_id = %ws,
                    stream = "box_health",
                    rows = n,
                    error = %e,
                    "cameras ingest flush failed; requeuing"
                );
                let mut b = buffers.lock().unwrap_or_else(|p| p.into_inner());
                let Buffers {
                    box_health,
                    dropped,
                    flush_failures,
                    quarantined,
                    ..
                } = &mut *b;
                requeue_or_quarantine(
                    box_health,
                    dropped,
                    flush_failures,
                    quarantined,
                    "box_health",
                    ws,
                    rows,
                    max_flush_attempts(),
                );
            }
        }
    }
}

async fn run(sink: Arc<dyn Sink>, shutdown: tokio_util::sync::CancellationToken) {
    let mut ticker = tokio::time::interval(flush_interval());
    ticker.tick().await; // consume the immediate tick
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                flush_all(buffers(), sink.as_ref()).await;
                tracing::info!("cameras ingest buffer drained on shutdown");
                return;
            }
            _ = ticker.tick() => flush_all(buffers(), sink.as_ref()).await,
            _ = flush_notify().notified() => flush_all(buffers(), sink.as_ref()).await,
        }
    }
}

/// Spawn the flusher. No-op (synchronous writes) when buffering is disabled.
pub fn spawn(shutdown: tokio_util::sync::CancellationToken) {
    if buffering_disabled() {
        tracing::info!("cameras ingest buffer disabled; writes are synchronous");
        return;
    }
    tracing::info!(
        interval_ms = flush_interval().as_millis() as u64,
        flush_max_rows = flush_max_rows(),
        buffer_max_rows = buffer_max_rows(),
        "cameras ingest buffer flusher started"
    );
    tokio::spawn(run(Arc::new(AirhouseSink), shutdown));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // `TEST_LOCK` is a deliberate test serializer held across the system-under-
    // test `.await` (the async `push_*` call) to keep the global buffer state
    // from racing between tests — that's the intent, not a bug, so silence the
    // `await_holding_lock` lint for this test module only.
    #![allow(clippy::await_holding_lock)]
    use super::*;
    use serial_test::serial;

    // Serialize buffer-state tests so global OnceLock state doesn't race.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn event(track: &str) -> EventPayload {
        EventPayload {
            event_id: uuid::Uuid::nil(),
            ts: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            camera_id: uuid::Uuid::nil(),
            event_type: "person".into(),
            zone_id: None,
            line_id: None,
            track_id: track.into(),
            dwell_seconds: None,
            confidence: None,
            frame_uri: None,
        }
    }

    #[serial]
    #[tokio::test]
    async fn push_events_buffers_without_writing() {
        let _g = TEST_LOCK.lock().unwrap();
        // SAFETY: serialized via TEST_LOCK.
        unsafe {
            std::env::remove_var("OXY_CAMERAS_INGEST_BUFFER_DISABLED");
        }
        {
            buffers()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .events
                .clear();
        }
        let ws = uuid::Uuid::from_u128(7);
        let n = push_events(ws, vec![event("a"), event("b")]).await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            buffers().lock().unwrap_or_else(|p| p.into_inner()).events[&ws].len(),
            2
        );
    }

    #[test]
    fn push_generic_drops_oldest_beyond_cap() {
        let mut m: HashMap<uuid::Uuid, Vec<i32>> = HashMap::new();
        let mut dropped = 0u64;
        let ws = uuid::Uuid::nil();
        // cap=3, flush_at=100. Push 1,2 then 3,4,5 → keep last 3 = [3,4,5], dropped=2.
        push_generic(&mut m, &mut dropped, ws, vec![1, 2], 3, 100);
        push_generic(&mut m, &mut dropped, ws, vec![3, 4, 5], 3, 100);
        assert_eq!(m[&ws], vec![3, 4, 5]);
        assert_eq!(dropped, 2);
    }

    #[test]
    fn push_generic_signals_at_flush_threshold() {
        let mut m: HashMap<uuid::Uuid, Vec<i32>> = HashMap::new();
        let mut dropped = 0u64;
        let ws = uuid::Uuid::nil();
        assert!(!push_generic(&mut m, &mut dropped, ws, vec![1, 2], 100, 3)); // 2 < 3
        assert!(push_generic(&mut m, &mut dropped, ws, vec![3], 100, 3)); // 3 >= 3
    }

    // ── flush_all tests ──────────────────────────────────────────────────────

    struct FakeSink {
        events: std::sync::Mutex<Vec<(uuid::Uuid, usize)>>, // (ws, rows) per write_events call
        fail_events: bool,
    }

    #[async_trait::async_trait]
    impl Sink for FakeSink {
        async fn write_events(
            &self,
            ws: uuid::Uuid,
            rows: &[EventPayload],
        ) -> crate::service::ServiceResult<()> {
            if self.fail_events {
                return Err(crate::service::ServiceError::Airhouse(
                    crate::airhouse::AirhouseError::Insert("boom".into()),
                ));
            }
            self.events.lock().unwrap().push((ws, rows.len()));
            Ok(())
        }

        async fn write_camera_health(
            &self,
            _ws: uuid::Uuid,
            _r: &[CameraHealthPayload],
        ) -> crate::service::ServiceResult<()> {
            Ok(())
        }

        async fn write_box_health(
            &self,
            _ws: uuid::Uuid,
            _r: &[BoxHealthPayload],
        ) -> crate::service::ServiceResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn flush_coalesces_pushes_into_one_write() {
        let bufs = Mutex::new(Buffers::default());
        let ws = uuid::Uuid::from_u128(1);
        {
            let mut b = bufs.lock().unwrap();
            let Buffers {
                events, dropped, ..
            } = &mut *b;
            push_generic(
                events,
                dropped,
                ws,
                vec![event("a"), event("b")],
                100_000,
                100_000,
            );
            push_generic(events, dropped, ws, vec![event("c")], 100_000, 100_000);
        }
        let sink = FakeSink {
            events: Mutex::new(vec![]),
            fail_events: false,
        };
        flush_all(&bufs, &sink).await;
        // 3 pushes → ONE write_events of 3 rows; buffer now empty.
        assert_eq!(*sink.events.lock().unwrap(), vec![(ws, 3)]);
        assert!(bufs.lock().unwrap().events.is_empty());
    }

    #[tokio::test]
    async fn flush_failure_requeues_rows() {
        let bufs = Mutex::new(Buffers::default());
        let ws = uuid::Uuid::from_u128(2);
        {
            let mut b = bufs.lock().unwrap();
            let Buffers {
                events, dropped, ..
            } = &mut *b;
            push_generic(
                events,
                dropped,
                ws,
                vec![event("a"), event("b")],
                100_000,
                100_000,
            );
        }
        let sink = FakeSink {
            events: Mutex::new(vec![]),
            fail_events: true,
        };
        flush_all(&bufs, &sink).await;
        // write failed → rows requeued for the next cycle.
        assert_eq!(bufs.lock().unwrap().events[&ws].len(), 2);
    }

    /// Below the bound a rejected batch keeps its place at the head, so a
    /// transient backend fault (DP rollout, tripped breaker) loses nothing.
    #[test]
    fn rejected_batch_is_requeued_while_under_the_attempt_bound() {
        let ws = Uuid::nil();
        let mut map: HashMap<Uuid, Vec<EventPayload>> = HashMap::new();
        let (mut dropped, mut quarantined) = (0u64, 0u64);
        let mut failures = HashMap::new();

        let mut batch = vec![event("a")];
        for i in 1..3 {
            requeue_or_quarantine(
                &mut map,
                &mut dropped,
                &mut failures,
                &mut quarantined,
                "events",
                ws,
                batch,
                3,
            );
            assert_eq!(map[&ws].len(), 1, "batch must survive attempt {i}");
            assert_eq!(failures[&("events", ws)], i);
            batch = map.remove(&ws).unwrap(); // the next flush drains it again
        }
        assert_eq!(quarantined, 0);
    }

    /// The bug this exists for: a batch rejected on its content is re-sent every
    /// flush interval forever, wedging the stream and feeding the DP's breaker
    /// (prod 2026-07-15: ~12 failures/min/stream for 2.5h). At the bound it must
    /// be discarded so the stream can drain again.
    #[test]
    fn batch_is_quarantined_once_it_hits_the_attempt_bound() {
        let ws = Uuid::nil();
        let mut map: HashMap<Uuid, Vec<EventPayload>> = HashMap::new();
        let (mut dropped, mut quarantined) = (0u64, 0u64);
        let mut failures = HashMap::new();

        // Three flush cycles: drain the buffer, fail, requeue — until the bound.
        let mut batch = vec![event("a"), event("b")];
        for _ in 0..3 {
            requeue_or_quarantine(
                &mut map,
                &mut dropped,
                &mut failures,
                &mut quarantined,
                "events",
                ws,
                batch,
                3,
            );
            batch = map.remove(&ws).unwrap_or_default();
        }

        assert_eq!(quarantined, 2, "the rejected rows are counted as discarded");
        assert!(
            map.get(&ws).is_none_or(|b| b.is_empty()),
            "the poison batch must not be put back"
        );
        assert!(
            !failures.contains_key(&("events", ws)),
            "counter resets after quarantine so the next batch starts clean"
        );
    }

    /// Rows that arrived *during* the failed write are innocent — they were
    /// never attempted. Quarantine must discard only the batch that was
    /// actually rejected.
    #[test]
    fn quarantine_keeps_rows_that_arrived_during_the_write() {
        let ws = Uuid::nil();
        let mut map: HashMap<Uuid, Vec<EventPayload>> = HashMap::new();
        map.insert(ws, vec![event("arrived-during-write")]);
        let (mut dropped, mut quarantined) = (0u64, 0u64);
        let mut failures = HashMap::new();

        requeue_or_quarantine(
            &mut map,
            &mut dropped,
            &mut failures,
            &mut quarantined,
            "events",
            ws,
            vec![event("poison")],
            1,
        );

        assert_eq!(quarantined, 1);
        assert_eq!(map[&ws].len(), 1);
        assert_eq!(map[&ws][0].track_id, "arrived-during-write");
    }

    /// A success between rejections means the backend is flaky, not the batch —
    /// the run must restart so flakiness never accumulates into a quarantine.
    #[test]
    fn success_resets_the_consecutive_failure_run() {
        let ws = Uuid::nil();
        let bufs = Mutex::new(Buffers::default());
        bufs.lock()
            .unwrap()
            .flush_failures
            .insert(("events", ws), 2);

        clear_failures(&bufs, "events", ws);

        assert!(bufs.lock().unwrap().flush_failures.is_empty());
    }

    /// `0` restores the pre-existing retry-forever behaviour as an escape hatch.
    #[test]
    fn attempt_bound_of_zero_never_quarantines() {
        let ws = Uuid::nil();
        let mut map: HashMap<Uuid, Vec<EventPayload>> = HashMap::new();
        let (mut dropped, mut quarantined) = (0u64, 0u64);
        let mut failures = HashMap::new();

        for _ in 0..50 {
            requeue_or_quarantine(
                &mut map,
                &mut dropped,
                &mut failures,
                &mut quarantined,
                "events",
                ws,
                vec![event("a")],
                0,
            );
        }
        assert_eq!(quarantined, 0);
        assert_eq!(map[&ws].len(), 50);
    }
}
