//! Integration tests for `PgTransaction` — the pinned-connection primitive
//! behind `ctx.tx()`.
//!
//! These run against a **real** Postgres (shared testcontainer, or
//! `OXY_TEST_POSTGRES_URL`). That is deliberate and not negotiable for this
//! surface: every property worth having here — does a rollback actually
//! discard, is an uncommitted write actually invisible, does a dropped handle
//! actually leave nothing behind — is a property of the *server*, and a mock
//! would assert only that our own code called the method we told it to call.
//!
//! Run with:
//!
//!   cargo nextest run -p agentic-connector --features postgres \
//!     --test integration -E 'test(postgres_tx_tests)'
//!
//! Every test uses its own table name because the container is shared across
//! the binary.

#![cfg(feature = "postgres")]

use agentic_connector::{DatabaseConnector, PostgresConnector};
use serde_json::json;

use super::postgres_tests::test_dsn;

/// Build a connector, or `None` when there is no Docker and no external DSN.
async fn connector() -> Option<PostgresConnector> {
    let dsn = test_dsn().await?;
    Some(PostgresConnector::new(
        &dsn.host,
        dsn.port,
        &dsn.user,
        &dsn.password,
        &dsn.database,
    ))
}

/// Skip-with-a-reason, so a suite run without Docker reads as "not exercised"
/// rather than "passed".
macro_rules! connector_or_skip {
    () => {
        match connector().await {
            Some(c) => c,
            None => {
                eprintln!("skipping: Docker not available and OXY_TEST_POSTGRES_URL not set");
                return;
            }
        }
    };
}

async fn scratch_table(c: &PostgresConnector, name: &str, cols: &str) -> String {
    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(&format!("DROP TABLE IF EXISTS {name}"), &[])
        .await
        .expect("drop");
    tx.exec(&format!("CREATE TABLE {name} ({cols})"), &[])
        .await
        .expect("create");
    tx.commit().await.expect("commit ddl");
    name.to_string()
}

/// Count rows using a *separate* connection, so we observe what actually
/// committed rather than what one session can see inside its own transaction.
async fn count_outside(c: &PostgresConnector, table: &str) -> i64 {
    let mut tx = c.begin_transaction().await.expect("begin observer");
    let rows = tx
        .query(&format!("SELECT count(*)::int8 AS n FROM {table}"), &[])
        .await
        .expect("count");
    let n = rows[0]["n"].as_i64().expect("count is an integer");
    tx.rollback().await.expect("close observer");
    n
}

// ── The core transactional guarantees ───────────────────────────────────────

#[tokio::test]
async fn commit_persists_every_statement() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_commit_t", "id int4 primary key, label text").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(
        &format!("INSERT INTO {t} (id, label) VALUES ($1, $2)"),
        &[json!(1), json!("a")],
    )
    .await
    .expect("insert 1");
    tx.exec(
        &format!("INSERT INTO {t} (id, label) VALUES ($1, $2)"),
        &[json!(2), json!("b")],
    )
    .await
    .expect("insert 2");
    tx.commit().await.expect("commit");

    assert_eq!(count_outside(&c, &t).await, 2);
}

#[tokio::test]
async fn rollback_discards_every_statement() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_rollback_t", "id int4 primary key").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(&format!("INSERT INTO {t} (id) VALUES ($1)"), &[json!(1)])
        .await
        .expect("insert");
    tx.rollback().await.expect("rollback");

    assert_eq!(count_outside(&c, &t).await, 0, "rollback must discard");
}

/// The safety property the whole design leans on: an abandoned transaction
/// must NOT commit. The isolate can be killed mid-transaction (timeout, client
/// disconnect, dashboard cancel) and no cleanup code of ours is guaranteed to
/// run — so dropping the handle has to be equivalent to a rollback.
#[tokio::test]
async fn dropping_the_handle_without_commit_rolls_back() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_drop_t", "id int4 primary key").await;

    {
        let mut tx = c.begin_transaction().await.expect("begin");
        tx.exec(&format!("INSERT INTO {t} (id) VALUES ($1)"), &[json!(7)])
            .await
            .expect("insert");
        // Dropped here without commit or rollback.
    }
    // The socket close races the server's cleanup; give it a moment before
    // asserting, otherwise this test is flaky for reasons unrelated to the
    // property under test.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    assert_eq!(
        count_outside(&c, &t).await,
        0,
        "an abandoned transaction must not commit"
    );
}

