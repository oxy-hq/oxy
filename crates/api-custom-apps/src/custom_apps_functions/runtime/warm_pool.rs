//! A small pool of isolates that have never run anything, built ahead of
//! demand.
//!
//! Starting an invocation used to mean creating an OS thread, building its
//! Tokio runtime and creating the V8 isolate from the bootstrap snapshot, all
//! on the request path. This pool does that work before the request arrives.
//!
//! ## What an entry is
//!
//! One dedicated thread that has already built its runtime and its isolate
//! and is waiting for exactly one [`Job`]. An invocation takes an entry, sends
//! it the job, and the thread runs it exactly as a thread built on the request
//! path would. When the job ends the thread returns, which drops the runtime
//! and the isolate. Nothing is handed back to the pool.
//!
//! ## Why isolates are not reused
//!
//! A `JsRuntime` cannot safely serve a second invocation: the tenant's module
//! and the bootstrap script share one global scope and `deno_core` has no
//! per-run realm to discard, so whatever one invocation leaves on `globalThis`
//! the next would see. An entry is therefore used once, and the pool's only
//! job is to have the next one ready.
//!
//! ## Refill
//!
//! One thread for the life of the process keeps the pool at its size: it
//! waits until fewer than `size` entries are ready, starts one entry's thread,
//! and waits for that build to finish before starting the next. Building one
//! at a time bounds the CPU a refill takes from the invocations it is for. A
//! build that fails is retried after a pause. The thread starts at the first
//! [`WarmPool::take`], so a pool of size 0 starts nothing.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Once, OnceLock, PoisonError};
use std::time::Duration;

use tokio::sync::oneshot;

use super::engine::{ActiveEngine, IsolateConfig, IsolateEngine};
use super::{
    BOOTSTRAP_JS, FnResponse, IsolateRun, RuntimeError, execute_on_engine, heap_limit_bytes,
    spawn_cold,
};

/// How many never-used isolates each process keeps ready. `0` turns the pool
/// off: every invocation builds its own thread and isolate.
pub const WARM_ISOLATES_ENV: &str = "OXY_FUNCTION_WARM_ISOLATES";

/// Two covers a burst of two arriving together without the second paying for
/// a cold start, and costs two idle isolates of memory.
const DEFAULT_WARM_ISOLATES: usize = 2;

/// How long the refill thread waits after a failed build before trying again.
const REFILL_RETRY_PAUSE: Duration = Duration::from_secs(1);

/// Everything one invocation hands to the thread that will run it.
pub(super) struct Job {
    pub run: IsolateRun,
    pub handle_tx: oneshot::Sender<<ActiveEngine as IsolateEngine>::Terminate>,
    pub done_tx: oneshot::Sender<Result<FnResponse, RuntimeError>>,
    /// Sentry hubs are per thread: this carries the invocation's hub onto the
    /// isolate thread.
    pub hub: Arc<sentry::Hub>,
    /// Parentless on purpose; see `run_in` for why.
    pub span: tracing::Span,
}

/// A waiting thread: its identity and the one place it can be sent a job.
struct Slot {
    id: u64,
    job_tx: oneshot::Sender<Job>,
}

#[derive(Default)]
struct State {
    ready: VecDeque<Slot>,
    /// A build is in progress, so the refill thread starts no other.
    building: bool,
    last_build_ok: bool,
    next_id: u64,
}

pub(super) struct WarmPool {
    size: usize,
    state: Mutex<State>,
    changed: Condvar,
    refill_started: Once,
    /// The thread and isolate of every job a slot received, for the tests that
    /// assert none was served twice.
    #[cfg(test)]
    pub(super) served: Mutex<Vec<(u64, std::thread::ThreadId)>>,
}

/// The pool this process uses, sized from [`WARM_ISOLATES_ENV`] on first use.
pub(super) fn global() -> &'static Arc<WarmPool> {
    static POOL: OnceLock<Arc<WarmPool>> = OnceLock::new();
    POOL.get_or_init(|| WarmPool::new(size_from_env(std::env::var(WARM_ISOLATES_ENV).ok())))
}

/// An unset or unparsable value is the default, not zero: a typo must not
/// silently turn the pool off.
pub(super) fn size_from_env(value: Option<String>) -> usize {
    value
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_WARM_ISOLATES)
}

