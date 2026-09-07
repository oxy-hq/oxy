//! Kiosk devices — the binding a frontline PIN is only ever usable inside.
//!
//! A PIN is four to six digits. Offered as a bearer credential from any client
//! it is a brute-force target with a small keyspace, and the per-credential
//! lockout only bounds the damage per identifier. The design record
//! (`internal-docs/frontline-identity.md`) names device binding as required
//! before this faces a user: a PIN may be verified only for a request carrying
//! a credential that says "this is one of the tenant's enrolled kiosks".
//!
//! The row lives in two states. Created by an org admin it holds a one-time,
//! 24-hour **enrol token** (hashed); opening the enrol link on the kiosk
//! trades that for a long-lived **device secret** (hashed) that the browser
//! keeps in an HttpOnly cookie, and clears the token. Revocation is a
//! timestamp, never a delete: the audit trail of which kiosk a shift was
//! signed in on has to outlive the tablet.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE IF NOT EXISTS org_kiosk_devices (
                id                UUID PRIMARY KEY,
                org_id            UUID NOT NULL
                    REFERENCES organizations(id) ON DELETE CASCADE,
                name              TEXT NOT NULL,
                return_to         TEXT,
                enrol_token_hash  TEXT,
                enrol_expires_at  TIMESTAMPTZ,
                secret_hash       TEXT,
                created_by        UUID REFERENCES users(id) ON DELETE SET NULL,
                created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
                bound_at          TIMESTAMPTZ,
                last_seen_at      TIMESTAMPTZ,
                revoked_at        TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS org_kiosk_devices_org
                ON org_kiosk_devices (org_id, created_at DESC);
            CREATE UNIQUE INDEX IF NOT EXISTS org_kiosk_devices_enrol_token
                ON org_kiosk_devices (enrol_token_hash)
                WHERE enrol_token_hash IS NOT NULL;
            "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS org_kiosk_devices;")
            .await?;
        Ok(())
    }
}
