//! Custom-app usage tracking — two append-only tables for "is this
//! app being used and by whom?".
//!
//! See `internal-docs/customer-apps.md` §13 for the engineer-facing
//! overview of the feature; the architecture rationale (Postgres vs.
//! Airhouse, fire-and-forget vs. TaskSpec, etc.) lives in the module
//! docstrings of `customer_apps_tracking.rs`. TL;DR:
//!
//! - `custom_app_view_event` — one row per HTML serve of a custom-app
//!   bundle (the user "opened the app"). Recorded server-side from
//!   `customer_apps_serve::serve_dispatch` via `tokio::spawn`.
//! - `custom_app_event` — engineer-tagged events fired from inside
//!   the bundle via the SDK's `useTrackEvent("export-clicked", …)`
//!   hook. Free-form payload, engineer-chosen shape.
//!
//! Both cascade-delete with the `apps` row so the admin delete-app
//! flow (which already nukes the S3 bundle bytes — see
//! `customer_apps_build_store::delete_app`) also clears the
//! activity history. No orphan PII.
//!
//! Indexes are picked for the two Activity-tab queries the admin UI
//! actually runs:
//! - `(app_id, viewed_at DESC)` — recent visitors / headline numbers
//! - `(user_id, viewed_at DESC)` — operator filtering "what's <email>
//!   been opening?" cross-app from the Users admin (future)
//! - `(app_id, occurred_at DESC)` — recent custom events
//! - `(app_id, event_name)` — events table grouping in the UI

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum CustomAppViewEvent {
    Table,
    Id,
    AppId,
    UserId,
    UserEmail,
    SessionId,
    ViewedAt,
    Referrer,
    UserAgentClass,
    Source,
}

#[derive(DeriveIden)]
enum CustomAppEvent {
    Table,
    Id,
    AppId,
    UserId,
    UserEmail,
    SessionId,
    EventName,
    Payload,
    OccurredAt,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CustomAppViewEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CustomAppViewEvent::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CustomAppViewEvent::AppId).uuid().not_null())
                    .col(ColumnDef::new(CustomAppViewEvent::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(CustomAppViewEvent::UserEmail)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CustomAppViewEvent::SessionId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CustomAppViewEvent::ViewedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(CustomAppViewEvent::Referrer).text())
                    .col(
                        ColumnDef::new(CustomAppViewEvent::UserAgentClass)
                            .text()
                            .not_null()
                            .default("browser"),
                    )
                    .col(
                        ColumnDef::new(CustomAppViewEvent::Source)
                            .text()
                            .not_null()
                            .default("subpath"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_custom_app_view_event_app_id")
                            .from(CustomAppViewEvent::Table, CustomAppViewEvent::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_custom_app_view_event_user_id")
                            .from(CustomAppViewEvent::Table, CustomAppViewEvent::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_custom_app_view_event_app_viewed")
                    .table(CustomAppViewEvent::Table)
                    .col(CustomAppViewEvent::AppId)
                    .col((CustomAppViewEvent::ViewedAt, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_custom_app_view_event_user_viewed")
                    .table(CustomAppViewEvent::Table)
                    .col(CustomAppViewEvent::UserId)
                    .col((CustomAppViewEvent::ViewedAt, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CustomAppEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CustomAppEvent::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CustomAppEvent::AppId).uuid().not_null())
                    .col(ColumnDef::new(CustomAppEvent::UserId).uuid().not_null())
                    .col(ColumnDef::new(CustomAppEvent::UserEmail).text().not_null())
                    .col(ColumnDef::new(CustomAppEvent::SessionId).uuid().not_null())
                    .col(ColumnDef::new(CustomAppEvent::EventName).text().not_null())
                    .col(
                        ColumnDef::new(CustomAppEvent::Payload)
                            .json_binary()
                            .not_null()
                            .default(Expr::cust("'{}'::jsonb")),
                    )
                    .col(
                        ColumnDef::new(CustomAppEvent::OccurredAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_custom_app_event_app_id")
                            .from(CustomAppEvent::Table, CustomAppEvent::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_custom_app_event_user_id")
                            .from(CustomAppEvent::Table, CustomAppEvent::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_custom_app_event_app_occurred")
                    .table(CustomAppEvent::Table)
                    .col(CustomAppEvent::AppId)
                    .col((CustomAppEvent::OccurredAt, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_custom_app_event_app_event_name")
                    .table(CustomAppEvent::Table)
                    .col(CustomAppEvent::AppId)
                    .col(CustomAppEvent::EventName)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CustomAppEvent::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(CustomAppViewEvent::Table).to_owned())
            .await?;
        Ok(())
    }
}
