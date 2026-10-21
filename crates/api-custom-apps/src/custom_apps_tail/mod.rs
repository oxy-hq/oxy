//! A live tail of one app environment's function invocations:
//! `GET /api/customer-apps/{id}/environments/{name}/tail` (SSE), for the
//! sandbox loop (`internal-docs/custom-app-sandboxes.md` §4 "Read-back", §5.4)
//! and `oxyc logs --follow`.
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`event`] | the events (`start` / `log` / `finish`) and their `NOTIFY` wire form |
//! | [`producer`] | one follower per invocation, wherever the isolate runs |
//! | [`gate`] | "is this pair watched?", read at most once a second per process |
//! | [`watchers`] | the `app_environment_tails` leases a stream holds |
//! | [`listener`] | this process's `LISTEN` connection, opened on the first tail |
//! | [`bus`] | this process's subscribers, a bounded ring per pair |
//! | [`stream`] | one stream's events and their SSE framing |
//! | [`resume`] | what a `Last-Event-ID` resume replays |
//! | [`admission`], [`recheck`], [`handler`] | who may tail what, asked again while it streams; the route |
//!
//! ## Fan-out: Postgres `LISTEN`/`NOTIFY`, gated by a watcher table
//!
//! A line is produced wherever the function runs — the replica that took the
//! `/fn` call, or a worker draining a check run — and the subscriber's SSE is
//! held by whichever replica took the `GET`. An in-process broadcast would
//! deliver only when the two are the same process, so it silently misses
//! most lines in production while passing every single-process test.
//!
//! - **Transport:** `pg_notify('oxy_app_tail', <envelope>)` from the producer
//!   on its pooled connection; one dedicated `LISTEN` connection per serving
//!   process (opened on the first tail, not at boot) fans each envelope out to
//!   this process's subscribers ([`bus::deliver_local`]). The same primitive
//!   chat (`chat::delivery`) and the task router already ride — no new
//!   infrastructure. Unlike chat, the payload **is** the content (a batch of
//!   lines, each cut to [`event::MAX_MESSAGE_BYTES`], the envelope under
//!   Postgres's 8000-byte limit): there is no durable store of lines to
//!   re-read when capture is off, which is the case this route exists for.
//! - **Gate:** producers `NOTIFY` only for an `(app, environment)` someone is
//!   watching. A subscriber registers a row (`app_environment_tails`: app,
//!   environment, `expires_at`, renewed while the stream lives — control
//!   plane, no content); each producing process reads the set of watched
//!   pairs at most once a second and checks it locally. Without the gate every
//!   production invocation would `NOTIFY`, and each `NOTIFY` serialises its
//!   transaction's commit on a database-wide lock. A stream says `open` only
//!   after [`gate::SETTLE`], so every call started after `open` is tailed.
//! - **Not guaranteed:** `NOTIFY` is fire-and-forget. A listener reconnect
//!   loses what was sent meanwhile (the stream says so with a `dropped`
//!   marker), and a resume replays invocation **outcomes** from
//!   `app_function_invocations`, not lines. Lines stay readable afterwards
//!   through `/logs` where capture is on.
//!
//! ## Bounds
//!
//! One broadcast ring of [`bus::SUBSCRIBER_BUFFER`] events per watched
//! `(app, environment)` in each process. A subscriber that falls further
//! behind loses the oldest events and is told how many, as a
//! [`bus::Frame::Dropped`] marker — the buffer never grows. A producer sends
//! at most the isolate's own capped log buffer, every
//! [`producer::FOLLOW_EVERY`], and only while its pair is watched.

pub mod admission;
pub mod bus;
pub mod event;
pub mod gate;
pub mod handler;
pub mod listener;
pub mod producer;
pub mod recheck;
pub mod resume;
pub mod stream;
pub mod watchers;
