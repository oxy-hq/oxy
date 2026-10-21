//! The producing side: one [`Follower`] per function invocation, wherever the
//! isolate runs — the replica that took a `/fn` call, or a worker draining a
//! check run or a schedule (`custom_apps_functions::run_with_runtime`).
//!
//! While the invocation's `(app, environment)` is watched ([`gate`]), the
//! follower sends a `start`, then every [`FOLLOW_EVERY`] the lines the
//! isolate has appended to its log buffer since the last send, and at the end
//! the rest of the buffer and a `finish`. Unwatched, it sends nothing: its
//! cursor stays put, so a tail opened mid-run is sent the run's lines from the
//! first.
//!
//! The sends are the follower's own task's, never the invocation's: a
//! `NOTIFY` that waits on the pool delays a line, not a function's response.
//! What the end of the run has to do in line is one lock and a channel send.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use sentry::SentryFutureExt;
use tokio::sync::mpsc;
use uuid::Uuid;

pub use super::event::Outcome;
use super::event::{self, TailEvent};
use super::gate;
use crate::custom_apps_functions::LogLine;

/// How often a running invocation's new lines are sent.
pub const FOLLOW_EVERY: Duration = Duration::from_millis(250);

/// Which invocation a follower follows.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub app_id: Uuid,
    /// The environment's name: `production`, `staging` or `dev-<handle>`.
    pub environment: String,
    pub invocation_id: Uuid,
    pub function: String,
    pub mode: String,
    pub build_id: Uuid,
    pub request_id: Option<Uuid>,
}

#[derive(Default)]
struct Cursor {
    started: bool,
    /// How many of the buffer's lines have been taken for sending.
    sent: usize,
    finished: bool,
}

/// What the follower's task and the invocation's end share. Lines are taken
/// under one lock, by whichever of the two gets there, so none is sent twice.
struct Shared {
    invocation: Invocation,
    logs: Arc<Mutex<Vec<LogLine>>>,
    cursor: Mutex<Cursor>,
}

impl Shared {
    /// The events not yet taken: a `start` if none was, the new lines, and
    /// `finish` when the run has ended. Nothing while the pair is unwatched.
    fn take(&self, watched: bool, ending: Option<&Outcome>) -> Vec<TailEvent> {
        let mut cursor = self.cursor.lock().unwrap_or_else(|p| p.into_inner());
        if cursor.finished {
            return Vec::new();
        }
        cursor.finished = ending.is_some();
        if !watched {
            return Vec::new();
        }
        let inv = &self.invocation;
        let mut events = Vec::new();
        if !cursor.started {
            cursor.started = true;
            events.push(TailEvent::Start {
                invocation_id: inv.invocation_id,
                function: inv.function.clone(),
                mode: inv.mode.clone(),
                build_id: inv.build_id,
                request_id: inv.request_id,
            });
        }
        let logs = self.logs.lock().unwrap_or_else(|p| p.into_inner());
        let from = cursor.sent.min(logs.len());
        for (offset, line) in logs[from..].iter().enumerate() {
            let seq = u32::try_from(from + offset + 1).unwrap_or(u32::MAX);
            events.push(TailEvent::log(
                inv.invocation_id,
                &inv.function,
                seq,
                &line.level,
                &line.message,
            ));
        }
        cursor.sent = logs.len();
        if let Some(outcome) = ending {
            events.push(TailEvent::finish(
                inv.invocation_id,
                &inv.function,
                &inv.mode,
                outcome,
            ));
        }
        events
    }
}

/// One invocation's follower. [`Follower::finish`] it when the run returns;
/// dropped without that (the run was cancelled), it sends nothing more.
pub struct Follower {
    shared: Arc<Shared>,
    batches: mpsc::UnboundedSender<Vec<TailEvent>>,
}

impl Follower {
    /// Start following `invocation`, whose isolate appends to `logs`. Reads
    /// the watched set first ([`gate::is_watched`]: a set lookup, or one
    /// read of the watcher table a second per process), so a run that
    /// starts watched sends its `start` before the isolate does anything.
    pub async fn start(
        db: &DatabaseConnection,
        invocation: Invocation,
        logs: Arc<Mutex<Vec<LogLine>>>,
    ) -> Self {
        let watched = gate::is_watched(db, invocation.app_id, &invocation.environment).await;
        let shared = Arc::new(Shared {
            invocation,
            logs,
            cursor: Mutex::default(),
        });
        let (batches, queued) = mpsc::unbounded_channel();
        let first = shared.take(watched, None);
        if !first.is_empty() {
            let _ = batches.send(first);
        }
        tokio::spawn(
            follow(db.clone(), Arc::clone(&shared), queued)
                // Follows one invocation, so it carries that invocation's hub.
                .bind_hub(sentry::Hub::current()),
        );
        Self { shared, batches }
    }

