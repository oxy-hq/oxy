use sea_orm_migration::prelude::*;

/// Adds `provider` to `quickbooks_oauth_states`, so one state table can serve
/// every OAuth authorization-code provider (Google Drive is the second — see
/// `internal-docs/customer-apps-integrations.md`).
///
/// # Why the column, when the route already knows the provider
///
/// The callback path encodes the provider, so the handler could infer it
/// without reading the row. The column exists so the handler can **verify**
/// rather than infer: without it, a nonce minted for one provider can be
/// redeemed at another provider's callback. The `redirect_uri` on the row makes
/// that mostly unexploitable — the exchange would fail an exact-match check at
/// the vendor — but "mostly, via a second system's validation" is not the
/// guarantee to rest a consent flow on, and the check costs one comparison.
///
/// # The table keeps its name
///
/// `quickbooks_oauth_states` now holds rows for other providers, which reads
/// oddly. Renaming it is churn with a real downside: the table is referenced by
/// an entity, a service and a migration history, and renaming buys nothing a
/// doc comment cannot. The name is historical; the `provider` column is
/// authoritative.
///
/// # `NOT NULL DEFAULT 'quickbooks'` is a backfill, not an application default
///
/// Every row that exists when this runs is a QuickBooks state row, so the
/// default backfills them correctly and avoids a NULL window on a table the
/// live callback is reading. The entity declares the field as required, so
/// every Rust insert names it explicitly — the DB default should never be hit
/// again after this migration. It is deliberately not relied on as "the
/// provider you get if you forget", because forgetting would silently mint a
/// QuickBooks consent for a Google connect.
///
/// Rows are short-lived CSRF state (they expire, and are consumed once), so
/// `down` dropping the column loses nothing that matters.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE quickbooks_oauth_states
                ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'quickbooks';
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE quickbooks_oauth_states DROP COLUMN IF EXISTS provider",
            )
            .await?;
        Ok(())
    }
}