impl WarmPool {
    pub(super) fn new(size: usize) -> Arc<Self> {
        Arc::new(Self {
            size,
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            refill_started: Once::new(),
            #[cfg(test)]
            served: Mutex::new(Vec::new()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Pop a ready entry and wake the refill thread to replace it. `None` when
    /// none is ready, and always when the pool is off — without touching the
    /// state or starting the refill thread.
    fn take(self: &Arc<Self>) -> Option<Slot> {
        if self.size == 0 {
            return None;
        }
        self.refill_started.call_once(|| self.start_refill_thread());
        let slot = self.lock().ready.pop_front();
        self.changed.notify_all();
        slot
    }

    fn start_refill_thread(self: &Arc<Self>) {
        let pool = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("oxy-function-refill".into())
            .spawn(move || pool.refill_forever());
        if let Err(error) = spawned {
            tracing::warn!(
                target: "oxy.custom_app.warm_pool",
                %error,
                "could not start the warm isolate refill thread; invocations build their own isolates"
            );
        }
    }

    fn refill_forever(self: &Arc<Self>) {
        loop {
            let id = self.wait_for_room_and_reserve();
            if let Err(error) = spawn_warm(Arc::clone(self), id) {
                tracing::warn!(target: "oxy.custom_app.warm_pool", %error, "could not start a warm isolate thread");
                self.finish_build(None);
            }
            if !self.wait_for_build() {
                std::thread::sleep(REFILL_RETRY_PAUSE);
            }
        }
    }

    /// Block until fewer than `size` entries are ready, then mark a build as
    /// started and return the id the new entry will carry.
    fn wait_for_room_and_reserve(&self) -> u64 {
        let mut state = self.lock();
        while state.ready.len() >= self.size {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.building = true;
        state.next_id += 1;
        state.next_id
    }

    /// Block until the build in progress ends; whether it produced an entry.
    fn wait_for_build(&self) -> bool {
        let mut state = self.lock();
        while state.building {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.last_build_ok
    }

    /// End the build in progress, adding `slot` to the pool when it made one.
    fn finish_build(&self, slot: Option<Slot>) {
        let mut state = self.lock();
        state.last_build_ok = slot.is_some();
        state.ready.extend(slot);
        state.building = false;
        drop(state);
        self.changed.notify_all();
    }

    /// Entries waiting for a job.
    #[cfg(test)]
    pub(super) fn ready_len(&self) -> usize {
        self.lock().ready.len()
    }

    /// Whether anything in this pool ever ran: its refill thread, or a build.
    #[cfg(test)]
    pub(super) fn was_touched(&self) -> bool {
        let state = self.lock();
        self.refill_started.is_completed() || state.next_id > 0 || !state.ready.is_empty()
    }
}

/// Run `job` on a thread that already holds a built isolate when the pool has
/// one, and on a thread built now otherwise.
pub(super) fn start_isolate(pool: &Arc<WarmPool>, job: Job) -> Result<(), RuntimeError> {
    let job = match pool.take() {
        Some(slot) => match slot.job_tx.send(job) {
            Ok(()) => return Ok(()),
            // The thread is gone (it panicked after reporting itself ready).
            Err(job) => job,
        },
        None => job,
    };
    spawn_cold(job)
        .map_err(|e| RuntimeError::Internal(format!("failed to spawn isolate thread: {e}")))
}

/// Reports the build as failed unless [`BuildReport::ready`] was called, so
/// no exit path of the warm thread — an error, a panic — leaves the refill
/// thread waiting on a build that ended.
struct BuildReport {
    pool: Arc<WarmPool>,
    reported: bool,
}

impl BuildReport {
    fn ready(mut self, slot: Slot) {
        self.reported = true;
        self.pool.finish_build(Some(slot));
    }
}

impl Drop for BuildReport {
    fn drop(&mut self) {
        if !self.reported {
            self.pool.finish_build(None);
        }
    }
}

fn spawn_warm(pool: Arc<WarmPool>, id: u64) -> std::io::Result<()> {
    std::thread::Builder::new()
        // The same name as a thread built on the request path.
        .name("oxy-function".into())
        .spawn(move || warm_thread(pool, id))
        .map(|_| ())
}

/// Body of one pool entry: build the runtime and the isolate, announce
/// ready, wait for the one job, run it, return.
fn warm_thread(pool: Arc<WarmPool>, id: u64) {
    let report = BuildReport {
        pool: Arc::clone(&pool),
        reported: false,
    };
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            tracing::warn!(target: "oxy.custom_app.warm_pool", %error, "could not build a warm isolate runtime");
            return;
        }
    };
    let local = tokio::task::LocalSet::new();
    let created = local.block_on(&rt, async {
        ActiveEngine::create(IsolateConfig {
            heap_limit: heap_limit_bytes(),
            bootstrap: BOOTSTRAP_JS,
        })
    });
    let engine = match created {
        Ok(engine) => engine,
        Err(error) => {
            tracing::warn!(target: "oxy.custom_app.warm_pool", %error, "could not build a warm isolate");
            return;
        }
    };
    let (job_tx, job_rx) = oneshot::channel::<Job>();
    report.ready(Slot { id, job_tx });
    // Outside any runtime context, so a blocking receive is allowed. An error
    // means the slot was dropped unused, and the thread simply ends.
    let Ok(job) = job_rx.blocking_recv() else {
        return;
    };
    #[cfg(test)]
    pool.served
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((id, std::thread::current().id()));
    run_job(&rt, &local, engine, job);
}

/// The part of a thread's life that is the same whether the isolate was built
/// ahead of time or just now: counted as live, running under the invocation's
/// hub and span.
fn run_job(
    rt: &tokio::runtime::Runtime,
    local: &tokio::task::LocalSet,
    engine: ActiveEngine,
    job: Job,
) {
    // Counted from the job, not from the thread: an idle entry is not a live
    // isolate for the gauge that sizes memory against concurrency.
    let _isolate_guard = oxy_telemetry::metrics::sources::IsolateGuard::enter();
    let Job {
        run,
        handle_tx,
        done_tx,
        hub,
        span,
    } = job;
    sentry::Hub::run(hub, || {
        let _span_guard = span.enter();
        local.block_on(rt, async move {
            let result = execute_on_engine(engine, run, handle_tx).await;
            let _ = done_tx.send(result);
        });
    })
}
