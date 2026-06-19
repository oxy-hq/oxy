use oxy_shared::errors::OxyError;

/// Run ALL database migrations to the latest version.
///
/// Delegates to [`crate::cli::commands::serve::run_database_migrations`], which
/// runs every domain migrator (core SeaORM + runtime + analytics + workflow +
/// airway + airhouse + cameras) under a process-serialising Postgres advisory
/// lock — the SAME complete set `oxy serve` runs. The previous implementation
/// ran only the core `Migrator`, so a `migrate`-as-Job left the domain tables
/// uncreated.
///
/// `oxy migrate` is the dedicated one-shot entrypoint — e.g. a Helm
/// pre-install/pre-upgrade Job — so the serving pods can boot with
/// `OXY_SKIP_MIGRATIONS=1` instead of each racing the migrator.
pub async fn migrate() -> Result<(), OxyError> {
    crate::cli::commands::serve::run_database_migrations(false).await
}
