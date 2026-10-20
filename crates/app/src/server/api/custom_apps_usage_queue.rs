//! The bounded in-process queue between the custom-app serve path and the
//! usage tables.
//!
//! Before this module a navigation did `tokio::spawn(record_view)` — a new
//! connection, ~7 role-resolution reads and an insert per page load — and the
//! beacon spawned `record_event` per pageview or web vital. Nothing bounded
//! either: a busy app, or a slow database, grew the number of in-flight tasks
//! (and held connections) without limit. Now the request path only [`enqueue`]s,
//! and one drain task writes.
//!
//! **Why an in-process queue and not a `TaskSpec`** (`oxy-task-spec-default`).
//! The work is "flush *this process's* memory", exactly the class
//! `token_usage_flush` documents: a task claimed by another worker would have
//! nothing to write, and the data is deliberately not durable — a lost view row
//! is the stated trade for not writing one per request. Every HTTP replica runs
//! its own drain; the rows are independent inserts, so they simply sum.
//!
//! **Backpressure: drop, never block.** The queue has a fixed [`CAPACITY`] and
//! producers `try_send`. A full queue drops the row and counts it
//! ([`dropped_total`], plus a `warn!` on the first drop and every
//! [`DROP_LOG_EVERY`]th after; the drain also reports the totals once a minute
//! while they grow — the log is the only signal, there is no metrics endpoint). The request must never wait on telemetry, and
//! an unbounded buffer would only trade lost rows for an out-of-memory kill.
//!
//! **Batching.** The drain wakes every [`FLUSH_INTERVAL`] (or sooner once
//! [`BATCH_MAX`] rows are waiting), takes up to [`BATCH_MAX`] jobs, and writes
//! them with one connection and one multi-row `INSERT` per table. Role
//! resolution — the expensive part of a view — runs once per distinct
//! `(app, viewer)` per batch, with a little concurrency. If a multi-row insert
//! fails (for instance one row's app was deleted since it was queued, which
//! would otherwise sink its neighbours), the batch falls back to row-by-row so
//! only the rows that are actually bad are lost.
//!
//! **Shutdown.** The drain is registered as a shutdown hook and does a final
//! [`flush`] when the shutdown token fires. **Tests** call [`flush`] to make
//! queued rows visible without waiting for a tick; it drains inline whether or
//! not the background task exists, and waits for a batch the task has in
//! flight.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, FixedOffset};
use entity::{custom_app_event, custom_app_view_event};
use oxy::database::client::establish_connection;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection, EntityTrait};
use sentry::SentryFutureExt;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::custom_apps_tracking::{build_event_model, resolve_view_roles};

/// Jobs the queue holds before it starts dropping. A view job carries one
/// `apps::Model` and a few short strings, an event job at most
/// `MAX_EVENT_PAYLOAD_BYTES`, so the worst case is a few tens of MB — and at
/// the 500 ms drain cadence it is minutes of headroom at any realistic rate.
pub const CAPACITY: usize = 4096;
/// Most jobs written per drain round (one multi-row insert per table).
pub const BATCH_MAX: usize = 256;
/// How often the drain wakes when it has not been nudged sooner.
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(500);
/// Rounds a single [`flush`] runs before yielding, so a producer that outruns
/// the drain cannot keep one call (or the shutdown flush) spinning forever.
const MAX_ROUNDS_PER_FLUSH: usize = 16;
/// Concurrent role resolutions inside one batch.
const ROLE_LOOKUP_CONCURRENCY: usize = 8;
/// After the first drop, log every this-many-th one.
const DROP_LOG_EVERY: u64 = 1000;

/// A navigation to record. Roles are resolved by the drain, not stored here.
pub struct ViewJob {
    pub app: entity::apps::Model,
    pub user_id: Uuid,
    pub user_email: Option<String>,
    pub session_id: Uuid,
    pub referrer: Option<String>,
    pub user_agent_class: String,
    pub source: String,
    pub environment: String,
    /// The time of the navigation, not of the write.
    pub viewed_at: DateTime<FixedOffset>,
}

/// A platform-beacon event to record. Validated by the drain.
pub struct EventJob {
    pub app_id: Uuid,
    pub user_id: Uuid,
    pub user_email: String,
    pub session_id: Uuid,
    pub event_name: String,
    pub payload: serde_json::Value,
    pub environment: String,
    pub occurred_at: DateTime<FixedOffset>,
}

