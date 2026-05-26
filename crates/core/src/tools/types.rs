use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SQLInput {
    pub name: Option<String>,
    pub database: String,
    pub sql: String,
    pub dry_run_limit: Option<u64>,
    pub persist: bool,
}
