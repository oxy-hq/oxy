use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use df_interchange::Interchange;
use duckdb::Connection;

use crate::adapters::secrets::SecretsManager;
use crate::config::model::MotherDuck as MotherDuckConfig;
use crate::connector::constants::{CREATE_CONN, EXECUTE_QUERY, PREPARE_DUCKDB_STMT};
use crate::connector::utils::format_error_chain;
use oxy_shared::errors::OxyError;

use super::duckdb_pool::{PoolKey, PoolTarget, pool};
use super::engine::Engine;

#[derive(Debug)]
pub(super) struct MotherDuck {
    token: String,
    database: Option<String>,
}

impl MotherDuck {
    pub async fn from_config(
        secrets_manager: SecretsManager,
        config: MotherDuckConfig,
    ) -> Result<Self, OxyError> {
        let token = config.get_token(&secrets_manager).await?;
        Ok(Self {
            token,
            database: config.database,
        })
    }
}

/// The DSN DuckDB wants. It embeds the credential, so it must never reach a log
/// or an error message — see [`redact`].
fn connection_string(database: Option<&str>, token: &str) -> String {
    let base = match database {
        Some(db) => format!("md:{db}"),
        None => "md:".to_string(),
    };
    format!("{base}?motherduck_token={token}")
}

/// Strip the credential out of text on its way to a log or an error.
///
/// DuckDB echoes the DSN back in some open failures, and this is precisely the
/// path an operator reads while chasing a MotherDuck connection problem — so it
/// is the one place a token is most likely to be copied into a ticket.
fn redact(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    text.replace(token, "***")
}

/// Like `connector_internal_error`, but redacts the credential and omits the
/// `Debug` rendering (which would carry the DSN past [`redact`] unchanged).
fn motherduck_error(message: &str, e: &(dyn std::error::Error + 'static), token: &str) -> OxyError {
    let chain = redact(&format_error_chain(e), token);
    tracing::error!(error.chain = %chain, "{message}");
    OxyError::DBError(format!("{message}: {chain}"))
}

/// Check out a pooled connection to a MotherDuck database.
///
/// `md:` opens a `duckdb_database` handle exactly like a local file does, so it
/// is bound by the same rule [`super::duckdb::checkout_file_connection`] states:
/// a process must not hold two independent handles on one database. This
/// connector used to call `Connection::open` on **every query**, which both broke
/// that rule and paid a fresh network handshake plus MotherDuck extension load
/// each time — the image ships no bundled extensions, so `md:` autoloads one at
/// runtime. Pooling collapses that to once per (database, credential).
///
/// No `session_setup`: unlike the local targets there is no `file_search_path`,
/// `temp_directory` or `LOAD icu` to re-apply, and the remote catalog is visible
/// to every cloned session already.
///
/// The DSN is derived here rather than passed in, so the credential has exactly
/// one source of truth.
///
/// Fails with `(session_is_suspect, error)` because the two stages mean opposite
/// things. `get_or_init` failing is `Connection::open` failing: nothing was
/// pooled, so there is nothing to invalidate. `PoolEntry::checkout` failing is
/// `try_clone` on an *existing* primary — there is no `session_setup` to replay
/// for this target and no SQL involved, so it can only mean the pooled handle
/// itself has gone bad. Collapsing the two would leave the one error that
/// definitionally implicates the session as the one that never evicts it.
fn checkout_pooled(database: Option<&str>, token: &str) -> Result<Connection, (bool, OxyError)> {
    let key = PoolKey::motherduck(database, token);
    let dsn = connection_string(database, token);
    let token_owned = token.to_owned();
    let entry = pool()
        .get_or_init(key, move || {
            let conn = Connection::open(&dsn)
                .map_err(|err| motherduck_error(CREATE_CONN, &err, &token_owned))?;
            Ok((conn, Vec::new()))
        })
        .map_err(|e| (suspect_without_probe(FailureStage::Open), e))?;
    entry
        .checkout()
        .map_err(|e| (suspect_without_probe(FailureStage::Checkout), e))
}

/// The verdict for the two stages that never need a probe, so their call sites
/// don't have to pass an error string and a closure the function provably ignores
/// (and a `|| true` next to them reads as "the probe said alive", which is not
/// what is being expressed).
///
/// The closure returns `false` — "assume dead" — rather than panicking on an
/// invariant it expects never to be exercised. This runs on an error path the
/// connector is already handling, so an `unreachable!` here would turn a
/// recoverable query failure into an unwind if a later change ever routed `Query`
/// through it. Failing toward eviction costs one reopen instead, which is the
/// same direction the rest of this module errs in, and the tests still assert
/// that neither stage consults the probe at all.
fn suspect_without_probe(stage: FailureStage) -> bool {
    session_is_suspect(stage, "", || false)
}

/// Which stage failed, since the three mean different things for the pool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FailureStage {
    /// `Connection::open` inside the pool's init — nothing was pooled.
    Open,
    /// `try_clone` off an existing pooled primary.
    Checkout,
    /// `prepare` / `query_arrow` — bad SQL *or* a session the server ended.
    Query,
}