pub enum UsageJob {
    View(Box<ViewJob>),
    Event(Box<EventJob>),
}

/// The channel, its single consumer, and the drop counter. Constructed with a
/// capacity so tests can exercise overflow on a private instance; the process
/// uses the one in [`QUEUE`].
struct Queue {
    tx: mpsc::Sender<UsageJob>,
    rx: Mutex<mpsc::Receiver<UsageJob>>,
    wake: Notify,
    dropped: AtomicU64,
    lost: AtomicU64,
    written: AtomicU64,
}

impl Queue {
    fn with_capacity(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self {
            tx,
            rx: Mutex::new(rx),
            wake: Notify::new(),
            dropped: AtomicU64::new(0),
            lost: AtomicU64::new(0),
            written: AtomicU64::new(0),
        }
    }

    /// Never blocks. `true` when the job was accepted.
    fn push(&self, job: UsageJob) -> bool {
        match self.tx.try_send(job) {
            Ok(()) => {
                let queued = self.tx.max_capacity() - self.tx.capacity();
                if queued >= BATCH_MAX {
                    self.wake.notify_one();
                }
                true
            }
            Err(_) => {
                let n = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n % DROP_LOG_EVERY == 0 {
                    tracing::warn!(
                        dropped_total = n,
                        capacity = self.tx.max_capacity(),
                        "custom-app usage queue full; dropping usage rows"
                    );
                }
                false
            }
        }
    }
}

static QUEUE: LazyLock<Queue> = LazyLock::new(|| Queue::with_capacity(CAPACITY));

/// Hand a job to the drain. Never blocks, never fails the caller.
pub fn enqueue(job: UsageJob) {
    QUEUE.push(job);
}

/// Rows dropped because the queue was full, since process start.
pub fn dropped_total() -> u64 {
    QUEUE.dropped.load(Ordering::Relaxed)
}

/// Rows that were taken off the queue but could not be written (no database, or
/// a row the database refused), since process start.
pub fn lost_total() -> u64 {
    QUEUE.lost.load(Ordering::Relaxed)
}

/// Rows written by the drain, since process start.
pub fn written_total() -> u64 {
    QUEUE.written.load(Ordering::Relaxed)
}

/// One multi-row insert; on failure, row by row so one bad row (an app deleted
/// since the job was queued trips the foreign key) costs only itself.
macro_rules! insert_all {
    ($db:expr, $entity:ty, $models:expr, $what:literal) => {{
        let models = $models;
        if !models.is_empty() {
            let n = models.len() as u64;
            match <$entity>::insert_many(models.clone())
                .exec_without_returning($db)
                .await
            {
                Ok(_) => {
                    QUEUE.written.fetch_add(n, Ordering::Relaxed);
                }
                Err(batch_err) => {
                    tracing::warn!(
                        rows = n,
                        "custom-app usage drain: {} batch insert failed, retrying row by row: {batch_err}",
                        $what
                    );
                    for model in models {
                        match model.insert($db).await {
                            Ok(_) => {
                                QUEUE.written.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                QUEUE.lost.fetch_add(1, Ordering::Relaxed);
                                tracing::warn!(
                                    "custom-app usage drain: {} insert failed: {e}",
                                    $what
                                );
                            }
                        }
                    }
                }
            }
        }
    }};
}

