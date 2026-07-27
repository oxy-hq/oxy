use sea_orm_migration::prelude::*;

/// The partner distribution platform, as **one** migration.
///
/// This squashes nine branch migrations that built-then-demolished a lot of
/// schema — the original partner tier (`partners` / `partner_members` /
/// `partner_member_orgs`) was entirely replaced by the permission model, and an
/// abandoned "act as a user" column pair was added and dropped. Applying that
/// sequence against a fresh database meant creating five tables only to drop
/// them again. Since the partner tier never shipped, no database anywhere depends
/// on the intermediate states, so the honest history is the final schema in one
/// step.
///
/// What it produces, net, against `main`'s schema:
///   * DROP `workspace_oxy_access` (opt-in consent) → CREATE
///     `workspace_oxy_lockdown` (opt-out lockdown). The two mean opposite things,
///     so the old rows are discarded, not reinterpreted.
///   * CREATE `audit_events` (append-only, hash-chained, `seq BIGSERIAL` for
///     skew-independent ordering).
///   * CREATE `admin_assume_sessions` (explicit, bounded, audited staff
///     impersonation).
///   * CREATE the permission model: `partner_grants`, `partner_capabilities`
///     (the ceiling), `partner_orgs`, `partner_role_bindings` (partner access —
///     one row = one operator). A partner IS an org that holds a grant.
///
/// Applies cleanly on a fresh database AND reconciles a dev/staging database that
/// applied the PRE-SQUASH branch migrations. Those used different NAMES
/// (`create_partner_tier`, `create_audit_events`, `permission_model`, …), and
/// Sea-ORM keys migrations by name — so this newly-named migration DOES run there.
/// Its `if_not_exists` creates alone would silently skip tables an old migration
/// left behind with a narrower column set (e.g. `partner_capabilities` before
/// `develop_apps`, or `audit_events` before `seq`), which the new code then 500s
/// on. So `up()` first DROPs the partner/audit/assume tables; the partner tier
/// never shipped, so there is no production data at risk. Design:
/// `internal-docs/partner-platform.md`.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── Reconcile a pre-squash env (see the type doc) ────────────────────
        //
        // Drop the partner/audit/assume tables first so a stale, narrower schema
        // left by the old (differently-named) branch migrations can't survive the
        // `if_not_exists` creates below. Only runs on the FIRST application of THIS
        // migration name — Sea-ORM never re-runs it — so on a fresh DB these are
        // no-ops, and on an env that already ran THIS migration nothing re-drops.
        // CASCADE covers FKs; children are listed before parents anyway.
        for t in [
            // Reused-name tables — drop so the creates below define the schema.
            "partner_role_bindings",
            "partner_orgs",
            "partner_capabilities",
            "partner_grants",
            "admin_assume_sessions",
            "audit_events",
            // Orphans from the original partner tier, absent from the new model.
            "partner_member_orgs",
            "partner_members",
            "partners",
        ] {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TABLE IF EXISTS {t} CASCADE"))
                .await?;
        }

        // ── Oxy-staff access: opt-in consent → opt-out lockdown ──────────────
        //
        // `workspace_oxy_access` (a row = "GRANTED staff access") was
        // self-grantable by the party it gated: `resolve_effective_role`
        // synthesizes Owner for any Global Admin, and the grant was Owner-gated.
        // Replaced by `workspace_oxy_lockdown` (a row = "LOCKED staff OUT"; the
        // default of no row allows support out of the box), which only a REAL org
        // officer can set.
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceOxyLockdown::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkspaceOxyLockdown::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    // One lockdown row per workspace.
                    .col(
                        ColumnDef::new(WorkspaceOxyLockdown::WorkspaceId)
                            .uuid()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(WorkspaceOxyLockdown::LockedBy).uuid().null())
                    .col(
                        ColumnDef::new(WorkspaceOxyLockdown::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // The old consent rows mean the OPPOSITE of the new ones, so carrying them
        // over would lock out exactly the customers who had opted in. Discard.
        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceOxyAccess::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        // ── audit_events: one append-only stream for privileged actions ──────
        manager
            .create_table(
                Table::create()
                    .table(AuditEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuditEvents::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AuditEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(AuditEvents::ActorUserId).uuid())
                    .col(ColumnDef::new(AuditEvents::ActorEmail).text().not_null())
                    .col(
                        ColumnDef::new(AuditEvents::ActorType)
                            .text()
                            .not_null()
                            .default("user"),
                    )
                    .col(ColumnDef::new(AuditEvents::Action).text().not_null())
                    .col(ColumnDef::new(AuditEvents::OrgId).uuid())
                    .col(ColumnDef::new(AuditEvents::WorkspaceId).uuid())
                    .col(ColumnDef::new(AuditEvents::PartnerId).uuid())
                    .col(ColumnDef::new(AuditEvents::TargetType).text())
                    .col(ColumnDef::new(AuditEvents::TargetId).text())
                    .col(ColumnDef::new(AuditEvents::TargetLabel).text())
                    .col(ColumnDef::new(AuditEvents::Before).json_binary())
                    .col(ColumnDef::new(AuditEvents::After).json_binary())
                    .col(ColumnDef::new(AuditEvents::Ip).text())
                    .col(ColumnDef::new(AuditEvents::UserAgent).text())
                    .col(ColumnDef::new(AuditEvents::RequestId).text())
                    .col(
                        ColumnDef::new(AuditEvents::Outcome)
                            .text()
                            .not_null()
                            .default("success"),
                    )
                    .col(ColumnDef::new(AuditEvents::Reason).text())
                    .col(
                        ColumnDef::new(AuditEvents::Metadata)
                            .json_binary()
                            .not_null()
                            .default(Expr::cust("'{}'::jsonb")),
                    )
                    .col(ColumnDef::new(AuditEvents::PrevHash).text())
                    .col(ColumnDef::new(AuditEvents::Hash).text())
                    .to_owned(),
            )
            .await?;

        for (name, col) in [
            ("idx_audit_events_org_created", AuditEvents::OrgId),
            ("idx_audit_events_partner_created", AuditEvents::PartnerId),
            ("idx_audit_events_actor_created", AuditEvents::ActorUserId),
        ] {
            manager
                .create_index(
                    Index::create()
                        .if_not_exists()
                        .name(name)
                        .table(AuditEvents::Table)
                        .col(col)
                        .col((AuditEvents::CreatedAt, IndexOrder::Desc))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_audit_events_action")
                    .table(AuditEvents::Table)
                    .col(AuditEvents::Action)
                    .to_owned(),
            )
            .await?;

        // Monotonic insert sequence for skew-independent chain ordering:
        // `created_at` is app-generated (per-instance `now()`), so cross-instance
        // clock skew could mislead a verifier that re-sorts by time. `BIGSERIAL`
        // is DB-assigned and strictly increasing, so the per-org chain follows it.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS seq BIGSERIAL")
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_audit_events_org_seq \
                 ON audit_events (org_id, seq DESC)",
            )
            .await?;

        // ── admin_assume_sessions: explicit, bounded, audited impersonation ──
        manager
            .create_table(
                Table::create()
                    .table(AdminAssumeSessions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AdminAssumeSessions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    // The REAL staff user. Never the impersonated identity — the
                    // audit trail must always name who actually acted.
                    .col(
                        ColumnDef::new(AdminAssumeSessions::ActorUserId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AdminAssumeSessions::ActorEmail)
                            .string()
                            .not_null(),
                    )
                    // Scope: exactly one org. No blanket "assume everywhere".
                    .col(ColumnDef::new(AdminAssumeSessions::OrgId).uuid().not_null())
                    // Required — an unexplained impersonation is a red flag.
                    .col(
                        ColumnDef::new(AdminAssumeSessions::Reason)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AdminAssumeSessions::StartedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    // Hard bound. An expired row grants nothing.
                    .col(
                        ColumnDef::new(AdminAssumeSessions::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    // Set when explicitly ended; NULL while live.
                    .col(
                        ColumnDef::new(AdminAssumeSessions::EndedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Hot path: "is there a live session for (actor, org)?" — hit on every
        // org/workspace request by a non-member operator.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_admin_assume_actor_org")
                    .table(AdminAssumeSessions::Table)
                    .col(AdminAssumeSessions::ActorUserId)
                    .col(AdminAssumeSessions::OrgId)
                    .to_owned(),
            )
            .await?;

        // ── the permission model: a partner IS an org that holds a grant ─────
        manager
            .create_table(
                Table::create()
                    .table(PartnerGrants::Table)
                    .if_not_exists()
                    // The partner IS this org. Name/slug come from it.
                    .col(
                        ColumnDef::new(PartnerGrants::OrgId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(PartnerGrants::Status)
                            .string()
                            .not_null()
                            .default("active"),
                    )
                    .col(ColumnDef::new(PartnerGrants::CreatedBy).uuid().null())
                    .col(
                        ColumnDef::new(PartnerGrants::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_partner_grants_org")
                            .from(PartnerGrants::Table, PartnerGrants::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // The CEILING: what Oxy permits this partner AT ALL.
        manager
            .create_table(
                Table::create()
                    .table(PartnerCapabilities::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PartnerCapabilities::OrgId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(bool_col(PartnerCapabilities::ManageMembers, true))
                    .col(bool_col(PartnerCapabilities::ManageApps, true))
                    // The DATA PLANE. Off by default — publishing an app is not the
                    // same as reading the customer's warehouse.
                    .col(bool_col(PartnerCapabilities::DevelopApps, false))
                    .col(bool_col(PartnerCapabilities::ViewAudit, true))
                    .col(bool_col(PartnerCapabilities::ManageBilling, false))
                    .col(bool_col(PartnerCapabilities::ManageSecrets, false))
                    // Onboard client orgs. Sensitive — it mints billable tenants.
                    .col(bool_col(PartnerCapabilities::CreateOrgs, false))
                    .col(bool_col(PartnerCapabilities::ManageOrgSettings, false))
                    .col(
                        ColumnDef::new(PartnerCapabilities::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_partner_caps_org")
                            .from(PartnerCapabilities::Table, PartnerCapabilities::OrgId)
                            .to(PartnerGrants::Table, PartnerGrants::OrgId)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Which clients this partner manages.
        manager
            .create_table(
                Table::create()
                    .table(PartnerOrgs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PartnerOrgs::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PartnerOrgs::PartnerOrgId).uuid().not_null())
                    // UNIQUE: one partner per client. A client is never managed by two.
                    .col(
                        ColumnDef::new(PartnerOrgs::ManagedOrgId)
                            .uuid()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(PartnerOrgs::CreatedBy).uuid().null())
                    .col(
                        ColumnDef::new(PartnerOrgs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_partner_orgs_partner")
                            .from(PartnerOrgs::Table, PartnerOrgs::PartnerOrgId)
                            .to(PartnerGrants::Table, PartnerGrants::OrgId)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_partner_orgs_managed")
                            .from(PartnerOrgs::Table, PartnerOrgs::ManagedOrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Partner access. A row means this member of the partner org is a partner
        // OPERATOR — they act on the partner's clients, bounded by the ceiling. No
        // role, no per-client scope: one partnership, everyone on it reaches every
        // client. A partner-org member WITHOUT a row is just an employee. No email
        // keying, no orphan user_id — a partner's people are just org members.
        manager
            .create_table(
                Table::create()
                    .table(PartnerRoleBindings::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PartnerRoleBindings::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(PartnerRoleBindings::OrgMemberId)
                            .uuid()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(PartnerRoleBindings::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_partner_binding_member")
                            .from(PartnerRoleBindings::Table, PartnerRoleBindings::OrgMemberId)
                            .to(OrgMembers::Table, OrgMembers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Reverse order: children before parents (FK cascade also covers it, but
        // be explicit), then restore the pre-branch consent table.
        for t in [
            "partner_role_bindings",
            "partner_orgs",
            "partner_capabilities",
            "partner_grants",
            "admin_assume_sessions",
            "audit_events",
            "workspace_oxy_lockdown",
        ] {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TABLE IF EXISTS {t} CASCADE"))
                .await?;
        }

        // Recreate the old consent table (empty — the grants are unrecoverable,
        // which is correct: on rollback nobody is opted in).
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceOxyAccess::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkspaceOxyAccess::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceOxyAccess::WorkspaceId)
                            .uuid()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(WorkspaceOxyAccess::GrantedBy).uuid().null())
                    .col(
                        ColumnDef::new(WorkspaceOxyAccess::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }
}

fn bool_col<T: IntoIden + 'static>(name: T, default: bool) -> ColumnDef {
    ColumnDef::new(name)
        .boolean()
        .not_null()
        .default(default)
        .to_owned()
}

#[derive(DeriveIden)]
enum WorkspaceOxyLockdown {
    Table,
    Id,
    WorkspaceId,
    LockedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum WorkspaceOxyAccess {
    Table,
    Id,
    WorkspaceId,
    GrantedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AuditEvents {
    Table,
    Id,
    CreatedAt,
    ActorUserId,
    ActorEmail,
    ActorType,
    Action,
    OrgId,
    WorkspaceId,
    PartnerId,
    TargetType,
    TargetId,
    TargetLabel,
    Before,
    After,
    Ip,
    UserAgent,
    RequestId,
    Outcome,
    Reason,
    Metadata,
    PrevHash,
    Hash,
}

#[derive(DeriveIden)]
enum AdminAssumeSessions {
    Table,
    Id,
    ActorUserId,
    ActorEmail,
    OrgId,
    Reason,
    StartedAt,
    ExpiresAt,
    EndedAt,
}

#[derive(DeriveIden)]
enum PartnerGrants {
    Table,
    OrgId,
    Status,
    CreatedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum PartnerCapabilities {
    Table,
    OrgId,
    ManageMembers,
    ManageApps,
    DevelopApps,
    ViewAudit,
    ManageBilling,
    ManageSecrets,
    CreateOrgs,
    ManageOrgSettings,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum PartnerOrgs {
    Table,
    Id,
    PartnerOrgId,
    ManagedOrgId,
    CreatedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum PartnerRoleBindings {
    Table,
    Id,
    OrgMemberId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum OrgMembers {
    Table,
    Id,
}
