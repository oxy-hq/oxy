//! The broker side of a `ctx.*` call: the request an op sends over the
//! channel ([`HostCall`]), the span it runs under, and how it is dispatched to
//! the [`FunctionHost`] and recorded. Engine-neutral — nothing here knows
//! which JS engine sent the call.

use tokio::sync::oneshot;

use super::FunctionHost;

/// A request the isolate sends to the broker loop.
pub(super) enum HostCall {
    Query {
        sql: String,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    QueryStream {
        sql: String,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Fetch {
        url: String,
        init: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SemanticQuery {
        spec: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    AirwayRun {
        pipeline_ref: String,
        variables: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    WarehouseWrite {
        op: String,
        payload: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SecretsSet {
        key: String,
        value: String,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SendEmail {
        input: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Notify {
        input: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Storage {
        op: String,
        payload: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    /// `ctx.org.people()` — no arguments; the org is the host's, never the
    /// function's.
    OrgPeople {
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    OrgPlaces {
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    OrgAssignments {
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    /// `ctx.platform.facts(opts)` — `opts` names a span, never an org.
    PlatformFacts {
        opts: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Tx {
        op: String,
        payload: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Oltp {
        op: String,
        payload: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Airhouse {
        op: String,
        payload: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
}

/// Which reply shape [`record_host_call_outcome`] should read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HostCallKind {
    Query,
    Fetch,
    Other,
}

/// Where a host call went, for the failure note's clear-on-success rule
/// (`FunctionHost::note_host_call_success`): a later success of the same op
/// clears a noted failure only when it went to the same place. Read from the
/// call before dispatch, on both the failing and the succeeding side, so the
/// two cannot disagree.
///
/// Only what a span of this call already records, and never a value the
/// caller could put anything in: a `fetch`'s host, by the one parser its span
/// uses (`url_shape::fetch_target`) — scheme, port, path and query stay out,
/// and a URL with no host has no target; a `warehouse.*` op's database and,
/// when the payload names one, table (`db.namespace`, `db.collection.name`),
/// gated as the span gates them (`data_audit::record_db_span`): a value that
/// is not `identifier_like` — over 64 chars, or anything outside
/// `[A-Za-z0-9_.$-]` — drops out of the target rather than into it, so the
/// note holds exactly what the span records. Every other op clears by name
/// alone: a storage key or an OLTP statement is payload, an app's own
/// Airhouse and OLTP schemas are one destination each, and a transaction op
/// names its transaction, not a place. The host normalizes what it keeps of
/// this (`HostCallFailure::noted`).
pub(super) fn host_call_target(call: &HostCall) -> Option<String> {
    use super::super::host_call_attrs::{fetch_target, identifier_like};
    match call {
        HostCall::Fetch { url, .. } => Some(fetch_target(url).host).filter(|host| !host.is_empty()),
        HostCall::WarehouseWrite { payload, .. } => {
            let field = |name: &str| {
                payload
                    .get(name)
                    .and_then(|v| v.as_str())
                    .filter(|v| identifier_like(v))
            };
            let database = field("database")?;
            Some(match field("table") {
                Some(table) => format!("{database} {table}"),
                None => database.to_string(),
            })
        }
        _ => None,
    }
}

/// The span one host op runs under. Named and attributed on the OpenTelemetry
/// semantic conventions where one exists (`db.query`,
/// `http.client.request`), `oxy.*` otherwise — see `host_call_attrs` for
/// what is deliberately *not* recorded. Also the fixed op name a failure of
/// this call pages under (`host_call_attrs::host_op_name`).
pub(super) fn host_call_span(call: &HostCall) -> (tracing::Span, HostCallKind, &'static str) {
    use super::super::host_call_attrs::{
        HOST_CALL_TARGET, db_query_summary, fetch_target, host_op_name,
    };
    use tracing::field::Empty;
    match call {
        HostCall::Query { sql, .. } | HostCall::QueryStream { sql, .. } => {
            let s = db_query_summary(sql);
            let streaming = matches!(call, HostCall::QueryStream { .. });
            let op = if streaming { "query_stream" } else { "query" };
            let span = tracing::info_span!(target: HOST_CALL_TARGET,
                "db.query",
                db.operation.name = %s.verb,
                db.collection.name = %s.table,
                oxy.op = op,
                db.response.returned_rows = Empty,
                oxy.truncated = Empty,
                otel.status_code = Empty,
                error.type = Empty,
            );
            (span, HostCallKind::Query, op)
        }
        HostCall::Fetch { url, init, .. } => {
            let t = fetch_target(url);
            let method = init
                .get("method")
                .and_then(|m| m.as_str())
                .map(str::to_ascii_uppercase)
                .unwrap_or_else(|| "GET".to_string());
            let span = tracing::info_span!(target: HOST_CALL_TARGET,
                "http.client.request",
                otel.kind = "client",
                http.request.method = %method,
                url.scheme = %t.scheme,
                server.address = %t.host,
                server.port = t.port,
                http.response.status_code = Empty,
                otel.status_code = Empty,
                error.type = Empty,
            );
            (span, HostCallKind::Fetch, "fetch")
        }
        HostCall::SemanticQuery { spec, .. } => {
            let measures = spec
                .get("measures")
                .and_then(|m| m.as_array())
                .map(|m| m.len());
            let span = tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.semantic.query",
                oxy.semantic.measures = measures,
                otel.status_code = Empty,
                error.type = Empty,
            );
            (span, HostCallKind::Other, "semantic.query")
        }
        HostCall::AirwayRun { pipeline_ref, .. } => (
            tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.airway.run",
                oxy.airway.pipeline = %pipeline_ref,
                otel.status_code = Empty,
                error.type = Empty,
            ),
            HostCallKind::Other,
            "airway.run",
        ),
        HostCall::WarehouseWrite { op, .. } => {
            let span = tracing::info_span!(target: HOST_CALL_TARGET,
                "db.query",
                db.system = "airhouse",
                db.operation.name = %op,
                db.namespace = Empty,
                db.query.summary = Empty,
                db.collection.name = Empty,
                otel.status_code = Empty,
                error.type = Empty,
            );
            (span, HostCallKind::Other, host_op_name("warehouse", op))
        }
        HostCall::Tx { op, .. } => {
            let span = tracing::info_span!(target: HOST_CALL_TARGET,
                "db.transaction",
                db.operation.name = %op,
                db.namespace = Empty,
                db.query.summary = Empty,
                db.collection.name = Empty,
                otel.status_code = Empty,
                error.type = Empty,
            );
            (span, HostCallKind::Other, host_op_name("tx", op))
        }
        HostCall::Airhouse { op, .. } => {
            let span = tracing::info_span!(target: HOST_CALL_TARGET,
                "db.query",
                db.system = "airhouse",
                db.operation.name = %op,
                db.namespace = Empty,
                db.query.summary = Empty,
                db.collection.name = Empty,
                otel.status_code = Empty,
                error.type = Empty,
            );
            (span, HostCallKind::Other, host_op_name("airhouse", op))
        }
        HostCall::Oltp { op, .. } => {
            let span = tracing::info_span!(target: HOST_CALL_TARGET,
                "db.query",
                db.system = "postgres",
                db.operation.name = %op,
                // `db.namespace` / `db.query.summary` / `db.collection.name`
                // are recorded by the host once it has resolved the schema
                // and summarised the SQL (`host::record_db_span`), so the
                // statement is not summarised twice.
                db.namespace = Empty,
                db.query.summary = Empty,
                db.collection.name = Empty,
                otel.status_code = Empty,
                error.type = Empty,
            );
            (span, HostCallKind::Other, host_op_name("oltp", op))
        }
        HostCall::SecretsSet { key, .. } => (
            tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.secrets.set",
                oxy.secret.name = %key,
                otel.status_code = Empty,
                error.type = Empty,
            ),
            HostCallKind::Other,
            "secrets.set",
        ),
        HostCall::SendEmail { input, .. } => {
            let recipients = match input.get("to") {
                Some(serde_json::Value::Array(a)) => a.len(),
                Some(serde_json::Value::String(_)) => 1,
                _ => 0,
            };
            (
                tracing::info_span!(target: HOST_CALL_TARGET,
                    "oxy.email.send",
                    oxy.email.recipients = recipients,
                    otel.status_code = Empty,
                    error.type = Empty,
                ),
                HostCallKind::Other,
                "email.send",
            )
        }
        HostCall::Notify { input, .. } => {
            let targets = match input.get("to") {
                Some(serde_json::Value::Array(a)) => a.len(),
                Some(serde_json::Value::Object(_)) => 1,
                _ => 0,
            };
            (
                tracing::info_span!(target: HOST_CALL_TARGET,
                    "oxy.notify",
                    oxy.notify.targets = targets,
                    otel.status_code = Empty,
                    error.type = Empty,
                ),
                HostCallKind::Other,
                "notify",
            )
        }
        HostCall::Storage { op, .. } => (
            tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.storage",
                oxy.storage.op = %op,
                otel.status_code = Empty,
                error.type = Empty,
            ),
            HostCallKind::Other,
            host_op_name("storage", op),
        ),
        HostCall::OrgPeople { .. } => (
            tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.org.people",
                otel.status_code = Empty,
                error.type = Empty,
            ),
            HostCallKind::Other,
            "org.people",
        ),
        HostCall::OrgPlaces { .. } => (
            tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.org.places",
                otel.status_code = Empty,
                error.type = Empty,
            ),
            HostCallKind::Other,
            "org.places",
        ),
        HostCall::OrgAssignments { .. } => (
            tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.org.assignments",
                otel.status_code = Empty,
                error.type = Empty,
            ),
            HostCallKind::Other,
            "org.assignments",
        ),
        HostCall::PlatformFacts { .. } => (
            tracing::info_span!(target: HOST_CALL_TARGET,
                "oxy.platform.facts",
                otel.status_code = Empty,
                error.type = Empty,
            ),
            HostCallKind::Other,
            "platform.facts",
        ),
    }
}

/// Run one host op and hand back the reply channel with its result, so the
/// caller can record the outcome on the current span before answering.
pub(super) async fn dispatch_host_call(
    call: HostCall,
    host: std::sync::Arc<dyn FunctionHost>,
) -> (
    oneshot::Sender<Result<serde_json::Value, String>>,
    Result<serde_json::Value, String>,
) {
    match call {
        HostCall::Query { sql, reply } => (reply, host.query(sql).await),
        HostCall::QueryStream { sql, reply } => (reply, host.query_stream(sql).await),
        HostCall::Fetch { url, init, reply } => (reply, host.fetch(url, init).await),
        HostCall::SemanticQuery { spec, reply } => (reply, host.semantic_query(spec).await),
        HostCall::AirwayRun {
            pipeline_ref,
            variables,
            reply,
        } => (reply, host.airway_run(pipeline_ref, variables).await),
        HostCall::WarehouseWrite { op, payload, reply } => {
            let result = if op == "query" {
                host.warehouse_query(payload).await
            } else {
                host.warehouse_write(op, payload).await
            };
            (reply, result)
        }
        HostCall::Tx { op, payload, reply } => (reply, host.tx(op, payload).await),
        HostCall::Oltp { op, payload, reply } => (reply, host.oltp(op, payload).await),
        HostCall::Airhouse { op, payload, reply } => (reply, host.airhouse(op, payload).await),
        HostCall::SecretsSet { key, value, reply } => (reply, host.secrets_set(key, value).await),
        HostCall::SendEmail { input, reply } => (reply, host.send_email(input).await),
        HostCall::Notify { input, reply } => (reply, host.notify(input).await),
        HostCall::Storage { op, payload, reply } => (reply, host.storage(op, payload).await),
        HostCall::OrgPeople { reply } => (reply, host.org_people().await),
        HostCall::OrgPlaces { reply } => (reply, host.org_places().await),
        HostCall::OrgAssignments { reply } => (reply, host.org_assignments().await),
        HostCall::PlatformFacts { opts, reply } => (reply, host.platform_facts(opts).await),
    }
}

/// Record how a host op ended on the current span: an `error.type` facet on
/// failure, rows / truncation for a query, the upstream status for a fetch.
pub(super) fn record_host_call_outcome(
    kind: HostCallKind,
    result: &Result<serde_json::Value, String>,
    environment_labels: bool,
) {
    use super::super::host_call_attrs::{
        classify_host_error_for, fetch_status, rows_and_truncated,
    };
    let span = tracing::Span::current();
    match result {
        Err(message) => {
            span.record("otel.status_code", "ERROR");
            span.record(
                "error.type",
                classify_host_error_for(message, environment_labels),
            );
        }
        Ok(value) => match kind {
            HostCallKind::Query => {
                let (rows, truncated) = rows_and_truncated(value);
                if let Some(rows) = rows {
                    span.record("db.response.returned_rows", rows);
                }
                if let Some(truncated) = truncated {
                    span.record("oxy.truncated", truncated);
                }
            }
            HostCallKind::Fetch => {
                if let Some(status) = fetch_status(value) {
                    span.record("http.response.status_code", status);
                    if status >= 500 {
                        span.record("otel.status_code", "ERROR");
                        span.record("error.type", "upstream_5xx");
                    }
                }
            }
            HostCallKind::Other => {}
        },
    }
}
