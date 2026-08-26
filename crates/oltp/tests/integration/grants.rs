//! The grant model, asserted against a real Postgres.
//!
//! These are the cases that broke in development and could not have been
//! caught by unit-testing the generated SQL, because the SQL was well-formed
//! every time — Postgres just refused to do what it looked like it did.

use crate::{Fixture, admin_dsn, attempt};

use oxy_oltp::provider::OltpProvider;
use oxy_oltp::schema::{self, GrantLevel, WriterRef};
use oxy_oltp::sql::{PgSqlExecutor, TenantSqlExecutor};

#[tokio::test]
async fn a_writer_is_confined_to_its_own_schema() {
    let Some(fx) = Fixture::create("confine").await else {
        return;
    };
    let app = fx.app("bookings");
    let pipeline = fx.pipeline("toast");
    let app_dsn = fx.add_writer(&app).await;
    let pipeline_dsn = fx.add_writer(&pipeline).await;

    attempt(&pipeline_dsn, "CREATE TABLE sales (id int)")
        .await
        .expect("a writer may create in its own schema");

    // The containment that makes schema-per-writer a boundary rather than a
    // naming convention.
    let err = attempt(
        &app_dsn,
        &format!("SELECT * FROM {}.sales", pipeline.schema_name()),
    )
    .await
    .expect_err("app must not reach the pipeline's schema");
    assert!(err.contains("permission denied"), "got {err}");

    let err = attempt(&app_dsn, "CREATE TABLE public.escape (id int)")
        .await
        .expect_err("no writer may create in public");
    assert!(err.contains("permission denied"), "got {err}");

    // Oxy owns the schema so a compromised writer cannot drop it.
    let err = attempt(
        &app_dsn,
        &format!("DROP SCHEMA {} CASCADE", app.schema_name()),
    )
    .await
    .expect_err("writer must not drop its schema");
    assert!(err.contains("must be owner"), "got {err}");

    fx.cleanup(&[&app, &pipeline]).await;
}

/// The confinement check must actually FIRE on a contaminated role — in CI,
/// without Neon.
///
/// `neon_live` proves the real service grants `neon_superuser` to every role its
/// API creates, which is why roles are minted in SQL. But that test is opt-in,
/// needs credentials and spends money, so it never runs here — and what local
/// Postgres cannot produce is that *membership*. The check's attribute half
/// (`rolsuper`, `rolcreaterole`, …) is reachable on any cluster; the group
/// membership half, which is the half the Neon incident was actually about, had
/// no failing-direction coverage anywhere CI could run.
///
/// So this stands in for the provider: a group role plays `neon_superuser`, and
/// granting it to a login role is exactly what Neon's API does behind the
/// scenes. What it verifies is OUR half — that the check detects unexpected
/// group membership and raises `OXY01`. What it cannot verify is that Neon
/// still behaves that way; only the live test can, and it is the reason this
/// one is not the whole story.
#[tokio::test]
async fn the_confinement_check_catches_a_role_the_provider_contaminated() {
    let Some(fx) = Fixture::create("contaminated").await else {
        return;
    };
    let role = format!("{}_contaminated", fx.owner_role);
    let group = format!("{}_pretend_superuser", fx.owner_role);

    // Start from a known-clean slate, because a red run cannot reach the
    // teardown at the bottom.
    //
    // Both roles are cluster-global, so they outlive the fixture database, and
    // every assertion below runs BEFORE the teardown. Dropping only the group
    // here would be too late for the login role: `ensure_login_role_sql` is
    // ensure-shaped, so it happily re-uses a leftover role WITH its leftover
    // membership intact — and the very first assertion, that a SQL-minted role
    // is confined, would then fail on contamination the previous run left. That
    // is the poisoning this guards against, and it lands one assertion earlier
    // than the `CREATE ROLE` collision.
    // Dropped on the ADMIN connection, not the owner's. The fixture recreates
    // the tenant owner every run, and a role created by the PREVIOUS owner is
    // orphaned once that owner is dropped: the new owner holds `CREATEROLE` but
    // no ADMIN option on it, so its `DROP ROLE` fails `42501` — silently, since
    // this is best-effort — and the `CREATE ROLE` below then fails for real.
    // Only the superuser can clear a leftover from an earlier run.
    for name in [&role, &group] {
        let _ = attempt(&admin_dsn(), &format!("DROP ROLE IF EXISTS \"{name}\"")).await;
    }

    // A role minted the way Oxy does it passes.
    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &oxy_oltp::roles::ensure_login_role_sql(&role, "correct-horse-battery")
                .expect("build role sql"),
        )
        .await
        .expect("mint the role");
    attempt(
        &fx.owner_dsn,
        &oxy_oltp::roles::assert_confined_sql(&role).expect("build assert"),
    )
    .await
    .expect("a SQL-minted role is confined");

    // Now contaminate it the way the provider would.
    attempt(&fx.owner_dsn, &format!("CREATE ROLE \"{group}\""))
        .await
        .expect("create the stand-in group");
    attempt(&fx.owner_dsn, &format!("GRANT \"{group}\" TO \"{role}\""))
        .await
        .expect("grant it, as the provider's API does");

    let err = attempt(
        &fx.owner_dsn,
        &oxy_oltp::roles::assert_confined_sql(&role).expect("build assert"),
    )
    .await
    .expect_err("a role carrying unexpected membership must not pass");
    assert!(
        err.contains("OXY01"),
        "the check must raise OUR error, not fail incidentally: {err}"
    );
    assert!(
        err.contains("not confined"),
        "and it must name what is wrong: {err}"
    );

    // Tidy up on the green path; the slate at the top covers the red one.
    for sql in [
        format!("DROP ROLE IF EXISTS \"{role}\""),
        format!("DROP ROLE IF EXISTS \"{group}\""),
    ] {
        let _ = attempt(&admin_dsn(), &sql).await;
    }
    fx.cleanup(&[]).await;
}

