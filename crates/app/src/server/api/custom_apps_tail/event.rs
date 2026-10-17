//! What a tail carries, and its wire form between processes.
//!
//! An invocation is a `start`, its `log` lines, and a `finish`. Between the
//! producing process and the subscriber's they travel as an [`Envelope`] in a
//! Postgres `NOTIFY` payload, which Postgres caps at 8000 bytes — so a line
//! is cut to [`MAX_MESSAGE_BYTES`] (and says so), and a batch is split across
//! as many payloads as it needs, in order ([`pack`]).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The one `NOTIFY` channel. The app and environment ride in the payload:
/// `LISTEN` takes an identifier, so a channel per environment would mean a
/// `LISTEN` issued at run time per environment anyone tails.
pub const NOTIFY_CHANNEL: &str = "oxy_app_tail";

/// A payload's budget, under Postgres's 8000-byte `NOTIFY` limit.
pub const MAX_PAYLOAD_BYTES: usize = 7_500;

/// The most of one line's message (or a finish's error) a tail carries.
pub const MAX_MESSAGE_BYTES: usize = 2_000;

/// One thing that happened in an invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TailEvent {
    Start {
        invocation_id: Uuid,
        function: String,
        mode: String,
        build_id: Uuid,
        request_id: Option<Uuid>,
    },
    Log {
        invocation_id: Uuid,
        function: String,
        /// The line's position in its invocation, from 1.
        seq: u32,
        level: String,
        message: String,
        /// The message was cut to fit; the whole line is in `/logs` where
        /// observability capture is on.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
    },
    Finish {
        invocation_id: Uuid,
        function: String,
        mode: String,
        status: String,
        /// The pager's rule (`failure_signal::Failure::of`), as the
        /// invocation listing reports it: read this, not `status`.
        failed: bool,
        http_status: u16,
        duration_ms: u64,
        error: Option<String>,
        /// Read back from the invocation's row by a resume
        /// (`Last-Event-ID`), not sent live: lines between the two streams
        /// are not replayed, and a reader may have seen this one already.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        replayed: bool,
    },
}

/// How an invocation ended, as its `finish` event reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub status: String,
    /// The pager's rule (`failure_signal::Failure::of`).
    pub failed: bool,
    pub http_status: u16,
    pub duration_ms: u64,
    pub error: Option<String>,
}

impl TailEvent {
    /// The SSE event name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Log { .. } => "log",
            Self::Finish { .. } => "finish",
        }
    }

    /// An invocation's end, its error cut to [`MAX_MESSAGE_BYTES`].
    pub fn finish(invocation_id: Uuid, function: &str, mode: &str, outcome: &Outcome) -> Self {
        Self::Finish {
            invocation_id,
            function: function.to_string(),
            mode: mode.to_string(),
            status: outcome.status.clone(),
            failed: outcome.failed,
            http_status: outcome.http_status,
            duration_ms: outcome.duration_ms,
            error: outcome
                .error
                .as_deref()
                .map(|error| cut(error, MAX_MESSAGE_BYTES).0),
            replayed: false,
        }
    }

    /// A log line, its message cut to [`MAX_MESSAGE_BYTES`].
    pub fn log(invocation_id: Uuid, function: &str, seq: u32, level: &str, message: &str) -> Self {
        let (message, truncated) = cut(message, MAX_MESSAGE_BYTES);
        Self::Log {
            invocation_id,
            function: function.to_string(),
            seq,
            level: level.to_string(),
            message,
            truncated,
        }
    }
}

/// One `NOTIFY` payload: the environment it happened in and a batch of
/// events, in order. Short keys: the budget is the payload's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(rename = "a")]
    pub app_id: Uuid,
    #[serde(rename = "e")]
    pub environment: String,
    #[serde(rename = "v")]
    pub events: Vec<TailEvent>,
}

