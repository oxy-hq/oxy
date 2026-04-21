//! `ThreadOwnerLookup` impl backed by the platform `threads` table.
//!
//! Lives in the host application so the agentic stack stays free of the
//! `entity` crate.

use agentic_pipeline::platform::ThreadOwnerLookup;
use async_trait::async_trait;
use sea_orm::{DatabaseConnection, EntityTrait};

pub struct OxyThreadOwnerLookup {
    db: DatabaseConnection,
}

impl OxyThreadOwnerLookup {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ThreadOwnerLookup for OxyThreadOwnerLookup {
    async fn thread_owner(
        &self,
        thread_id: uuid::Uuid,
    ) -> Result<Option<Option<uuid::Uuid>>, String> {
        entity::threads::Entity::find_by_id(thread_id)
            .one(&self.db)
            .await
            .map(|opt| opt.map(|t| t.user_id))
            .map_err(|e| e.to_string())
    }
}