/// Start the one drain task. The returned handle finishes after the final
/// flush, so it can be registered as a shutdown hook.
///
/// Bound to a tagged custom-app hub: the rows carry viewer emails and
/// tenant-authored payloads, and there is no request hub to inherit.
pub fn spawn_usage_drain(shutdown: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(
        async move {
            let mut tick = tokio::time::interval(FLUSH_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut report = ShedReport::default();
            loop {
                tokio::select! {
                    _ = tick.tick() => {}
                    _ = QUEUE.wake.notified() => {}
                    _ = shutdown.cancelled() => {
                        flush().await;
                        report.emit(true);
                        break;
                    }
                }
                flush().await;
                report.emit(false);
            }
        }
        .bind_hub(crate::server::api::middlewares::sentry_surface::custom_app_hub()),
    )
}

/// How often the drain reports rows it has shed, at most.
const SHED_REPORT_EVERY: Duration = Duration::from_secs(60);

/// The only signal that usage rows are being lost: the counters are not on a
/// metrics endpoint (the serve process has none of its own for this), so the
/// drain says so in the log — one `warn!` per [`SHED_REPORT_EVERY`] while the
/// totals keep growing, and once more at shutdown. Silent while nothing is shed.
#[derive(Default)]
struct ShedReport {
    last: (u64, u64),
    at: Option<std::time::Instant>,
}

impl ShedReport {
    fn emit(&mut self, force: bool) {
        let now = (dropped_total(), lost_total());
        if now == self.last {
            return;
        }
        if !force && self.at.is_some_and(|at| at.elapsed() < SHED_REPORT_EVERY) {
            return;
        }
        tracing::warn!(
            dropped_total = now.0,
            lost_total = now.1,
            written_total = written_total(),
            dropped_since_last = now.0 - self.last.0,
            lost_since_last = now.1 - self.last.1,
            "custom-app usage rows were shed (queue full, or the database refused them)"
        );
        self.last = now;
        self.at = Some(std::time::Instant::now());
    }
}

/// Write everything queued now and return how many jobs were taken.
///
/// This is both the drain's body and the **test flush hook**: it runs inline
/// whether or not [`spawn_usage_drain`] was ever called, and — because it holds
/// the consumer lock across the whole write — it also waits for a batch the
/// background task has already taken off the queue.
pub async fn flush() -> usize {
    let mut rx = QUEUE.rx.lock().await;
    let mut total = 0;
    for _ in 0..MAX_ROUNDS_PER_FLUSH {
        let batch = take_batch(&mut rx);
        let taken = batch.len();
        if taken == 0 {
            break;
        }
        write_batch(batch).await;
        total += taken;
        if taken < BATCH_MAX {
            break;
        }
    }
    total
}

fn take_batch(rx: &mut mpsc::Receiver<UsageJob>) -> Vec<UsageJob> {
    let mut batch = Vec::new();
    while batch.len() < BATCH_MAX {
        match rx.try_recv() {
            Ok(job) => batch.push(job),
            Err(_) => break,
        }
    }
    batch
}

async fn write_batch(batch: Vec<UsageJob>) {
    let mut views = Vec::new();
    let mut events = Vec::new();
    for job in batch {
        match job {
            UsageJob::View(v) => views.push(*v),
            UsageJob::Event(e) => events.push(*e),
        }
    }
    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            QUEUE
                .lost
                .fetch_add((views.len() + events.len()) as u64, Ordering::Relaxed);
            tracing::warn!("custom-app usage drain: DB connect failed; dropping batch: {e}");
            return;
        }
    };
    write_views(&db, views).await;
    write_events(&db, events).await;
}

type Roles = (Option<String>, Option<String>);

async fn write_views(db: &DatabaseConnection, views: Vec<ViewJob>) {
    if views.is_empty() {
        return;
    }
    let roles = resolve_roles(db, &views).await;
    let models: Vec<custom_app_view_event::ActiveModel> = views
        .into_iter()
        .map(|v| {
            let (app_role, org_role) = roles
                .get(&(v.app.id, v.user_id))
                .cloned()
                .unwrap_or((None, None));
            custom_app_view_event::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                app_id: ActiveValue::Set(v.app.id),
                user_id: ActiveValue::Set(v.user_id),
                user_email: ActiveValue::Set(v.user_email.unwrap_or_default()),
                session_id: ActiveValue::Set(v.session_id),
                viewed_at: ActiveValue::Set(v.viewed_at),
                referrer: ActiveValue::Set(v.referrer),
                user_agent_class: ActiveValue::Set(v.user_agent_class),
                source: ActiveValue::Set(v.source),
                app_role: ActiveValue::Set(app_role),
                org_role: ActiveValue::Set(org_role),
                environment: ActiveValue::Set(v.environment),
            }
        })
        .collect();
    insert_all!(db, custom_app_view_event::Entity, models, "view");
}

/// One role lookup per distinct `(app, viewer)` in the batch: the answer cannot
/// differ within the half-second a batch spans, and a person reloading a page
/// would otherwise pay for the full fact load each time.
async fn resolve_roles(db: &DatabaseConnection, views: &[ViewJob]) -> HashMap<(Uuid, Uuid), Roles> {
    let mut distinct: HashMap<(Uuid, Uuid), &ViewJob> = HashMap::new();
    for v in views {
        distinct.entry((v.app.id, v.user_id)).or_insert(v);
    }
    let distinct: Vec<&ViewJob> = distinct.into_values().collect();
    let mut resolved = HashMap::with_capacity(distinct.len());
    for group in distinct.chunks(ROLE_LOOKUP_CONCURRENCY) {
        let answers = futures::future::join_all(group.iter().map(|v| resolve_one(db, v))).await;
        resolved.extend(answers);
    }
    resolved
}