/// An uncommitted write must be invisible to another session. This is what
/// proves we are in a real transaction rather than autocommitting each
/// statement — the failure mode that would make every other test here pass
/// while the feature is a lie.
#[tokio::test]
async fn an_uncommitted_write_is_invisible_to_another_connection() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_isolation_t", "id int4 primary key").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(&format!("INSERT INTO {t} (id) VALUES ($1)"), &[json!(1)])
        .await
        .expect("insert");

    assert_eq!(
        count_outside(&c, &t).await,
        0,
        "another session must not see an uncommitted row"
    );

    // …and sees it the moment we commit.
    tx.commit().await.expect("commit");
    assert_eq!(count_outside(&c, &t).await, 1);
}

/// The motivating case from the design doc: "insert order + line items +
/// decrement inventory, or nothing". A failure partway through must leave the
/// earlier statements unapplied.
#[tokio::test]
async fn a_failure_partway_through_leaves_nothing_applied() {
    let c = connector_or_skip!();
    let orders = scratch_table(&c, "tx_orders_t", "id int4 primary key, table_no int4").await;
    let items = scratch_table(
        &c,
        "tx_order_items_t",
        "order_id int4 not null, qty int4 not null check (qty > 0)",
    )
    .await;

    let mut tx = c.begin_transaction().await.expect("begin");
    let returned = tx
        .query(
            &format!("INSERT INTO {orders} (id, table_no) VALUES ($1, $2) RETURNING id"),
            &[json!(1), json!(12)],
        )
        .await
        .expect("insert order");
    let order_id = returned[0]["id"].as_i64().expect("RETURNING id");

    // Violates the CHECK constraint — this is the "third statement fails" case.
    let failed = tx
        .exec(
            &format!("INSERT INTO {items} (order_id, qty) VALUES ($1, $2)"),
            &[json!(order_id), json!(0)],
        )
        .await;
    assert!(
        failed.is_err(),
        "the check constraint should reject qty = 0"
    );

    tx.rollback().await.expect("rollback");

    assert_eq!(
        count_outside(&c, &orders).await,
        0,
        "the order must not survive"
    );
    assert_eq!(count_outside(&c, &items).await, 0);
}

// ── Parameter binding ───────────────────────────────────────────────────────

/// The security property. `ctx.warehouse.exec` takes a bare string; this path
/// exists for surfaces that accept end-user input, so a value that looks like
/// SQL must be stored as data and nothing else.
#[tokio::test]
async fn parameters_are_bound_not_interpolated() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_injection_t", "id int4 primary key, note text").await;
    let hostile = "'); DROP TABLE tx_injection_t; --";

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(
        &format!("INSERT INTO {t} (id, note) VALUES ($1, $2)"),
        &[json!(1), json!(hostile)],
    )
    .await
    .expect("insert hostile string");
    tx.commit().await.expect("commit");

    let mut tx = c.begin_transaction().await.expect("begin read");
    let rows = tx
        .query(&format!("SELECT note FROM {t} WHERE id = $1"), &[json!(1)])
        .await
        .expect("read back");
    tx.rollback().await.expect("close");

    assert_eq!(
        rows[0]["note"].as_str(),
        Some(hostile),
        "the payload must round-trip as a literal string"
    );
    assert_eq!(
        count_outside(&c, &t).await,
        1,
        "the table must still exist and hold its row"
    );
}

/// JSON has one number type; Postgres has several. We prepare first and bind
/// to the type Postgres inferred, so a plain JSON number lands in an `int4`
/// column without the caller casting anything.
#[tokio::test]
async fn a_json_number_binds_to_the_columns_actual_integer_type() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_inttypes_t", "small int2, mid int4, big int8").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(
        &format!("INSERT INTO {t} (small, mid, big) VALUES ($1, $2, $3)"),
        &[json!(1), json!(2), json!(3_000_000_000i64)],
    )
    .await
    .expect("int2/int4/int8 all bind from JSON numbers");
    let rows = tx
        .query(&format!("SELECT small, mid, big FROM {t}"), &[])
        .await
        .expect("read back");
    tx.rollback().await.expect("close");

    assert_eq!(rows[0]["small"].as_i64(), Some(1));
    assert_eq!(rows[0]["mid"].as_i64(), Some(2));
    assert_eq!(rows[0]["big"].as_i64(), Some(3_000_000_000));
}

#[tokio::test]
async fn null_and_json_columns_round_trip() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_nulljson_t", "id int4, blob jsonb, maybe text").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(
        &format!("INSERT INTO {t} (id, blob, maybe) VALUES ($1, $2, $3)"),
        &[json!(1), json!({"k": [1, 2]}), serde_json::Value::Null],
    )
    .await
    .expect("insert");
    let rows = tx
        .query(&format!("SELECT id, blob, maybe FROM {t}"), &[])
        .await
        .expect("read back");
    tx.rollback().await.expect("close");

    assert_eq!(rows[0]["blob"], json!({"k": [1, 2]}));
    assert_eq!(rows[0]["maybe"], serde_json::Value::Null);
}

