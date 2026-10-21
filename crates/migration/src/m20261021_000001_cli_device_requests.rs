//! `cli_device_requests` — the pending approvals of the device-code flow
//! (`oxyc login --no-browser`, `oxyc tokens create … --no-browser`; RFC 8628
//! in shape, PKCE-bound like the loopback flow's `cli_auth_codes`).
//!
//! An agent where no browser can reach it asks for a token; the row holds what
//! it asked for until a person approves it from any device, and until the CLI
//! collects what was approved. Control-plane data: a short-lived handshake
//! between Oxy's own CLI and Oxy's own web app, swept an hour after it lapses.
//!
//! - `device_code_hash` — SHA-256 of the secret the polling CLI holds. The
//!   code itself is never stored.
//! - `user_code` — the eight characters a person compares on the page. Unique
//!   while the row exists, so the page can find the request by it.
//! - `ask` — the `/cli-auth` parameters the loopback flow would have put in
//!   the URL (`kind`, `apps`, `hours`, `name`, `standing`), as sent.
//! - `status` — `pending`, `approved` or `denied`; `user_id` and `decided_at`
//!   are set by the decision, `mint` by an approval (the checked mint, as
//!   `cli_auth_codes.mint` holds it; null for a login), `denial` by a refusal.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS cli_device_requests (
    device_code_hash BYTEA PRIMARY KEY,
    user_code TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    hostname TEXT NOT NULL,
    ask JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    mint JSONB,
    denial TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    decided_at TIMESTAMPTZ,
    last_polled_at TIMESTAMPTZ,
    consumed_at TIMESTAMPTZ
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_cli_device_requests_user_code
    ON cli_device_requests (user_code);
CREATE INDEX IF NOT EXISTS idx_cli_device_requests_expires_at
    ON cli_device_requests (expires_at);
"#;

const DOWN_SQL: &str = "DROP TABLE IF EXISTS cli_device_requests;";

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
