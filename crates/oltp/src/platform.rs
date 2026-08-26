//! **The single declaration of what every tenant database must contain.**
//!
//! Database-per-tenant means Oxy owns objects inside N databases — the ledger
//! schema, the analyst role, baseline grants — and every change to them is a
//! fan-out. The tax is unavoidable; what makes it survivable is that the
//! expected state is declared in exactly one place, versioned, and recorded
//! per tenant in Oxy's *own* database.
//!
//! That last part is what lets Oxy answer "which tenants are behind?" with a
//! single query against `oltp_tenants.platform_schema_version`, rather than
//! connecting to every tenant to find out.
//!
//! # Rules for adding a step
//!
//! 1. **Append only.** Never edit a shipped step — tenants that already ran it
//!    will not run it again, so an edit silently applies to new tenants only.
//! 2. **Idempotent.** Every statement must tolerate being re-run (`IF NOT
//!    EXISTS`, or a `GRANT` that is already held). Reconcile then repairs drift
//!    rather than only filling gaps.
//! 3. **Bump [`PLATFORM_SCHEMA_VERSION`].** A test fails if it doesn't match
//!    the step count, so this can't be forgotten.
//!
//! Note the asymmetry with app migrations ([`crate::schema`] docs): those are
//! forward-only because reverting a build can't un-drop a column. These are
//! idempotent *and* forward-only, because a tenant may be many versions behind.

use crate::schema::{SchemaError, analyst_role_for, validate_name};

/// Version of the Oxy-owned objects a tenant database is expected to carry.
///
/// Recorded on `oltp_tenants.platform_schema_version`. A tenant below this is
/// reconciled the next time anything touches it.
pub const PLATFORM_SCHEMA_VERSION: i32 = 3;

/// Table recording which workspace migrations a tenant has applied.
///
/// Distinct from the earlier `app_migrations`: schemas are authored per
/// **workspace** and keyed by `file_path`, not per app. `app_migrations`
/// survives because platform steps are append-only, but nothing writes it.
pub const MIGRATIONS_TABLE: &str = "oxy_meta.schema_migrations";

/// Schema holding Oxy-owned bookkeeping inside a tenant database.
///
/// Deliberately **not** owned by any writer: an app owns `app_<slug>` and can
/// do as it likes there, but it must not be able to rewrite its own migration
/// history.
pub const META_SCHEMA: &str = "oxy_meta";

/// One versioned change to the Oxy-owned state of a tenant database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformStep {
    /// The version a tenant is at once this step has been applied.
    pub version: i32,
    pub description: &'static str,
    pub statements: Vec<String>,
}

/// Every step, in order. `steps[i].version == i + 1`.
/// `provider` is not decoration: on a cluster where tenants **share a role
/// namespace** (`local`), every role name carries a per-tenant tag, so the
/// analyst here is `oxy_analyst_ro_<tag>` and the bare `oxy_analyst_ro` is a
/// decoy. Hardening the bare name meant `REVOKE CREATE ON SCHEMA public` and
/// `REVOKE ALL ON SCHEMA oxy_meta` landed on a role nothing logs in as, while
/// the role that actually serves queries kept whatever it was created with.
pub fn platform_steps(provider: &str, database: &str) -> Result<Vec<PlatformStep>, SchemaError> {
    validate_name(database)?;
    let db = quote(database);
    let meta = quote(META_SCHEMA);
    let analyst_name = analyst_role_for(provider, database);
    let analyst = quote(&analyst_name);

    Ok(vec![
        PlatformStep {
            version: 1,
            description: "baseline hardening, oxy_meta ledger, analyst role",
            statements: vec![
                // ── Baseline hardening ──────────────────────────────────────────
                // Postgres 15+ already revokes CREATE on public from PUBLIC, but
                // stating it explicitly means the invariant holds regardless of the
                // provider's template or server version.
                format!("REVOKE ALL ON DATABASE {db} FROM PUBLIC"),
                "REVOKE CREATE ON SCHEMA public FROM PUBLIC".to_string(),
                // ── Oxy-owned bookkeeping ───────────────────────────────────────
                format!("CREATE SCHEMA IF NOT EXISTS {meta}"),
                format!(
                    "CREATE TABLE IF NOT EXISTS {meta}.app_migrations (
                     app_id      UUID        NOT NULL,
                     version     INTEGER     NOT NULL,
                     name        TEXT        NOT NULL,
                     checksum    TEXT        NOT NULL,
                     applied_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                     PRIMARY KEY (app_id, version)
                 )"
                ),
                // ── Analyst role ────────────────────────────────────────────────
                // NOLOGIN: the provider mints the login credential. Created with no
                // schema grants at all, so a fresh database exposes nothing until a
                // writer is explicitly published to analytics.
                format!(
                    "DO $$ BEGIN \
                   IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{analyst_name}') THEN \
                     CREATE ROLE {analyst} NOLOGIN; \
                   END IF; \
                 END $$"
                ),
                format!("REVOKE CREATE ON SCHEMA public FROM {analyst}"),
                // The analyst must never see Oxy's own bookkeeping.
                format!("REVOKE ALL ON SCHEMA {meta} FROM {analyst}"),
            ],
        },
        PlatformStep {
            version: 2,
            description: "workspace schema-migration ledger",
            statements: vec![format!(
                "CREATE TABLE IF NOT EXISTS {meta}.schema_migrations (
                 file_path   TEXT        NOT NULL PRIMARY KEY,
                 checksum    TEXT        NOT NULL,
                 revision_id UUID,
                 applied_at  TIMESTAMPTZ NOT NULL DEFAULT now()
             )"
            )],
        },
        PlatformStep {
            version: 3,
            description: "harden the tenant-qualified analyst role",
            // Step 1 built these statements against the BARE `oxy_analyst_ro`,
            // so every tenant provisioned before this ran them on a decoy. The
            // statements are identical and idempotent; a tenant provisioned
            // after the fix runs them a second time and nothing changes.
            //
            // A new step rather than an edit to step 1: a tenant already at
            // version 2 never re-runs step 1, so editing it would fix new
            // tenants only — which is the half that was never broken.
            statements: vec![
                format!(
                    "DO $$ BEGIN \
                   IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{analyst_name}') THEN \
                     CREATE ROLE {analyst} NOLOGIN; \
                   END IF; \
                 END $$"
                ),
                format!("REVOKE CREATE ON SCHEMA public FROM {analyst}"),
                format!("REVOKE ALL ON SCHEMA {meta} FROM {analyst}"),
            ],
        },
    ])
}

