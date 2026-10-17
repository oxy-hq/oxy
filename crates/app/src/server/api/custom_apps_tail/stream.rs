//! One tail stream, as the events it yields — `open`, then `start` / `log` /
//! `finish` as they arrive, `dropped` markers, and exactly one `done` — and
//! their SSE framing.
//!
//! [`Tail::open`] does everything that can fail before the response starts:
//! the listener attached, this process subscribed, the lease written. What
//! follows cannot fail the request, so every stream that started ends with a
//! `done` (`timeout`, `shutdown`, `access_revoked`, `reauthenticate`), or
//! with the client going away.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;

use axum::http::{HeaderValue, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, DbErr};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::bus::{self, Frame};
use super::listener::{self, Link};
use super::recheck::{REAUTHENTICATE_AFTER, RECHECK_EVERY, Recheck};
use super::{gate, resume, watchers};

/// How often an idle stream sends a comment, so proxies and clients keep it.
pub const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// What a stream is of, once admitted.
#[derive(Debug, Clone)]
pub struct Target {
    pub app_id: Uuid,
    /// The environment's name: `production`, `staging` or `dev-<handle>`.
    pub environment: String,
    /// Replay only invocations created from here on: a sandbox agent token
    /// sees its own sandbox, not an earlier one that had the name.
    pub since: Option<DateTime<Utc>>,
}

/// What the request asked of the stream, beside its target.
#[derive(Default)]
pub struct Asked {
    /// The `Last-Event-ID` a resume continues after.
    pub resume_after: Option<i64>,
    /// `?timeout=`: when the stream ends with `done {"reason":"timeout"}`.
    pub timeout: Option<Duration>,
    /// Asked every [`RECHECK_EVERY`]; `false` ends the stream with
    /// `done {"reason":"access_revoked"}`.
    pub recheck: Option<Recheck>,
}

/// Why a tail could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Unavailable(#[from] listener::Unavailable),
    #[error("registering the tail failed: {0}")]
    Register(#[from] DbErr),
}

/// One event before framing: its SSE name and its `data`.
#[derive(Debug, Clone, PartialEq)]
pub struct Out {
    pub name: &'static str,
    pub data: Value,
}

impl Out {
    fn new(name: &'static str, data: Value) -> Self {
        Self { name, data }
    }

    fn dropped(reason: &str, events: Option<u64>) -> Self {
        let mut data = json!({ "reason": reason });
        if let Some(events) = events {
            data["events"] = json!(events);
        }
        Self::new("dropped", data)
    }
}

/// Event ids: the stream's clock in milliseconds, strictly increasing, so a
/// `Last-Event-ID` is a point in time a resume can read outcomes after.
#[derive(Debug, Clone, Copy)]
pub struct EventIds {
    last: i64,
}

impl EventIds {
    pub fn after(last: i64) -> Self {
        Self { last }
    }

    pub fn next_at(&mut self, now_ms: i64) -> i64 {
        self.last = now_ms.max(self.last + 1);
        self.last
    }

    pub fn next(&mut self) -> i64 {
        self.next_at(Utc::now().timestamp_millis())
    }
}

/// A live tail.
pub struct Tail {
    db: DatabaseConnection,
    target: Target,
    resume_after: Option<i64>,
    deadline: Option<tokio::time::Instant>,
    /// When this request has streamed long enough that its client must
    /// authenticate again ([`REAUTHENTICATE_AFTER`]).
    reauthenticate_at: tokio::time::Instant,
    subscription: bus::Subscription,
    link: watch::Receiver<Link>,
    epoch: u64,
    link_open: bool,
    shutdown: CancellationToken,
    recheck: Option<Recheck>,
    recheck_every: tokio::time::Interval,
    pending: VecDeque<Out>,
    opened: bool,
    ended: bool,
    _watch: watchers::Watch,
}

impl Tail {
    /// Attach the listener, subscribe, and write the lease — in that order,
    /// so nothing a producer sends once it sees the lease can be missed here.
    pub async fn open(
        db: DatabaseConnection,
        target: Target,
        asked: Asked,
    ) -> Result<Self, OpenError> {
        let link = listener::ensure_started().await?;
        let epoch = link.borrow().epoch;
        let subscription = bus::subscribe(target.app_id, &target.environment);
        let watch = watchers::Watch::register(&db, target.app_id, &target.environment).await?;
        let now = tokio::time::Instant::now();
        let mut recheck_every = tokio::time::interval_at(now + RECHECK_EVERY, RECHECK_EVERY);
        recheck_every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Ok(Self {
            db,
            resume_after: asked.resume_after,
            deadline: asked.timeout.map(|t| now + t),
            reauthenticate_at: now + REAUTHENTICATE_AFTER,
            subscription,
            link,
            epoch,
            link_open: true,
            shutdown: listener::shutdown_signal().unwrap_or_default(),
            recheck: asked.recheck,
            recheck_every,
            pending: VecDeque::new(),
            opened: false,
            ended: false,
            _watch: watch,
            target,
        })
    }