#[tokio::test]
async fn a_parameter_count_mismatch_is_an_error_naming_both_counts() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_argcount_t", "id int4, label text").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    let err = tx
        .exec(
            &format!("INSERT INTO {t} (id, label) VALUES ($1, $2)"),
            &[json!(1)],
        )
        .await
        .expect_err("two placeholders, one argument")
        .to_string();
    tx.rollback().await.expect("close");

    assert!(
        err.contains('2') && err.contains('1'),
        "names both counts: {err}"
    );
}

/// A type we cannot decode must fail loudly with the cast that fixes it —
/// never a silent coercion. `numeric` is the case that matters: quietly
/// returning it as an `f64` is how money columns lose cents.
#[tokio::test]
async fn an_undecodable_column_type_errors_with_the_fix() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_numeric_t", "amount numeric(10,2)").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(
        &format!("INSERT INTO {t} (amount) VALUES ($1::text::numeric)"),
        &[json!("12.50")],
    )
    .await
    .expect("the documented text-cast escape hatch must work for writes");

    let err = tx
        .query(&format!("SELECT amount FROM {t}"), &[])
        .await
        .expect_err("numeric is not directly decodable")
        .to_string();
    assert!(err.contains("amount"), "names the column: {err}");
    assert!(err.contains("::text"), "gives the fix: {err}");

    // …and the cast the error recommends actually works.
    let rows = tx
        .query(&format!("SELECT amount::text AS amount FROM {t}"), &[])
        .await
        .expect("the recommended cast must work");
    tx.rollback().await.expect("close");
    assert_eq!(rows[0]["amount"].as_str(), Some("12.50"));
}

/// Two transactions on the same connector must not serialise behind one
/// another — the reason `begin_transaction` opens its own connection instead
/// of borrowing the connector's mutex-guarded client.
#[tokio::test]
async fn two_transactions_are_concurrently_open_on_one_connector() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_concurrent_t", "id int4 primary key").await;

    let mut a = c.begin_transaction().await.expect("begin a");
    let mut b = c.begin_transaction().await.expect("begin b");

    a.exec(&format!("INSERT INTO {t} (id) VALUES ($1)"), &[json!(1)])
        .await
        .expect("a writes");
    b.exec(&format!("INSERT INTO {t} (id) VALUES ($1)"), &[json!(2)])
        .await
        .expect("b writes while a is still open");

    a.commit().await.expect("commit a");
    b.commit().await.expect("commit b");

    assert_eq!(count_outside(&c, &t).await, 2);
}

/// REPRO (finding 1): a statement that fails aborts the Postgres transaction
/// block. `COMMIT` on an aborted block does **not** error — the server ends the
/// block and reports the `ROLLBACK` command tag — so a callback that catches a
/// failed statement and returns normally resolves `ctx.tx()` having persisted
/// nothing.
#[tokio::test]
async fn commit_after_a_swallowed_statement_error_must_not_report_success() {
    let c = connector_or_skip!();
    let t = scratch_table(
        &c,
        "tx_poison_t",
        "id int4 primary key, qty int4 check (qty > 0)",
    )
    .await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(
        &format!("INSERT INTO {t} (id, qty) VALUES ($1, $2)"),
        &[json!(1), json!(5)],
    )
    .await
    .expect("first insert succeeds");
    let _swallowed = tx
        .exec(
            &format!("INSERT INTO {t} (id, qty) VALUES ($1, $2)"),
            &[json!(2), json!(0)],
        )
        .await
        .expect_err("check constraint rejects qty = 0");

    let committed = tx.commit().await;
    assert!(
        committed.is_err(),
        "commit must refuse an aborted transaction rather than silently persisting nothing"
    );
    assert_eq!(
        count_outside(&c, &t).await,
        0,
        "nothing was persisted either way"
    );
}