/// Should the pooled session be evicted?
///
/// Pure, and takes the liveness probe as a closure, so the decision table is
/// unit-testable without a MotherDuck account — the network is the one thing a
/// test here cannot have.
///
/// The two errors are asymmetric and the bias follows that: failing to evict a
/// dead session breaks every later query until the process restarts, while
/// evicting a live one costs a single reopen. So `Query` errs toward evicting —
/// the error text is consulted *first* and the probe only has to rescue the
/// common case (ordinary SQL against a healthy session).
fn session_is_suspect(
    stage: FailureStage,
    error_text: &str,
    probe_says_alive: impl FnOnce() -> bool,
) -> bool {
    match stage {
        FailureStage::Open => false,
        FailureStage::Checkout => true,
        FailureStage::Query => names_a_dead_session(error_text) || !probe_says_alive(),
    }
}

/// Error text that describes a broken connection rather than a broken query.
///
/// Every entry is a **phrase that cannot arrive as an identifier**. DuckDB names
/// the offending column in binder and catalog errors, so matching bare words like
/// `token`, `expired` or `connection` would read `Binder Error: Referenced column
/// access_token not found` as a dead session — evicting a live handle, logging a
/// connection warning that misleads whoever is reading, and (because `invalidate`
/// is keyed on database name) dropping another account's slot on a shared name.
/// A hallucinated column is *the* canonical binder error, so the workload that
/// would trip it is exactly the agent-iterating-on-generated-SQL case pooling
/// exists to protect.
///
/// The bias toward evicting still holds — a false positive costs one reopen, a
/// false negative costs every query until restart — it is just spent on phrases
/// that a schema cannot produce.
///
/// One known false positive is kept deliberately: a heavy scan that exceeds a
/// server-side limit surfaces as `… timed out` and will evict a healthy session.
/// That is one reopen, which is the cheap side of the asymmetry, and narrowing it
/// to `connection timed out` would drop genuine socket timeouts whose exact
/// wording we can't predict.
fn names_a_dead_session(text: &str) -> bool {
    let text = without_quoted_spans(text).to_ascii_lowercase();
    DEAD_SESSION_PHRASES
        .iter()
        .any(|needle| text.contains(needle))
}

/// Drop balanced quoted spans before matching needles.
///
/// `every_needle_is_a_phrase` rules out a needle arriving as an *identifier*, but
/// the text matched here is the whole error chain, and DuckDB quotes the
/// offending **value**: `Could not convert string 'connection closed' to DATE`.
/// `connection closed` and `timed out` are ordinary status values in the
/// telemetry and audit warehouses Oxy points at, so without this a cast failure
/// on such a column evicts a live session. Stripping quoted spans also covers the
/// quoted-identifier case (`"connection closed"` as a column name), which the
/// phrase rule cannot.
///
/// A **trailing** unbalanced quote keeps its remainder rather than swallowing it:
/// prose like `Can't reach host: connection refused` contains a lone apostrophe,
/// and dropping everything after it would discard the very phrase that proves the
/// session is dead — a false negative, which is the expensive direction.
///
/// Two limits worth stating, because this is a heuristic and both were found by
/// review rather than by the tests:
///
/// * **Pairing is positional, so it is parity-sensitive.** A lone apostrophe
///   *upstream* of a quoted value pairs with that value's opening quote and
///   leaves the value exposed: `Can't convert string 'connection closed' to DATE`
///   strips `t convert string ` and matches on the value anyway. That is inherent
///   to positional pairing — refusing to strip on odd parity leaves the same
///   value visible — and it lands on the cheap side (one reopen, plus a
///   misleading eviction warning).
/// * **A needle *inside* quotes is invisible**, since stripping is
///   unconditional. If MotherDuck ever wraps a remote failure as
///   `IO Error: remote request failed: 'connection reset by peer'`, the phrase
///   check misses it and the decision falls to the probe — the half documented as
///   unverified, and the expensive direction. Every DuckDB error shape we can
///   name keeps the signal outside the quotes (`HTTP GET error on '<url>'
///   (HTTP 401)`), so this is not narrowed on speculation; it is recorded so the
///   next reader knows it was considered.
fn without_quoted_spans(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(open) = rest.find(['\'', '"']) else {
            out.push_str(rest);
            return out;
        };
        let quote = rest[open..].chars().next().expect("find returned an index");
        out.push_str(&rest[..open]);
        let after = &rest[open + quote.len_utf8()..];
        match after.find(quote) {
            Some(close) => rest = &after[close + quote.len_utf8()..],
            None => {
                out.push_str(after);
                return out;
            }
        }
    }
}