/// `text` cut to at most `max` bytes on a character boundary, and whether
/// anything was cut.
fn cut(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_string(), false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

/// `event`, its message shrunk until it serialises inside `budget` bytes. A
/// message of control characters escapes to six bytes each, so the byte cut
/// in [`TailEvent::log`] alone does not guarantee a fit.
fn fit(event: TailEvent, budget: usize) -> TailEvent {
    let mut event = event;
    while serde_json::to_string(&event).map_or(0, |s| s.len()) > budget {
        match &mut event {
            TailEvent::Log {
                message, truncated, ..
            } if !message.is_empty() => {
                let (shorter, _) = cut(message, message.len() / 2);
                *message = shorter;
                *truncated = true;
            }
            TailEvent::Finish {
                error: Some(error), ..
            } if !error.is_empty() => {
                let (shorter, _) = cut(error, error.len() / 2);
                *error = shorter;
            }
            _ => break,
        }
    }
    event
}

/// `events` as `NOTIFY` payloads, each at most [`MAX_PAYLOAD_BYTES`], in
/// order. Every event lands in exactly one payload.
pub fn pack(app_id: Uuid, environment: &str, events: Vec<TailEvent>) -> Vec<String> {
    let envelope = |events: Vec<TailEvent>| Envelope {
        app_id,
        environment: environment.to_string(),
        events,
    };
    let overhead = serde_json::to_string(&envelope(Vec::new())).map_or(0, |s| s.len());
    // Each event after the first costs a comma.
    let budget = MAX_PAYLOAD_BYTES.saturating_sub(overhead + 1);
    let mut payloads = Vec::new();
    let mut batch = Vec::new();
    let mut used = 0;
    for event in events {
        let event = fit(event, budget);
        let size = serde_json::to_string(&event).map_or(0, |s| s.len()) + 1;
        if !batch.is_empty() && used + size > budget {
            payloads.push(std::mem::take(&mut batch));
            used = 0;
        }
        used += size;
        batch.push(event);
    }
    if !batch.is_empty() {
        payloads.push(batch);
    }
    payloads
        .into_iter()
        .filter_map(|events| serde_json::to_string(&envelope(events)).ok())
        .collect()
}

/// A payload back to its envelope; `None` for one this build cannot read.
pub fn unpack(payload: &str) -> Option<Envelope> {
    serde_json::from_str(payload).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(seq: u32, message: &str) -> TailEvent {
        TailEvent::log(Uuid::nil(), "probe", seq, "log", message)
    }

    /// A batch larger than one payload is split, in order, every payload
    /// under the limit, and nothing is lost or repeated.
    #[test]
    fn a_batch_is_split_into_payloads_under_the_limit_in_order() {
        let app = Uuid::new_v4();
        let events: Vec<TailEvent> = (1..=40).map(|seq| line(seq, &"x".repeat(900))).collect();
        let payloads = pack(app, "dev-a1", events.clone());
        assert!(payloads.len() > 1, "40 lines of 900 bytes need several");
        let mut back = Vec::new();
        for payload in &payloads {
            assert!(payload.len() <= MAX_PAYLOAD_BYTES, "{}", payload.len());
            let envelope = unpack(payload).expect("readable");
            assert_eq!(
                (envelope.app_id, envelope.environment.as_str()),
                (app, "dev-a1")
            );
            back.extend(envelope.events);
        }
        assert_eq!(back, events);
    }

    /// A line longer than the cap is cut on a character boundary and says
    /// so; one under it is carried whole.
    #[test]
    fn a_long_line_is_cut_and_marked() {
        let TailEvent::Log {
            message, truncated, ..
        } = line(1, &"é".repeat(MAX_MESSAGE_BYTES))
        else {
            unreachable!()
        };
        assert!(truncated);
        assert!(message.len() <= MAX_MESSAGE_BYTES);
        assert!(message.chars().all(|c| c == 'é'), "cut on a boundary");
        let TailEvent::Log {
            message, truncated, ..
        } = line(2, "saved")
        else {
            unreachable!()
        };
        assert_eq!((message.as_str(), truncated), ("saved", false));
    }

    /// Control characters escape to six bytes each, so a line under the byte
    /// cap can still overflow a payload: it is shrunk until it fits.
    #[test]
    fn a_line_that_escapes_past_the_budget_still_fits_one_payload() {
        let payloads = pack(
            Uuid::new_v4(),
            "staging",
            vec![line(1, &"\u{1}".repeat(1_999))],
        );
        assert_eq!(payloads.len(), 1);
        assert!(
            payloads[0].len() <= MAX_PAYLOAD_BYTES,
            "{}",
            payloads[0].len()
        );
        let event = &unpack(&payloads[0]).expect("readable").events[0];
        assert!(
            matches!(
                event,
                TailEvent::Log {
                    truncated: true,
                    ..
                }
            ),
            "{event:?}"
        );
    }

    /// The SSE names, and the wire's `event` tag, are the contract's.
    #[test]
    fn events_are_named_start_log_and_finish() {
        let id = Uuid::nil();
        let finish = TailEvent::Finish {
            invocation_id: id,
            function: "probe".into(),
            mode: "route".into(),
            status: "success".into(),
            failed: false,
            http_status: 200,
            duration_ms: 12,
            error: None,
            replayed: false,
        };
        assert_eq!(finish.name(), "finish");
        assert_eq!(serde_json::to_value(&finish).unwrap()["event"], "finish");
        assert_eq!(line(1, "a").name(), "log");
        let json = serde_json::to_value(line(1, "a")).unwrap();
        assert!(json.get("truncated").is_none(), "absent unless cut: {json}");
        let json = serde_json::to_value(&finish).unwrap();
        assert!(
            json.get("replayed").is_none(),
            "absent unless replayed: {json}"
        );
    }

    /// A finish's error is cut like a line's message.
    #[test]
    fn a_long_error_is_cut() {
        let outcome = Outcome {
            status: "error".into(),
            failed: true,
            http_status: 0,
            duration_ms: 1,
            error: Some("x".repeat(MAX_MESSAGE_BYTES * 2)),
        };
        let TailEvent::Finish { error, .. } =
            TailEvent::finish(Uuid::nil(), "probe", "route", &outcome)
        else {
            unreachable!()
        };
        assert_eq!(error.map(|e| e.len()), Some(MAX_MESSAGE_BYTES));
    }
}
