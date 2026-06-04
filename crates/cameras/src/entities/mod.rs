//! SeaORM entities for the camera fleet aggregate.
//!
//! Aggregate root: `Site`. Children (within-aggregate hard FK + cascade):
//! `EdgeBox` and `Camera`. Grandchild: `EdgeBoxToken` (FK to `EdgeBox`).
//!
//! Cross-aggregate ref: `sites.workspace_id` is a loose `Uuid` column —
//! no FK constraint, per `domain-boundaries.md` P3.

pub mod audit_events;
pub mod cameras;
pub mod compliance_arbitrations;
pub mod device_claims;
pub mod device_registry;
pub mod domain_packs;
pub mod edge_box_tokens;
pub mod edge_boxes;
pub mod rollout_plans;
pub mod sites;
pub mod unifi_credentials;

// Re-export the conventional `Entity` / `Model` aliases each domain crate
// exposes so callers can `use oxy_cameras::entities::sites` and get the full
// SeaORM types in one import.
pub use audit_events::Entity as AuditEvents;
pub use cameras::Entity as Cameras;
pub use compliance_arbitrations::Entity as ComplianceArbitrations;
pub use device_claims::Entity as DeviceClaims;
pub use device_registry::Entity as DeviceRegistry;
pub use domain_packs::Entity as DomainPacks;
pub use edge_box_tokens::Entity as EdgeBoxTokens;
pub use edge_boxes::Entity as EdgeBoxes;
pub use rollout_plans::Entity as RolloutPlans;
pub use sites::Entity as Sites;
pub use unifi_credentials::Entity as UnifiCredentials;
