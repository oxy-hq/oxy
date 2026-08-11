use sea_orm_migration::prelude::*;

/// Airway's **operational tier** — `airway::config::global::GlobalConfig`, the
/// settings that are installed once per process rather than resolved per run.
///
/// # Why this is a different table from `airway_source_config`
///
/// `airway_source_config` is the **policy tier**: `contract_policy` and
/// `environment`, per source kind, with a per-workspace override, resolved on
/// every run by `agentic_pipeline::airway_config::resolve_admission`. This
/// table is the operational tier: seven settings that airway installs into a
/// process-wide `OnceLock` and every transport then reads. Two different
/// scopes, two different lifetimes, two tables — merging them would give the
/// per-source-kind row a column that is meaningless per kind.
///
/// # Singleton, enforced in the schema
///
/// `id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1)`. The primary key forbids
/// a second row with the same id and the `CHECK` forbids any id but `1`, so
/// "at most one row" is a property Postgres holds rather than a convention the
/// application remembers. `DEFAULT 1` means a writer never has to name the id.
/// Pinned by `singleton_constraint_rejects_a_second_row` in
/// `crates/app/src/server/api/admin/airway_config/deployment_tests.rs`.
///
/// A partial unique index on a constant (the trick
/// `airway_source_config_global_uniq` uses for its `workspace_id IS NULL` row)
/// would work too, but it admits `id = 7` and then has to explain what that
/// row means. A `CHECK` says "there is one row and its name is 1".
///
/// # Every setting is nullable, and NULL means airway's default
///
/// **NULL is not zero.** An unset column means "leave airway's compiled-in
/// value alone" — it maps to `None` on the matching `GlobalConfig` field, and
/// `GlobalConfig::apply_to_http` / `apply_to_retry` are gap-fillers, so a
/// `None` leaves `HttpConfig`/`RetryConfig`'s own constant in place. A `0`
/// stored here is a *chosen* zero and airway rejects it for the three
/// durations (a zero deadline has already passed; a zero backoff cap
/// collapses every delay; a zero first delay is a hot retry loop). There is no
/// Oxy-side default constant anywhere in this feature, deliberately: one would
/// drift from upstream's the first time airway changed a built-in.
///
/// # Column names are airway's own key spellings, units included
///
/// `timeout_secs`, `retry_initial_delay_ms`, `retry_max_delay_secs` — the
/// names come from `airway::config::global`'s `pub const` key roster and carry
/// the unit in the name, which is the single stated unit for each duration all
/// the way out to the admin UI's field label. `agentic_airway::deployment_config`
/// reads this table with a hand-written `SELECT` (it is barred from depending
/// on `entity`), so the column list is pinned against the entity by
/// `entity_columns_match_the_airway_key_roster`.
///
/// # The `>= 0` checks are type-domain guards, not policy
///
/// `max_retries` is `u32` and the three durations are `Duration` on the airway
/// side; Postgres has no unsigned integer, so a negative would be a value the
/// Rust type cannot hold. These checks keep the column inside its type's
/// domain. Everything semantic — zero, an empty `user_agent`, a backoff factor
/// below 1, half an mTLS identity — stays `GlobalConfig::validate`'s job and is
/// enforced on the write path, so there is exactly one place that decides what
/// a *meaningful* value is.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE IF NOT EXISTS airway_deployment_config (
                -- Singleton: the PK forbids a duplicate id, the CHECK forbids
                -- any id but 1. Together that is "at most one row", held by
                -- Postgres rather than by convention.
                id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),

                -- Every setting NULLable. NULL = "airway's built-in default",
                -- never 0 / disabled. See this migration's doc comment.
                timeout_secs BIGINT
                    CHECK (timeout_secs IS NULL OR timeout_secs >= 0),
                max_retries INTEGER
                    CHECK (max_retries IS NULL OR max_retries >= 0),
                user_agent TEXT,
                retry_initial_delay_ms BIGINT
                    CHECK (retry_initial_delay_ms IS NULL OR retry_initial_delay_ms >= 0),
                retry_max_delay_secs BIGINT
                    CHECK (retry_max_delay_secs IS NULL OR retry_max_delay_secs >= 0),
                retry_backoff_factor DOUBLE PRECISION,

                -- `tls` is one `GlobalConfig` field (`Option<TlsConfig>`) spread
                -- over four columns, matching airway's four flat `tls_*` keys.
                -- `tls_server_name` and `tls_enabled` are deliberately absent:
                -- airway withholds both because its only consumer
                -- (`TlsConfig::to_reqwest_client_builder`) cannot honour them,
                -- and a column for a key airway refuses is a knob that does
                -- nothing.
                tls_ca_cert TEXT,
                tls_client_cert TEXT,
                tls_client_key_file TEXT,
                tls_danger_accept_invalid_certs BOOLEAN,

                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS airway_deployment_config CASCADE")
            .await?;
        Ok(())
    }
}