/// `REPLICATION` must fail the confinement check.
///
/// The attribute list in `assert_confined_sql` is Neon's, read from their
/// open-source `compute_ctl`: a spec-created role is `CREATE ROLE x INHERIT
/// CREATEROLE CREATEDB BYPASSRLS REPLICATION IN ROLE neon_superuser`. Four
/// attributes and a membership — and the check tested only three of the four.
///
/// `REPLICATION` is the one that was missing and the one that matters most
/// here. A role holding it can open a replication connection and stream the
/// database's whole WAL, which carries every other writer's rows. That reads
/// PAST table ACLs rather than through them, so a single unconfined writer
/// would see the entire org's OLTP data while every `GRANT` in the system still
/// looked correct.
///
/// Unlike the `neon_superuser` membership, this needs no stand-in: `REPLICATION`
/// is a plain Postgres attribute, so the failing direction is reachable on any
/// cluster.
#[tokio::test]
async fn the_confinement_check_catches_a_replication_role() {
    let Some(fx) = Fixture::create("replication").await else {
        return;
    };
    let role = format!("{}_replicator", fx.owner_role);
    // Admin connection: see the slate in the contaminated test above.
    let _ = attempt(&admin_dsn(), &format!("DROP ROLE IF EXISTS \"{role}\"")).await;

    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &oxy_oltp::roles::ensure_login_role_sql(&role, "correct-horse-battery")
                .expect("build role sql"),
        )
        .await
        .expect("mint the role");
    attempt(
        &fx.owner_dsn,
        &oxy_oltp::roles::assert_confined_sql(&role).expect("build assert"),
    )
    .await
    .expect("a SQL-minted role is confined");

    // Exactly what Neon's spec-apply hands a role — and, like Neon, granted by
    // something more privileged than the tenant owner. Setting `REPLICATION`
    // requires superuser: the owner holds `CREATEROLE` and still gets `42501`,
    // which is itself the shape of the hazard. The provider can confer an
    // authority the tenant could never confer on itself, and only the
    // confinement check stands between that and a handed-out credential.
    attempt(&admin_dsn(), &format!("ALTER ROLE \"{role}\" REPLICATION"))
        .await
        .expect("grant replication as the superuser");

    let err = attempt(
        &fx.owner_dsn,
        &oxy_oltp::roles::assert_confined_sql(&role).expect("build assert"),
    )
    .await
    .expect_err("a role holding REPLICATION must not pass");
    assert!(
        err.contains("OXY01"),
        "the check must raise OUR error: {err}"
    );
    assert!(
        err.contains("replication"),
        "and it must name the attribute, so an operator knows what to strip: {err}"
    );

    let _ = attempt(&admin_dsn(), &format!("DROP ROLE IF EXISTS \"{role}\"")).await;
    fx.cleanup(&[]).await;
}