/// The motivating example's real shape: an OLTP table keyed by `uuid` with a
/// `timestamptz` audit column. Before uuid/temporal support, `INSERT …
/// RETURNING id` on this table errored — i.e. the documented example did not run.
#[tokio::test]
async fn a_uuid_keyed_table_with_timestamps_round_trips() {
    let c = connector_or_skip!();
    let t = scratch_table(
        &c,
        "tx_uuid_t",
        "id uuid primary key default gen_random_uuid(), \
         created_at timestamptz not null, on_day date not null, at_time time not null",
    )
    .await;

    let mut tx = c.begin_transaction().await.expect("begin");
    let rows = tx
        .query(
            &format!(
                "INSERT INTO {t} (created_at, on_day, at_time) VALUES ($1, $2, $3) \
                 RETURNING id, created_at, on_day, at_time"
            ),
            &[
                json!("2026-08-18T12:00:00Z"),
                json!("2026-08-18"),
                json!("12:00:00"),
            ],
        )
        .await
        .expect("uuid PK + temporal columns must round-trip");

    let id = rows[0]["id"].as_str().expect("uuid renders as a string");
    assert_eq!(id.len(), 36, "canonical uuid form: {id}");
    assert!(
        rows[0]["created_at"]
            .as_str()
            .unwrap()
            .starts_with("2026-08-18T12:00:00"),
        "timestamptz is RFC-3339: {}",
        rows[0]["created_at"]
    );
    assert_eq!(rows[0]["on_day"].as_str(), Some("2026-08-18"));

    // A value read back must be usable as a parameter without reformatting —
    // that round-trip is the whole point of choosing these output formats.
    let found = tx
        .query(
            &format!("SELECT 1 AS n FROM {t} WHERE id = $1"),
            &[json!(id)],
        )
        .await
        .expect("the returned uuid must bind straight back as a parameter");
    assert_eq!(found.len(), 1);
    tx.rollback().await.expect("close");
}

/// A `query` that would materialise an unbounded result while holding locks is
/// refused, with the fix in the message.
#[tokio::test]
async fn an_unbounded_query_inside_a_transaction_is_refused() {
    let c = connector_or_skip!();
    let mut tx = c.begin_transaction().await.expect("begin");
    let err = tx
        .query("SELECT generate_series(1, 50000) AS n", &[])
        .await
        .expect_err("50k rows is over the in-transaction ceiling")
        .to_string();
    tx.rollback().await.expect("close");
    assert!(err.contains("LIMIT"), "names the fix: {err}");
    assert!(
        err.contains("ctx.query"),
        "points outside the transaction: {err}"
    );
}

/// The server-side backstops must actually be in force on the pinned
/// connection — `SET LOCAL` in the same batch as `BEGIN` is easy to get wrong
/// (outside a block it is a silent no-op with a warning).
#[tokio::test]
async fn the_server_side_timeouts_are_in_force_inside_the_transaction() {
    let c = connector_or_skip!();
    let mut tx = c.begin_transaction().await.expect("begin");
    let rows = tx
        .query(
            "SELECT current_setting('statement_timeout') AS stmt, \
             current_setting('idle_in_transaction_session_timeout') AS idle",
            &[],
        )
        .await
        .expect("read the session settings");
    tx.rollback().await.expect("close");

    assert_eq!(rows[0]["stmt"].as_str(), Some("30s"), "statement_timeout");
    assert_eq!(
        rows[0]["idle"].as_str(),
        Some("1min"),
        "idle_in_transaction_session_timeout"
    );
}

/// A client-side rejection happens *after* a successful `prepare`, so the
/// Postgres block is still healthy and the transaction must remain committable.
/// The cancellation-safe arm-then-disarm ordering makes this easy to get wrong
/// in the other direction — poisoning on an error that never reached the server.
#[tokio::test]
async fn a_client_side_argument_error_does_not_poison_the_transaction() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_clienterr_t", "id int4 primary key").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(&format!("INSERT INTO {t} (id) VALUES ($1)"), &[json!(1)])
        .await
        .expect("first insert");

    // Wrong argument count — rejected by us, never sent to the server.
    tx.exec(&format!("INSERT INTO {t} (id) VALUES ($1)"), &[])
        .await
        .expect_err("argument count is checked client-side");
    // Unbindable value — same, rejected after prepare succeeded.
    tx.exec(
        &format!("INSERT INTO {t} (id) VALUES ($1)"),
        &[json!("nope")],
    )
    .await
    .expect_err("a string does not bind to int4");

    tx.commit()
        .await
        .expect("the block is healthy — these never reached the server");
    assert_eq!(count_outside(&c, &t).await, 1, "the good insert committed");
}

/// The row cap must stop *reading* at the ceiling rather than materialising
/// the whole result and then rejecting it.
#[tokio::test]
async fn the_row_cap_stops_reading_rather_than_collecting_first() {
    let c = connector_or_skip!();
    let mut tx = c.begin_transaction().await.expect("begin");

    // 5M rows would be an obvious memory event if this collected before
    // checking; streaming bails just past the ceiling.
    let err = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        tx.query("SELECT generate_series(1, 5000000) AS n", &[]),
    )
    .await
    .expect("must bail early, not grind through 5M rows")
    .expect_err("over the ceiling")
    .to_string();
    tx.rollback().await.expect("close");

    assert!(err.contains("LIMIT"), "names the fix: {err}");
}