    /// The next event, or `None` once `done` has been yielded.
    pub async fn next(&mut self) -> Option<Out> {
        if let Some(out) = self.pending.pop_front() {
            return Some(out);
        }
        if self.ended {
            return None;
        }
        if !self.opened {
            self.opened = true;
            self.settle().await;
            return self.pending.pop_front();
        }
        Some(self.live().await)
    }

    /// Wait until every producer will see the lease ([`gate::SETTLE`]), then
    /// queue `open` and, on a resume, the outcomes the reader missed.
    async fn settle(&mut self) {
        tokio::time::sleep(gate::SETTLE).await;
        self.pending.push_back(Out::new(
            "open",
            json!({
                "app_id": self.target.app_id,
                "environment": self.target.environment,
                "resumed": self.resume_after.is_some(),
            }),
        ));
        if let Some(after) = self.resume_after {
            self.replay(after).await;
        }
    }

    async fn replay(&mut self, after: i64) {
        let t = &self.target;
        match resume::replay(&self.db, t.app_id, &t.environment, after, t.since).await {
            Ok(replay) => {
                if replay.left_out > 0 {
                    let left_out = replay.left_out as u64;
                    self.pending
                        .push_back(Out::dropped("resume_limit", Some(left_out)));
                }
                for event in replay.outcomes {
                    self.pending
                        .push_back(Out::new(event.name(), event_data(&event)));
                }
            }
            Err(e) => {
                tracing::warn!(app_id = %t.app_id, error = %e, "tail: resume replay failed");
                self.pending.push_back(Out::dropped("resume_failed", None));
            }
        }
    }

    /// Wait for the next live thing: an event, a gap, or the end.
    async fn live(&mut self) -> Out {
        let deadline = self.deadline;
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => return self.end("shutdown"),
                _ = sleep_until(deadline) => return self.end("timeout"),
                _ = tokio::time::sleep_until(self.reauthenticate_at) => {
                    return self.end("reauthenticate");
                }
                _ = self.recheck_every.tick(), if self.recheck.is_some() => {
                    if !self.still_admitted().await {
                        return self.end("access_revoked");
                    }
                }
                changed = self.link.changed(), if self.link_open => match changed {
                    Ok(()) => {
                        let link = *self.link.borrow_and_update();
                        if link.attached && link.epoch != self.epoch {
                            self.epoch = link.epoch;
                            return Out::dropped("listener_reconnected", None);
                        }
                    }
                    Err(_) => self.link_open = false,
                },
                frame = self.subscription.next() => return match frame {
                    Frame::Event(event) => Out::new(event.name(), event_data(&event)),
                    Frame::Dropped { events } => Out::dropped("lagged", Some(events)),
                    Frame::Closed => self.end("closed"),
                },
            }
        }
    }

    async fn still_admitted(&self) -> bool {
        match &self.recheck {
            Some(recheck) => recheck().await,
            None => true,
        }
    }

    fn end(&mut self, reason: &str) -> Out {
        self.ended = true;
        Out::new("done", json!({ "reason": reason }))
    }

    /// The stream as an SSE response: never cached, never buffered by a
    /// proxy (`X-Accel-Buffering: no`), never compressed — the `/api` tree
    /// mounts no compression, and the custom-app tree's skips
    /// `text/event-stream`.
    pub fn into_sse(self, ids: EventIds) -> Response {
        let stream = futures::stream::unfold((self, ids), |(mut tail, mut ids)| async move {
            let out = tail.next().await?;
            let event = Event::default()
                .event(out.name)
                .id(ids.next().to_string())
                .data(out.data.to_string());
            Some((Ok::<_, Infallible>(event), (tail, ids)))
        });
        let mut response = Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
            .into_response();
        let headers = response.headers_mut();
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
        response
    }
}

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

fn event_data(event: &super::event::TailEvent) -> Value {
    serde_json::to_value(event).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids follow the clock, never repeat and never go back — a clock that
    /// stalls or steps back still yields a later id.
    #[test]
    fn event_ids_strictly_increase() {
        let mut ids = EventIds::after(1_000);
        assert_eq!(ids.next_at(5_000), 5_000);
        assert_eq!(ids.next_at(5_000), 5_001);
        assert_eq!(ids.next_at(4_000), 5_002);
        assert_eq!(ids.next_at(9_000), 9_000);
        // A resumed stream continues after the id it was given.
        assert_eq!(EventIds::after(10_000).next_at(9_000), 10_001);
    }

    /// A `dropped` marker says why, and how many when it knows.
    #[test]
    fn a_dropped_marker_carries_its_reason_and_count() {
        assert_eq!(
            Out::dropped("lagged", Some(4)).data,
            json!({ "reason": "lagged", "events": 4 })
        );
        assert_eq!(
            Out::dropped("listener_reconnected", None).data,
            json!({ "reason": "listener_reconnected" })
        );
    }
}