/// A named fn, not a closure, so the future's lifetimes stay nameable (a
/// borrowing async closure here trips "Send is not general enough").
async fn resolve_one(db: &DatabaseConnection, v: &ViewJob) -> ((Uuid, Uuid), Roles) {
    // A label for the activity log, not a decision — see `resolve_view_roles`.
    let viewer = oxy_server_authz::Caller::without_credential(
        v.user_id,
        v.user_email.as_deref().unwrap_or(""),
    );
    (
        (v.app.id, v.user_id),
        resolve_view_roles(db, &v.app, &viewer).await,
    )
}

async fn write_events(db: &DatabaseConnection, events: Vec<EventJob>) {
    let models: Vec<custom_app_event::ActiveModel> = events
        .into_iter()
        .filter_map(|e| {
            let (app_id, name) = (e.app_id, e.event_name.clone());
            build_event_model(
                e.app_id,
                e.user_id,
                e.user_email,
                e.session_id,
                e.event_name,
                e.payload,
                &e.environment,
                e.occurred_at,
            )
            .map_err(|err| {
                tracing::warn!(%app_id, event_name = %name, "custom-app usage drain: rejected event: {err}");
                QUEUE.lost.fetch_add(1, Ordering::Relaxed);
            })
            .ok()
        })
        .collect();
    insert_all!(db, custom_app_event::Entity, models, "event");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(n: usize) -> UsageJob {
        UsageJob::Event(Box::new(EventJob {
            app_id: Uuid::nil(),
            user_id: Uuid::nil(),
            user_email: String::new(),
            session_id: Uuid::nil(),
            event_name: format!("oxy-test-{n}"),
            payload: serde_json::json!({}),
            environment: "production".into(),
            occurred_at: chrono::Utc::now().fixed_offset(),
        }))
    }

    fn name(job: &UsageJob) -> &str {
        match job {
            UsageJob::Event(e) => &e.event_name,
            UsageJob::View(_) => "view",
        }
    }

    /// The contract the request path relies on: a full queue refuses the job
    /// immediately and counts it. `push` is synchronous, so "did not block" is
    /// "returned at all" — a blocking send would hang this test.
    #[test]
    fn a_full_queue_drops_and_counts_instead_of_blocking() {
        let q = Queue::with_capacity(2);
        assert!(q.push(event(0)));
        assert!(q.push(event(1)));
        assert!(!q.push(event(2)));
        assert!(!q.push(event(3)));
        assert_eq!(q.dropped.load(Ordering::Relaxed), 2);
    }

    /// Shedding the overflow must keep the oldest rows, in order — the queue
    /// drops new work, it does not reorder or evict what it already accepted.
    #[tokio::test]
    async fn a_drain_round_takes_accepted_jobs_in_order_and_frees_room() {
        let q = Queue::with_capacity(3);
        for i in 0..5 {
            q.push(event(i));
        }
        let mut rx = q.rx.lock().await;
        let batch = take_batch(&mut rx);
        let names: Vec<_> = batch.iter().map(name).collect();
        assert_eq!(names, ["oxy-test-0", "oxy-test-1", "oxy-test-2"]);
        drop(rx);
        assert!(q.push(event(9)), "a drained queue accepts work again");
    }

    #[tokio::test]
    async fn a_round_is_capped_at_the_batch_size() {
        let q = Queue::with_capacity(BATCH_MAX * 2);
        for i in 0..BATCH_MAX + 10 {
            q.push(event(i));
        }
        let mut rx = q.rx.lock().await;
        assert_eq!(take_batch(&mut rx).len(), BATCH_MAX);
        assert_eq!(take_batch(&mut rx).len(), 10);
        assert!(take_batch(&mut rx).is_empty());
    }

    /// A producer reaching a batch's worth wakes the drain rather than leaving
    /// it to the tick.
    #[tokio::test]
    async fn reaching_a_batch_wakes_the_drain() {
        let q = Queue::with_capacity(BATCH_MAX * 2);
        for i in 0..BATCH_MAX {
            q.push(event(i));
        }
        tokio::time::timeout(Duration::from_millis(200), q.wake.notified())
            .await
            .expect("the BATCH_MAX-th push must notify");
    }
}