/// The documented `::text` remedy must actually work **inside** the same
/// transaction. It only does because an undecodable column is rejected from the
/// statement's metadata before any row is read, leaving the block healthy —
/// discovering it mid-stream would mean abandoning a running query, which ends
/// the transaction and makes the advice we print unusable where we print it.
#[tokio::test]
async fn the_documented_cast_remedy_works_inside_the_same_transaction() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_abandon_t", "amount numeric(10,2)").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    tx.exec(
        &format!("INSERT INTO {t} (amount) VALUES ($1::text::numeric)"),
        &[json!("12.50")],
    )
    .await
    .expect("insert");

    let err = tx
        .query(&format!("SELECT amount FROM {t}"), &[])
        .await
        .expect_err("numeric is not directly decodable")
        .to_string();
    assert!(err.contains("::text"), "names the fix: {err}");

    // Apply exactly the fix the error named, in the same transaction.
    let rows = tx
        .query(&format!("SELECT amount::text AS amount FROM {t}"), &[])
        .await
        .expect("the named fix must work here — that is where the author is standing");
    assert_eq!(rows[0]["amount"].as_str(), Some("12.50"));

    tx.commit()
        .await
        .expect("the block was never poisoned — no row was ever read");
    assert_eq!(count_outside(&c, &t).await, 1, "the insert committed");
}

/// Cancelling the abandoned read is what keeps the pinned connection usable:
/// without it the server keeps producing rows, tokio-postgres drains them on
/// the driver, and the ROLLBACK queues behind that drain for up to
/// `statement_timeout`. Bounding the rollback is the whole point of this test.
#[tokio::test]
async fn a_rollback_after_an_abandoned_read_is_not_stalled_by_the_server() {
    let c = connector_or_skip!();
    let mut tx = c.begin_transaction().await.expect("begin");

    tx.query("SELECT generate_series(1, 20000000) AS n", &[])
        .await
        .expect_err("over the row cap");

    tokio::time::timeout(std::time::Duration::from_secs(10), tx.rollback())
        .await
        .expect("rollback must not queue behind 20M rows draining")
        .expect("rollback succeeds");
}

/// Aggregates, not money columns, are what authors hit first: `AVG`, `SUM`
/// over bigint/numeric, and a bare decimal literal all resolve to `numeric`.
/// The pre-flight rejects them, and the error names the *output* column
/// (`avg`), which is not a column that exists in the schema — so the docs have
/// to say this out loud.
#[tokio::test]
async fn aggregates_resolve_to_numeric_and_are_rejected_pre_flight() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_agg_t", "qty int4, amount numeric(10,2)").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    let err = tx
        .query(&format!("SELECT count(*) AS n, avg(qty) FROM {t}"), &[])
        .await
        .expect_err("avg() is numeric even over an int4 column")
        .to_string();
    assert!(err.contains("avg"), "names the output column: {err}");
    assert!(err.contains("::text"), "names the fix: {err}");

    // count(*) alone is int8 and fine — the failure is specific to numeric.
    tx.query(&format!("SELECT count(*) AS n FROM {t}"), &[])
        .await
        .expect("count(*) is int8");
    // And the cast works in place, because nothing was read.
    tx.query(&format!("SELECT avg(qty)::text AS avg FROM {t}"), &[])
        .await
        .expect("the named fix works here");
    tx.commit().await.expect("block never poisoned");
}

/// Consequence of checking metadata rather than values: a statement whose
/// SELECT list mentions an undecodable type is rejected even when it would
/// have returned no rows. Deterministic beats size-dependent, but it is a
/// behaviour change worth pinning so nobody "fixes" it back.
#[tokio::test]
async fn an_undecodable_column_is_rejected_even_when_no_rows_match() {
    let c = connector_or_skip!();
    let t = scratch_table(&c, "tx_emptynum_t", "id int4, amount numeric(10,2)").await;

    let mut tx = c.begin_transaction().await.expect("begin");
    let err = tx
        .query(
            &format!("SELECT amount FROM {t} WHERE id = $1"),
            &[json!(404)],
        )
        .await
        .expect_err("rejected from metadata, not from a value")
        .to_string();
    tx.rollback().await.expect("close");
    assert!(err.contains("amount"), "{err}");
}