/// The phrases [`names_a_dead_session`] matches on.
///
/// Lifted out of the function body so `every_needle_is_a_phrase` can enforce the
/// invariant the doc comment above states. Three separate reviews caught a bare
/// word slipping into this list (`connection`, `token`, `expired`, then
/// `unauthorized`, then `unauthenticated`), each one evicting a live session on
/// an ordinary column name — so the rule is checked by a test rather than by
/// whoever reads the diff next.
const DEAD_SESSION_PHRASES: &[&str] = &[
    "connection reset",
    "connection refused",
    "connection closed",
    "connection error",
    "broken pipe",
    "not connected",
    "timed out",
    "tls handshake",
    "unexpected eof",
    "401 unauthorized",
    // Credential failures rendered without a status code. Token expiry on a
    // long-lived pooled session is the likeliest way this connector meets a
    // dead session at all, so leaving it to match only when a number happens
    // to sit next to the word would push the case that matters most onto the
    // half of the decision that is documented as unverified.
    "authentication failed",
    "invalid credentials",
    "is unauthenticated",
    "unauthenticated request",
    "token expired",
    "expired token",
    "http 401",
    "http 403",
];

/// Best-effort "is the remote session still answering?".
///
/// Reads the catalog rather than evaluating a constant: `SELECT 1` is exactly the
/// shape MotherDuck's hybrid planner can answer from the local DuckDB engine, so
/// it could report a killed session as alive.
///
/// **This has not been verified against a session killed server-side** — doing so
/// needs a real MotherDuck account, which this change was developed without. That
/// is precisely why [`session_is_suspect`] checks the error text before calling
/// this: if the planner does serve the probe locally, the guarantee degrades to
/// the message check rather than silently disappearing.
fn session_is_alive(conn: &Connection) -> bool {
    match conn.prepare("SELECT 1 FROM information_schema.tables LIMIT 1") {
        Ok(mut stmt) => stmt.query_arrow([]).is_ok(),
        Err(_) => false,
    }
}

/// Drop the pooled handle for this database so the next query reopens it.
///
/// Unlike `Local`/`File`, a pooled `md:` primary is a **network session**: once
/// MotherDuck ends it server-side (idle eviction, credential expiry, a transient
/// network drop) every `try_clone()` off that primary hands back a connection on
/// a dead handle, and every subsequent query fails until the process restarts.
/// Opening per query — what this connector did before pooling — was wasteful but
/// self-healing; this restores that property without giving up reuse.
fn invalidate_session(database: Option<&str>) {
    // Self-healing that happens silently is indistinguishable from a hang to the
    // operator reading these logs — and this is the log they read while chasing
    // exactly this failure.
    tracing::warn!(
        database = database.unwrap_or("<default>"),
        "MotherDuck session looked broken; dropping the pooled handle so the next query reopens it"
    );
    pool().invalidate(&PoolTarget::MotherDuck {
        database: database.map(str::to_owned),
    });
}

