//! `app_custom_domains` — a customer-owned hostname (`app.acme.com`) a custom
//! app's production environment is also served at.
//!
//! Control-plane data: which app a hostname routes to, and the proof that the
//! org adding it controls its DNS. No app content.
//!
//! - `domain` — lowercase ASCII (an internationalised name is stored in its
//!   punycode form), no port, no trailing dot.
//! - `verification_token` — the value the org publishes as a TXT record at
//!   `_oxygen-challenge.<domain>`. Random per row, so a claim another org made
//!   (or this org made and deleted) proves nothing for this one.
//! - `verified_at` — set once the TXT record matched. Only a verified row
//!   routes, issues a certificate, or opens a sign-in.
//! - `last_checked_at` / `last_check_error` — the last verification attempt,
//!   for the console to show why a domain is still pending.
//!
//! A hostname may be **claimed** by several apps at once — an org that typed a
//! domain first must not be able to squat it — but **verified** by one: the
//! partial unique index is the arbiter when two orgs prove the same name.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS app_custom_domains (
    id UUID PRIMARY KEY,
    app_id UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    domain TEXT NOT NULL,
    verification_token TEXT NOT NULL,
    verified_at TIMESTAMPTZ,
    last_checked_at TIMESTAMPTZ,
    last_check_error TEXT,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_app_custom_domains_app_domain
    ON app_custom_domains (app_id, domain);
CREATE UNIQUE INDEX IF NOT EXISTS idx_app_custom_domains_verified_domain
    ON app_custom_domains (domain) WHERE verified_at IS NOT NULL;
"#;

const DOWN_SQL: &str = "DROP TABLE IF EXISTS app_custom_domains;";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP_SQL).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(DOWN_SQL)
            .await?;
        Ok(())
    }
}