/// A writer must not be able to squat the namespace of one that does not exist
/// yet.
///
/// Pipeline writers hold `CREATE ON DATABASE` by design — Airway needs to make
/// its own `raw_*` schemas. That same grant lets one create `app_bookings`
/// before that app is provisioned, and `CREATE SCHEMA IF NOT EXISTS` is a no-op
/// on an existing schema, so the squatter stays its OWNER: able to read, alter
/// and drop everything the app later puts there, while every grant Oxy issues
/// still succeeds and reads as correct.
///
/// Refusing is the only option available — `ALTER SCHEMA … OWNER TO` needs
/// membership the tenant owner does not have on every provider — so this pins
/// that provisioning stops loudly rather than granting into a namespace Oxy
/// does not own.
#[tokio::test]
async fn a_writer_cannot_squat_another_writers_schema_name() {
    let Some(fx) = Fixture::create("squat").await else {
        return;
    };
    let pipeline = fx.pipeline("toast");
    let app = fx.app("bookings");
    let pipeline_dsn = fx.add_writer(&pipeline).await;

    // The squat: legitimate SQL for this role, and the schema name of an app
    // that has not been provisioned yet.
    attempt(
        &pipeline_dsn,
        &format!("CREATE SCHEMA {}", app.schema_name()),
    )
    .await
    .expect("a pipeline writer may create schemas — that is the grant it holds");

    // Provisioning the app must now refuse rather than grant into it.
    let err = PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &oxy_oltp::schema::ensure_writer_sql(
                &app,
                GrantLevel::ReadWrite,
                &fx.owner_role,
                &fx.analyst_role(),
            )
            .expect("build writer sql"),
        )
        .await
        .expect_err("provisioning into a squatted schema must fail");

    let err = err.to_string();
    assert!(
        err.contains("OXY02") || err.contains("is owned by"),
        "the failure must be OUR ownership check, not an unrelated error: {err}"
    );

    fx.cleanup(&[&app, &pipeline]).await;
}

#[tokio::test]
async fn a_migration_created_table_is_usable_by_the_app() {
    let Some(fx) = Fixture::create("migrationgrants").await else {
        return;
    };
    let app = fx.app("bookings");
    let app_dsn = fx.add_writer(&app).await;

    // A migration runs as the OWNER, so the table it creates is owned by the
    // owner — not the writer. Without reconciled grants the app cannot touch
    // its own table, silently, until it queries. This is the bug that would
    // have made the whole feature useless in production.
    fx.as_owner(&format!(
        "CREATE TABLE {}.customers (
             id BIGSERIAL PRIMARY KEY,
             email TEXT NOT NULL
         )",
        app.schema_name()
    ))
    .await;

    let before = attempt(&app_dsn, "INSERT INTO customers (email) VALUES ('a@b.c')").await;
    assert!(
        before.is_err(),
        "precondition: an owner-created table starts ungranted"
    );

    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &schema::reconcile_migration_grants_sql(
                &app,
                &fx.owner_role,
                false,
                &fx.analyst_role(),
                &fx.role_name(&app),
            )
            .unwrap(),
        )
        .await
        .expect("reconcile grants");

    // BIGSERIAL is backed by a sequence — an INSERT fails on the sequence even
    // when the table grant is right, which is why the reconcile covers both.
    attempt(&app_dsn, "INSERT INTO customers (email) VALUES ('a@b.c')")
        .await
        .expect("app can insert after grants reconcile");

    fx.cleanup(&[&app]).await;
}

#[tokio::test]
async fn reconciling_grants_does_not_error_on_writer_owned_tables() {
    let Some(fx) = Fixture::create("mixedowners").await else {
        return;
    };
    let app = fx.app("bookings");
    let app_dsn = fx.add_writer(&app).await;

    // A schema with BOTH a writer-owned and an owner-owned table. The first
    // implementation used `GRANT ... ON ALL TABLES IN SCHEMA`, which errors
    // here: the owner has no grant option on the writer's table.
    attempt(&app_dsn, "CREATE TABLE writer_owned (id int)")
        .await
        .expect("writer creates its own table");
    fx.as_owner(&format!(
        "CREATE TABLE {}.owner_owned (id int)",
        app.schema_name()
    ))
    .await;

    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &schema::reconcile_migration_grants_sql(
                &app,
                &fx.owner_role,
                false,
                &fx.analyst_role(),
                &fx.role_name(&app),
            )
            .unwrap(),
        )
        .await
        .expect("reconcile must tolerate a mixed-ownership schema");

    fx.cleanup(&[&app]).await;
}

