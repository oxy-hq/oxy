//! A live tail of one app environment's function invocations:
//! `GET /api/customer-apps/{id}/environments/{env}/tail` (SSE), for the
//! sandbox loop (`internal-docs/custom-app-sandboxes.md` §4 "No log tail").
//!
//! **Status: half-built.** What is here is the core the route stands on —
//! the event model and its NOTIFY wire form ([`event`]) and the bounded
//! in-process subscriber registry ([`bus`]). The listener, the watcher table,
//! the per-invocation follower, the handler and its mount, and `oxyc logs
//! --follow` are not written yet; the plan below is what they must do.
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
//!   transaction's commit on a database-wide lock.
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
//! [`bus::Frame::Dropped`] marker — the buffer never grows.

pub mod bus;
pub mod event;
