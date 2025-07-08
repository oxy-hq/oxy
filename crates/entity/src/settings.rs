//! `SeaORM` Entity for GitHub settings configuration

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// GitHub synchronization status
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum SyncStatus {
    #[serde(rename = "idle")]
    #[sea_orm(string_value = "idle")]
    Idle,
    #[serde(rename = "syncing")]
    #[sea_orm(string_value = "syncing")]
    Syncing,
    #[serde(rename = "synced")]
    #[sea_orm(string_value = "synced")]
    Synced,
    #[serde(rename = "error")]
    #[sea_orm(string_value = "error")]
    Error,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Encrypted GitHub Personal Access Token
    pub github_token: String,
    /// GitHub repository ID (as returned by GitHub API)
    pub selected_repo_id: Option<i64>,
    /// Current revision/commit hash of the synced repo
    pub revision: Option<String>,
    /// Sync status: idle, syncing, synced, error
    pub sync_status: SyncStatus,
    pub onboarded: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