impl Engine for MotherDuck {
    async fn run_query_with_limit(
        &self,
        query: &str,
        _dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let query = query.to_string();
        let database = self.database.clone();
        let token = self.token.clone();

        // Run blocking database operations in a spawned thread
        tokio::task::spawn_blocking(move || {
            // Scoped so the checked-out connection and its statement are dropped
            // *before* any invalidation runs — dropping the pooled primary while a
            // clone of it is still live is exactly the two-handles situation the
            // pool exists to avoid.
            let outcome = (|| {
                let conn = checkout_pooled(database.as_deref(), &token)?;

                let duckdb_chunks: Vec<_> = {
                    let mut stmt = match conn.prepare(&query) {
                        Ok(stmt) => stmt,
                        Err(err) => {
                            let e = motherduck_error(PREPARE_DUCKDB_STMT, &err, &token);
                            let suspect =
                                session_is_suspect(FailureStage::Query, &e.to_string(), || {
                                    session_is_alive(&conn)
                                });
                            return Err((suspect, e));
                        }
                    };
                    match stmt.query_arrow([]) {
                        Ok(stream) => stream.collect(),
                        Err(err) => {
                            let e = motherduck_error(EXECUTE_QUERY, &err, &token);
                            let suspect =
                                session_is_suspect(FailureStage::Query, &e.to_string(), || {
                                    session_is_alive(&conn)
                                });
                            return Err((suspect, e));
                        }
                    }
                };
                // Row *counts*, not the batches themselves: this is the path ops
                // turn debug logging on for while chasing a MotherDuck problem,
                // and rendering whole result sets there dumps customer data into
                // the log.
                tracing::debug!(
                    chunks = duckdb_chunks.len(),
                    "MotherDuck query returned result chunks"
                );
                // See duckdb.rs — `Interchange::from_arrow_58` panics on an
                // empty chunk vec because the macro indexes `df[0]` without
                // a guard. Short-circuit to the same empty-result shape
                // the function falls back to below.
                if duckdb_chunks.is_empty() {
                    return Ok((
                        Vec::new(),
                        std::sync::Arc::new(arrow::datatypes::Schema::empty()),
                    ));
                }
                // A conversion failure is our bug, not a dead session — the rows
                // already arrived, so the handle stays pooled.
                let arrow_chunks = Interchange::from_arrow_58(duckdb_chunks)
                    .map_err(|err| (false, motherduck_error(EXECUTE_QUERY, &err, &token)))?
                    .to_arrow_58()
                    .map_err(|err| (false, motherduck_error(EXECUTE_QUERY, &err, &token)))?;
                let schema: SchemaRef = arrow_chunks
                    .first()
                    .map(|b| b.schema())
                    .unwrap_or_else(|| std::sync::Arc::new(arrow::datatypes::Schema::empty()));
                Ok((arrow_chunks, schema))
            })();

            outcome.map_err(|(session_is_suspect, err)| {
                if session_is_suspect {
                    invalidate_session(database.as_deref());
                }
                err
            })
        })
        .await
        .map_err(|err| OxyError::RuntimeError(format!("Task join error: {}", err)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Connection::open` failed, so the pool holds nothing for this target.
    /// Evicting would be a no-op at best and could drop a *different* caller's
    /// freshly-built entry at worst.
    #[test]
    fn a_failed_open_never_evicts() {
        assert!(!session_is_suspect(FailureStage::Open, "", || {
            panic!("the probe must not run for an open failure — there is no session yet")
        }));
    }

    /// `try_clone` off an existing primary involves no SQL and no user input, so
    /// the only thing it can be telling us is that the pooled handle is bad. This
    /// is the case that regressed once already by being lumped in with `Open`.
    #[test]
    fn a_failed_checkout_always_evicts() {
        assert!(session_is_suspect(FailureStage::Checkout, "", || {
            panic!("the probe must not decide a checkout failure — it is definitionally the handle")
        }));
    }

    /// The case pooling exists for: an agent iterating on generated SQL must not
    /// pay a reopen per retry.
    #[test]
    fn bad_sql_on_a_live_session_keeps_the_handle() {
        let err = "Failed to execute query: Catalog Error: Table with name orders does not exist";
        assert!(!session_is_suspect(FailureStage::Query, err, || true));
    }

    #[test]
    fn a_query_failure_on_a_dead_session_evicts() {
        let err = "Failed to execute query: Catalog Error: Table with name orders does not exist";
        assert!(session_is_suspect(FailureStage::Query, err, || false));
    }

    /// The backstop. If MotherDuck's planner answers the liveness probe from the
    /// local engine, the probe reports a killed session as alive — so an error
    /// that *names* a broken connection has to evict on its own, without ever
    /// consulting it.
    #[test]
    fn an_error_naming_a_dead_session_evicts_even_when_the_probe_says_alive() {
        for err in [
            "Failed to execute query: Connection reset by peer",
            "Failed to prepare DuckDB statement: connection refused",
            "Failed to execute query: broken pipe",
            "Failed to execute query: HTTP 401 Unauthorized",
            "Failed to execute query: authentication failed",
            "Failed to prepare DuckDB statement: invalid credentials",
            "Failed to execute query: request is unauthenticated",
            "Failed to execute query: request timed out",
            "Failed to execute query: token expired",
        ] {
            assert!(
                session_is_suspect(FailureStage::Query, err, || true),
                "expected {err:?} to evict on the message alone"
            );
        }
    }

    /// The bias is deliberate — a false positive costs one reopen, a false
    /// negative costs every query until the process restarts — but it must not be
    /// so broad that ordinary SQL errors trip it.
    #[test]
    fn ordinary_sql_errors_do_not_read_as_a_dead_session() {
        for err in [
            "Parser Error: syntax error at or near SELCT",
            "Catalog Error: Table with name orders does not exist",
            "Conversion Error: Could not convert string to INT32",
            "Binder Error: Referenced column amount not found",
        ] {
            assert!(
                !names_a_dead_session(err),
                "{err:?} must not read as a dead session"
            );
        }
    }

    /// A needle can't arrive as an identifier, but it can arrive as a *value* —
    /// DuckDB quotes the offending string in a conversion error, and
    /// `connection closed` / `timed out` are ordinary status values in the
    /// telemetry and audit schemas this connector serves.
    #[test]
    fn a_quoted_data_value_is_not_a_dead_session() {
        for err in [
            "Failed to execute query: Conversion Error: Could not convert string 'connection closed' to DATE",
            "Failed to execute query: Conversion Error: Could not convert string 'timed out' to INT32",
            "Failed to execute query: Binder Error: Referenced column \"connection closed\" not found",
        ] {
            assert!(
                !names_a_dead_session(err),
                "{err:?} quotes a value, not a broken connection"
            );
        }
    }

    /// Stripping quoted spans must not swallow the rest of the message on a lone
    /// apostrophe — that would turn a real dead session into a missed one, which
    /// is the expensive direction.
    #[test]
    fn an_unbalanced_quote_still_leaves_the_signal_visible() {
        assert!(names_a_dead_session(
            "Failed to execute query: Can't reach host: connection refused"
        ));
    }

    /// Enforces the invariant the list's doc comment states, instead of trusting
    /// the next reader to notice. An unquoted SQL identifier cannot contain a
    /// space, so "contains a space" is exactly the property that makes a needle
    /// impossible to match against a column or table name.
    ///
    /// This exists because the rule was broken three times — `connection` /
    /// `token` / `expired`, then `unauthorized`, then `unauthenticated` — each
    /// time evicting a live pooled session on an ordinary schema, and each time
    /// caught by review rather than by the suite.
    #[test]
    fn every_needle_is_a_phrase() {
        for needle in DEAD_SESSION_PHRASES {
            assert!(
                needle.contains(' '),
                "{needle:?} is a bare word — a column named after it would evict a live session"
            );
        }
    }

    /// The regression the phrase list exists to prevent. DuckDB names the
    /// offending identifier, and these are ordinary warehouse column names — a
    /// bare-word match on `token` / `expired` / `connection` / `network` /
    /// `socket` / `timeout` would evict a perfectly live session. Both layers are
    /// asserted because `||` short-circuits: a bad phrase match never reaches the
    /// probe, so the probe cannot rescue it.
    #[test]
    fn a_sql_error_naming_a_session_ish_column_is_not_a_dead_session() {
        for err in [
            "Binder Error: Referenced column access_token not found",
            "Binder Error: Referenced column expired_at not found",
            "Catalog Error: Table with name connection_id does not exist",
            "Binder Error: Referenced column network_zone not found",
            "Binder Error: Referenced column socket_id not found",
            "Conversion Error: Could not convert timeout_ms to INT32",
            // An audit/security warehouse is exactly the shape Oxy points at, and
            // `unauthorized` was a bare needle until the HTTP form replaced it.
            "Catalog Error: Table with name unauthorized_events does not exist",
            "Binder Error: Referenced column unauthorized_attempts not found",
            // One per credential needle. The positive test proves a needle
            // fires; only this one proves it does not fire on customer data,
            // and skipping it here is what let `unauthorized` through.
            "Catalog Error: Table with name unauthenticated_requests does not exist",
            "Binder Error: Referenced column is_unauthenticated not found",
            "Binder Error: Referenced column authentication_failures not found",
            "Catalog Error: Table with name invalid_credentials_log does not exist",
        ] {
            assert!(
                !names_a_dead_session(err),
                "{err:?} names a column, not a broken connection"
            );
            assert!(
                !session_is_suspect(FailureStage::Query, err, || true),
                "{err:?} must leave a live pooled session alone"
            );
        }
    }
}