/// Steps needed to bring a tenant from `from_version` up to current.
///
/// `from_version` of 0 means a freshly provisioned database, which therefore
/// gets the whole list — a new tenant is correct by construction rather than by
/// remembering to run something extra at provision time.
pub fn steps_since(
    from_version: i32,
    provider: &str,
    database: &str,
) -> Result<Vec<PlatformStep>, SchemaError> {
    Ok(platform_steps(provider, database)?
        .into_iter()
        .filter(|s| s.version > from_version)
        .collect())
}

/// Flattened statements for [`steps_since`], ready for a
/// [`crate::sql::TenantSqlExecutor`] batch.
pub fn statements_since(
    from_version: i32,
    provider: &str,
    database: &str,
) -> Result<Vec<String>, SchemaError> {
    Ok(steps_since(from_version, provider, database)?
        .into_iter()
        .flat_map(|s| s.statements)
        .collect())
}

fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_version_matches_the_step_list() {
        let steps = platform_steps("neon", "neondb").unwrap();
        assert_eq!(
            steps.len() as i32,
            PLATFORM_SCHEMA_VERSION,
            "adding a step without bumping PLATFORM_SCHEMA_VERSION would leave \
             tenants recorded as up to date while missing the change"
        );
    }

    #[test]
    fn step_versions_are_dense_and_ordered_from_one() {
        for (i, step) in platform_steps("neon", "neondb").unwrap().iter().enumerate() {
            assert_eq!(step.version, i as i32 + 1, "step versions must be 1..=N");
        }
    }

    #[test]
    fn a_fresh_tenant_gets_every_step() {
        let all = platform_steps("neon", "neondb").unwrap();
        let fresh = steps_since(0, "neon", "neondb").unwrap();
        assert_eq!(fresh, all);
    }

    #[test]
    fn an_up_to_date_tenant_gets_nothing() {
        let none = steps_since(PLATFORM_SCHEMA_VERSION, "neon", "neondb").unwrap();
        assert!(none.is_empty(), "got {none:#?}");
    }

    #[test]
    fn every_statement_is_idempotent_shaped() {
        // Reconcile re-runs steps to repair drift, so a bare CREATE would fail
        // the second time and wedge the tenant.
        for step in platform_steps("neon", "neondb").unwrap() {
            for stmt in &step.statements {
                let creates_object = stmt.starts_with("CREATE SCHEMA")
                    || stmt.starts_with("CREATE TABLE")
                    || stmt.starts_with("CREATE INDEX");
                if creates_object {
                    assert!(
                        stmt.contains("IF NOT EXISTS"),
                        "non-idempotent statement: {stmt}"
                    );
                }
                if stmt.contains("CREATE ROLE") {
                    assert!(
                        stmt.contains("IF NOT EXISTS") || stmt.contains("pg_roles"),
                        "CREATE ROLE has no IF NOT EXISTS in Postgres; guard it: {stmt}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_analyst_cannot_read_oxys_own_bookkeeping() {
        let sql = statements_since(0, "neon", "neondb").unwrap().join("; ");
        assert!(
            sql.contains(&format!(
                r#"REVOKE ALL ON SCHEMA "{META_SCHEMA}" FROM "{}""#,
                analyst_role_for("neon", "neondb")
            )),
            "got {sql}"
        );
    }

    #[test]
    fn the_migration_ledger_is_keyed_per_app() {
        let sql = statements_since(0, "neon", "neondb").unwrap().join("; ");
        assert!(sql.contains("PRIMARY KEY (app_id, version)"), "got {sql}");
        assert!(
            sql.contains("checksum"),
            "editing a shipped migration must be detectable"
        );
    }

    #[test]
    fn an_invalid_database_name_is_rejected_rather_than_escaped() {
        assert!(platform_steps("neon", "neon\"db").is_err());
    }
}
