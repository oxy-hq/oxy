//! Typed view of a domain pack's JSON body.
//!
//! Persisted as opaque JSONB in `camera_domain_packs.body`; the
//! service layer deserializes into [`PackBody`] on read and
//! serializes back on write. Keeping the typed view in code (vs
//! letting the DB schema enforce shape) buys us cheap forward-
//! compatible field additions: a new optional field rolls out
//! without a migration.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The whole pack as it lives in JSONB.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackBody {
    /// Bumped when the body shape (not the contents) changes. v1 is
    /// the shape this file documents; future shapes deserialize
    /// against a different struct.
    pub schema_version: u32,

    /// JSON schema the worker tells the VLM to return. v1 just
    /// stores this for documentation; the worker continues parsing
    /// the existing `attire_compliant / hygiene_compliant / ...`
    /// shape. Plumbed through for forward-compat with multi-domain
    /// schemas.
    pub output_schema: serde_json::Value,

    /// Ordered list of roles. The order is the UI order in the
    /// role picker; first-with-`role_key=="other"` (or first row)
    /// is the fallback prompt.
    pub roles: Vec<RolePrompt>,

    /// Pack-wide variables every prompt template can reference
    /// via `{{ variables.* }}`. Concrete keys depend on the pack;
    /// the restaurant pack uses `required_attire` and
    /// `required_hygiene`. Stored as a BTreeMap so JSON output is
    /// deterministic — easier to diff for the curator UI.
    pub variables: BTreeMap<String, serde_json::Value>,

    /// P3 (Option C) — PPE bounding-box detector configuration.
    ///
    /// When `Some`, the edge worker downloads the referenced model,
    /// runs it on the same frame it sends to the VLM, and ships the
    /// detections alongside the compliance report. When `None` (the
    /// pre-Phase-A default), the worker falls back to the
    /// `PPE_YOLO_MODEL` env var; if both are unset, PPE inference
    /// stays disabled and `compliance.detections_json` is null.
    ///
    /// `#[serde(default)]` keeps packs created before this field
    /// landed deserializing cleanly (the JSONB on disk simply has
    /// no `ppe_yolo` key). `skip_serializing_if` mirrors the same
    /// shape going OUT so the export DTO + curator diff don't
    /// gain noisy `"ppe_yolo": null` rows on every legacy pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ppe_yolo: Option<PpeYoloConfig>,
}

/// PPE-YOLO configuration carried by the active domain pack. Lives
/// inside [`PackBody`] so swapping a workspace's pack
/// (restaurant → factory) atomically swaps the model on every box
/// without a per-box env-var dance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PpeYoloConfig {
    /// Where the worker should fetch the model from. Three shapes
    /// the worker accepts:
    ///   - `https://…/weights.pt`  — direct download (CDN, S3, etc.)
    ///   - `hf://owner/repo`       — HuggingFace hub id (Ultralytics resolves)
    ///   - `/app/models/file.pt`   — in-image path (already mounted)
    ///
    /// Worker caches by SHA-256 of the downloaded bytes; same URI
    /// on a subsequent poll is a no-op.
    pub model_uri: String,

    /// Optional integrity check. When provided, the worker verifies
    /// the downloaded blob's hash before swapping. Mismatches fail
    /// the swap and keep the previously-loaded model active —
    /// belt-and-braces against partial downloads / CDN hijack /
    /// pack edit + URL still pointing at the old artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_sha256: Option<String>,

    /// Min confidence to ship in the envelope. Detections below
    /// this floor are dropped server-side (the UI doesn't see them).
    /// `0.25` is a permissive default — the overlay color-codes
    /// low-confidence in amber so reviewers can judge for themselves.
    pub confidence_threshold: f32,

    /// Classes the worker SHIPS in the envelope. When `Some`,
    /// detections of any other class are filtered out — useful
    /// when a general-purpose model emits 80 COCO classes but only
    /// 5 PPE classes are interesting. When `None`, every class the
    /// model emits ships.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classes: Option<Vec<String>>,

    /// Camera roles for which PPE inference should be skipped
    /// entirely. Saves bandwidth (no envelope shipped) and avoids
    /// reviewer confusion ("why are there hat bboxes on the dining-
    /// room camera?"). Matched against `cameras.role` verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppress_for_roles: Vec<String>,
}

impl PpeYoloConfig {
    /// Default confidence the schema documents. Kept as a `pub`
    /// constant so the curator UI can pre-fill the form and the
    /// validator can reference the "sane" range without duplicating
    /// the literal.
    pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.25;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RolePrompt {
    /// Stable identifier within the pack. `cameras.role` stores
    /// this verbatim today (v1); v2 will namespace as
    /// `"{pack_key}:{role_key}"`.
    pub role_key: String,
    /// Display label for the role picker.
    pub label: String,
    /// One-line hint under the label in the picker.
    pub hint: String,
    /// Jinja2 template. Renders with the context
    /// `{ track_id, dwell_seconds, variables: { ... } }`.
    /// Render validation runs at PUT time using
    /// [`render_with_dummy_context`](super::validate::render_with_dummy_context).
    pub prompt: String,
}

impl PackBody {
    /// Find the prompt for a `role_key`, with fallback to the first
    /// `role_key == "other"` (or the first role) — never raises.
    pub fn prompt_for(&self, role_key: Option<&str>) -> Option<&RolePrompt> {
        if let Some(key) = role_key
            && let Some(matched) = self.roles.iter().find(|r| r.role_key == key)
        {
            return Some(matched);
        }
        self.roles
            .iter()
            .find(|r| r.role_key == "other")
            .or_else(|| self.roles.first())
    }
}
