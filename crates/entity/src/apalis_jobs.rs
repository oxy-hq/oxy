//! `SeaORM` Entity for Apalis job queue

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Job status for Apalis jobs
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum JobStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "running")]
    Running,
    #[sea_orm(string_value = "done")]
    Done,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "killed")]
    Killed,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "apalis_jobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub job_type: String,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub job: Json,
    pub run_at: DateTimeWithTimeZone,
    pub done_at: Option<DateTimeWithTimeZone>,
    pub lock_at: Option<DateTimeWithTimeZone>,
    pub lock_by: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