#[tokio::test]
async fn the_analyst_reads_only_what_it_is_granted_and_never_writes() {
    let Some(fx) = Fixture::create("analyst").await else {
        return;
    };
    let pipeline = fx.pipeline("toast");
    let app = fx.app("bookings");
    let pipeline_dsn = fx.add_writer(&pipeline).await;
    fx.add_writer(&app).await;

    attempt(&pipeline_dsn, "CREATE TABLE sales (id int, amount numeric)")
        .await
        .expect("pipeline creates its table");

    // The provider mints the analyst login; the platform step made the role.
    let analyst = fx
        .provider
        .create_role(&fx.database, "local", &fx.analyst_role())
        .await
        .expect("analyst role");
    let analyst_dsn = format!(
        "postgres://{}:{}@{}/{}?sslmode=disable",
        fx.analyst_role(),
        oxy_oltp::roles::encode_userinfo(&analyst.password.unwrap()),
        crate::host(),
        fx.database
    );

    // raw_* is analyst-visible by default; app_* is not.
    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &schema::grant_analyst_schema_sql(&pipeline, &fx.analyst_role()),
        )
        .await
        .unwrap();
    PgSqlExecutor
        .execute_batch(
            &pipeline_dsn,
            &schema::grant_analyst_tables_sql(&pipeline, &fx.analyst_role()),
        )
        .await
        .unwrap();

    attempt(
        &analyst_dsn,
        &format!("SELECT * FROM {}.sales", pipeline.schema_name()),
    )
    .await
    .expect("analyst reads opted-in ETL data");

    let err = attempt(
        &analyst_dsn,
        &format!("SELECT * FROM {}.anything", app.schema_name()),
    )
    .await
    .expect_err("app_* stays hidden without an opt-in");
    assert!(err.contains("permission denied"), "got {err}");

    // The invariant the whole design rests on: no human connection can write.
    let err = attempt(
        &analyst_dsn,
        &format!("INSERT INTO {}.sales VALUES (1, 2)", pipeline.schema_name()),
    )
    .await
    .expect_err("analyst must never write");
    assert!(err.contains("permission denied"), "got {err}");

    // Oxy's own bookkeeping is invisible to it.
    let err = attempt(&analyst_dsn, "SELECT * FROM oxy_meta.schema_migrations")
        .await
        .expect_err("analyst must not read oxy_meta");
    assert!(err.contains("permission denied"), "got {err}");

    fx.cleanup(&[&app, &pipeline]).await;
}

/// Opting an `app_*` schema into analytics has to cover the tables a
/// *migration* made, which the owner owns — not just the ones the writer made.
///
/// The writer-side `GRANT SELECT ON ALL TABLES` cannot touch those: you must
/// own an object to grant on it, and Postgres answers a grant you have no
/// grant option for with a **warning, not an error**. So the opt-in reported
/// success and the analyst still got `permission denied` on every real table.
#[tokio::test]
async fn opting_in_covers_tables_a_migration_created() {
    let Some(fx) = Fixture::create("analystmigration").await else {
        return;
    };
    let app = fx.app("bookings");
    fx.add_writer(&app).await;

    // As a migration does it: owner-owned, which is the whole point.
    fx.as_owner(&format!(
        "CREATE TABLE {}.orders (id bigserial primary key, total numeric)",
        app.schema_name()
    ))
    .await;

    let analyst = fx
        .provider
        .create_role(&fx.database, "local", &fx.analyst_role())
        .await
        .expect("analyst role");
    let analyst_dsn = format!(
        "postgres://{}:{}@{}/{}?sslmode=disable",
        fx.analyst_role(),
        oxy_oltp::roles::encode_userinfo(&analyst.password.unwrap()),
        crate::host(),
        fx.database
    );

    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &schema::grant_analyst_schema_sql(&app, &fx.analyst_role()),
        )
        .await
        .unwrap();
    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &schema::grant_analyst_owner_tables_sql(&app, &fx.owner_role, &fx.analyst_role())
                .unwrap(),
        )
        .await
        .unwrap();

    attempt(
        &analyst_dsn,
        &format!("SELECT * FROM {}.orders", app.schema_name()),
    )
    .await
    .expect("analyst reads a migration-created table after opt-in");

    // Opting back out must actually withdraw it. Revoking schema USAGE alone
    // would mask the access while leaving the SELECT grant in place, so the
    // next opt-in would restore reads this revoke claimed to have removed.
    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &schema::revoke_analyst_owner_tables_sql(&app, &fx.owner_role, &fx.analyst_role())
                .unwrap(),
        )
        .await
        .unwrap();

    let err = attempt(
        &analyst_dsn,
        &format!("SELECT * FROM {}.orders", app.schema_name()),
    )
    .await
    .expect_err("opt-out must withdraw the table grant, not just hide it");
    assert!(err.contains("permission denied"), "got {err}");

    fx.cleanup(&[&app]).await;
}