    /// The run has returned: queue the rest of its lines and its `finish`,
    /// if its pair is watched. Never awaits — the buffer is read here, before
    /// the caller drains it, and the sends are the follower's.
    pub fn finish(self, outcome: &Outcome) {
        let inv = &self.shared.invocation;
        let watched = gate::is_watched_now(inv.app_id, &inv.environment);
        let last = self.shared.take(watched, Some(outcome));
        if !last.is_empty() {
            let _ = self.batches.send(last);
        }
    }
}

/// The follower's task: send queued batches in order, and every
/// [`FOLLOW_EVERY`] take what is new while the pair is watched. Ends once
/// the [`Follower`] is gone and its last batch is sent.
async fn follow(
    db: DatabaseConnection,
    shared: Arc<Shared>,
    mut queued: mpsc::UnboundedReceiver<Vec<TailEvent>>,
) {
    let (app_id, environment) = (
        shared.invocation.app_id,
        shared.invocation.environment.clone(),
    );
    let mut every = tokio::time::interval(FOLLOW_EVERY);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    every.tick().await;
    loop {
        tokio::select! {
            // What is queued goes first: a tick taken after the finish was
            // queued would find nothing, but one sent ahead of it would
            // reorder lines.
            biased;
            batch = queued.recv() => match batch {
                Some(events) => notify(&db, app_id, &environment, events).await,
                None => return,
            },
            _ = every.tick() => {
                let watched = gate::is_watched(&db, app_id, &environment).await;
                let events = shared.take(watched, None);
                if !events.is_empty() {
                    notify(&db, app_id, &environment, events).await;
                }
            }
        }
    }
}

/// `NOTIFY` `events` as envelopes under the payload limit, in order. Best
/// effort: a failed send loses those lines from the tail and nothing else —
/// the invocation, its row and its persisted logs are untouched.
pub async fn notify(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    environment: &str,
    events: Vec<TailEvent>,
) {
    for payload in event::pack(app_id, environment, events) {
        let sent = db
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT pg_notify($1, $2)",
                [event::NOTIFY_CHANNEL.into(), payload.into()],
            ))
            .await;
        if let Err(e) = sent {
            tracing::warn!(%app_id, environment, error = %e, "tail: pg_notify failed; lines dropped from the tail");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(lines: &[&str]) -> Shared {
        Shared {
            invocation: Invocation {
                app_id: Uuid::new_v4(),
                environment: "dev-a1".into(),
                invocation_id: Uuid::new_v4(),
                function: "probe".into(),
                mode: "route".into(),
                build_id: Uuid::new_v4(),
                request_id: None,
            },
            logs: Arc::new(Mutex::new(lines.iter().map(|m| line(m)).collect())),
            cursor: Mutex::default(),
        }
    }

    fn line(message: &str) -> LogLine {
        LogLine {
            level: "info".into(),
            message: message.into(),
        }
    }

    fn outcome() -> Outcome {
        Outcome {
            status: "success".into(),
            failed: false,
            http_status: 200,
            duration_ms: 3,
            error: None,
        }
    }

    fn shape(events: &[TailEvent]) -> Vec<String> {
        events
            .iter()
            .map(|e| match e {
                TailEvent::Log { seq, message, .. } => format!("{seq}:{message}"),
                other => other.name().to_string(),
            })
            .collect()
    }

    /// Watched from the start: a `start`, each line once with its position,
    /// then the rest and a `finish` — and nothing after the finish.
    #[test]
    fn a_watched_run_sends_start_each_line_once_and_finish() {
        let s = shared(&["a", "b"]);
        assert_eq!(shape(&s.take(true, None)), ["start", "1:a", "2:b"]);
        assert!(s.take(true, None).is_empty(), "nothing new");
        s.logs.lock().unwrap().push(line("c"));
        assert_eq!(shape(&s.take(true, Some(&outcome()))), ["3:c", "finish"]);
        s.logs.lock().unwrap().push(line("late"));
        assert!(s.take(true, None).is_empty(), "nothing after the finish");
    }

    /// Unwatched, nothing is taken and the cursor stays put: a tail that
    /// starts watching mid-run gets the run's lines from the first.
    #[test]
    fn an_unwatched_run_sends_nothing_and_a_late_watch_gets_every_line() {
        let s = shared(&["a"]);
        assert!(s.take(false, None).is_empty());
        s.logs.lock().unwrap().push(line("b"));
        assert_eq!(shape(&s.take(true, None)), ["start", "1:a", "2:b"]);
    }

    /// A run that ends unwatched sends nothing, its finish included.
    #[test]
    fn a_run_that_ends_unwatched_sends_no_finish() {
        let s = shared(&["a"]);
        assert!(s.take(false, Some(&outcome())).is_empty());
        assert!(s.take(true, None).is_empty(), "finished");
    }
}