#[tokio::test]
async fn future_tables_are_covered_without_re_running_the_reconcile() {
    let Some(fx) = Fixture::create("defaultprivs").await else {
        return;
    };
    let app = fx.app("bookings");
    let app_dsn = fx.add_writer(&app).await;

    // Reconcile BEFORE the table exists. The ALTER DEFAULT PRIVILEGES half is
    // what has to carry it — otherwise every migration would need a follow-up
    // grant pass forever.
    PgSqlExecutor
        .execute_batch(
            &fx.owner_dsn,
            &schema::reconcile_migration_grants_sql(
                &app,
                &fx.owner_role,
                false,
                &fx.analyst_role(),
                &fx.role_name(&app),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    fx.as_owner(&format!(
        "CREATE TABLE {}.later (id BIGSERIAL PRIMARY KEY, note TEXT)",
        app.schema_name()
    ))
    .await;

    attempt(
        &app_dsn,
        "INSERT INTO later (note) VALUES ('after the fact')",
    )
    .await
    .expect("default privileges cover a table created later");

    fx.cleanup(&[&app]).await;
}

#[tokio::test]
async fn search_path_resolves_unqualified_names_to_the_writers_own_schema() {
    let Some(fx) = Fixture::create("searchpath").await else {
        return;
    };
    let app = fx.app("bookings");
    let pipeline = fx.pipeline("toast");
    let app_dsn = fx.add_writer(&app).await;
    let pipeline_dsn = fx.add_writer(&pipeline).await;

    // Same table name in two schemas. Unqualified, each writer must hit its
    // own — this is containment, not convenience.
    attempt(&app_dsn, "CREATE TABLE orders (id int, who text)")
        .await
        .unwrap();
    attempt(&pipeline_dsn, "CREATE TABLE orders (id int, who text)")
        .await
        .unwrap();

    attempt(&app_dsn, "INSERT INTO orders VALUES (1, 'app')")
        .await
        .unwrap();

    // Read back as each WRITER, not the owner: these tables are writer-owned,
    // and the owner has no SELECT on them — which is itself the containment
    // working.
    let count = |dsn: String, schema: String| async move {
        let (client, conn) = tokio_postgres::connect(&dsn, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        client
            .query(&format!("SELECT who FROM {schema}.orders"), &[])
            .await
            .unwrap()
            .len()
    };

    assert_eq!(
        count(app_dsn.clone(), app.schema_name()).await,
        1,
        "the row landed in the app's schema"
    );
    let other: Vec<()> = if count(pipeline_dsn.clone(), pipeline.schema_name()).await == 0 {
        vec![]
    } else {
        vec![()]
    };
    assert!(
        other.is_empty(),
        "and not in the pipeline's same-named table"
    );

    fx.cleanup(&[&app, &pipeline]).await;
}

#[tokio::test]
async fn an_injection_shaped_writer_name_is_rejected_before_any_sql() {
    // The generators never see this, because construction refuses it.
    for bad in [
        "a\"; DROP SCHEMA public; --",
        "a'; DROP TABLE x; --",
        "has space",
        "Has-Upper",
    ] {
        assert!(
            WriterRef::app(bad).is_err(),
            "expected rejection of {bad:?}"
        );
    }
    assert!(
        schema::reconcile_migration_grants_sql(
            &WriterRef::app("ok").unwrap(),
            "bad\"owner",
            false,
            "oxy_analyst_ro",
            "app_ok_rw"
        )
        .is_err(),
        "an invalid owner role must be refused too"
    );
    let _ = GrantLevel::ReadWrite;
}
