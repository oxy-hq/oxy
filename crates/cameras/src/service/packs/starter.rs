//! Curated starter library shipped as Rust constants.
//!
//! Each starter is a versioned `PackBody`. A workspace forks a
//! starter at create-time and tracks `(starter_key,
//! starter_version)` so Oxy can ship updates via the curator UI.
//!
//! ## Versioning discipline
//!
//! - **Patch** (`1.0.0` → `1.0.1`): typo fixes, prompt-wording
//!   tweaks. Auto-applies under the `auto_minor` pack channel if
//!   the operator hasn't locally edited that role.
//! - **Minor** (`1.0.0` → `1.1.0`): new role added, new variable.
//!   Existing prompts unchanged. Auto-applies under `auto_minor`.
//! - **Major** (`1.0.0` → `2.0.0`): output schema change OR
//!   prompt semantics meaningfully shifted. Always requires
//!   human review regardless of channel.
//!
//! Bumps live in this file. CI guards: a version can never go
//! backwards; `body` changes must come with a version bump.

use super::body::{PackBody, PpeYoloConfig, RolePrompt};
use serde_json::json;
use std::collections::BTreeMap;

/// One starter library entry — what a fresh workspace can fork.
pub struct StarterPack {
    pub key: &'static str,
    pub version: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// All published starters. Order is the display order in the
/// starter picker (first is recommended for new workspaces).
pub fn list() -> &'static [StarterPack] {
    &[
        StarterPack {
            key: "restaurant",
            version: RESTAURANT_VERSION,
            name: "Restaurant",
            description: "Kitchen / prep / dining roles. Focus on PPE, cross-contamination, table state.",
        },
        StarterPack {
            key: "retail_lp",
            version: RETAIL_LP_VERSION,
            name: "Retail loss prevention",
            description: "Register / aisle / receiving / exit roles. Focus on bag-stuffing, ticket-switching, sweethearting, unauthorized exit.",
        },
        StarterPack {
            key: "factory_safety",
            version: FACTORY_SAFETY_VERSION,
            name: "Factory safety",
            description: "Production line / loading dock / mezzanine roles. Focus on PPE, restricted zones, lockout-tagout.",
        },
    ]
}

/// Look up a starter's full body by key. Returns `None` if unknown.
/// The body is built fresh each call (not Lazy-cached) because
/// these are infrequent — used at fork time and at update-diff
/// time.
pub fn body_for(key: &str) -> Option<PackBody> {
    match key {
        "restaurant" => Some(restaurant_body()),
        "retail_lp" => Some(retail_lp_body()),
        "factory_safety" => Some(factory_safety_body()),
        _ => None,
    }
}

/// Look up a starter's currently published version by key.
pub fn version_for(key: &str) -> Option<&'static str> {
    list().iter().find(|s| s.key == key).map(|s| s.version)
}

// ── Restaurant ──────────────────────────────────────────────────────────────

// Bumped 2026-05-29 (1.0.1 → 1.0.2): added an explicit "if no person
// is clearly visible" rule to the output-schema reminder. Without
// this, the VLM kept returning `attire_compliant: false` on empty
// frames (technically true — there IS no attire) which our
// compliance-summary SQL was counting as a violation. Source-side
// fix is paired with a confidence >= 0.5 filter in
// service::compliance.
//
// Bumped 2026-06-01 (1.0.2 → 1.1.0): added optional `ppe_yolo` block
// that points workers at a food-service-tuned YOLO model for bbox
// overlays. New field, no semantic change to existing prompts —
// minor bump, auto-applies under `auto_minor`.
const RESTAURANT_VERSION: &str = "1.1.0";

fn restaurant_body() -> PackBody {
    let mut variables = BTreeMap::new();
    variables.insert("required_attire".into(), json!(["hat", "apron"]));
    variables.insert("required_hygiene".into(), json!(["glove"]));

    PackBody {
        schema_version: 1,
        output_schema: output_schema_v1(),
        variables,
        roles: vec![
            RolePrompt {
                role_key: "kitchen".into(),
                label: "Kitchen".into(),
                hint: "Hot line / cook — PPE + cross-contam".into(),
                prompt: with_schema_reminder(KITCHEN_PROMPT),
            },
            RolePrompt {
                role_key: "prep".into(),
                label: "Prep".into(),
                hint: "Back of house — handwash + board sep".into(),
                prompt: with_schema_reminder(PREP_PROMPT),
            },
            RolePrompt {
                role_key: "dining".into(),
                label: "Dining".into(),
                hint: "Front of house — table state".into(),
                prompt: with_schema_reminder(DINING_PROMPT),
            },
            RolePrompt {
                role_key: "other".into(),
                label: "Other".into(),
                hint: "Generic safety + presence".into(),
                prompt: with_schema_reminder(GENERAL_PROMPT),
            },
        ],
        // Phase A — PPE-YOLO defaults for the Restaurant pack.
        // `model_uri` is intentionally a placeholder URL string here;
        // workers running the v0 self-trained model just keep their
        // local `PPE_YOLO_MODEL` env var until we set up a proper
        // model registry. The presence of the block is the signal
        // for Phase B workers to start preferring pack over env.
        ppe_yolo: Some(PpeYoloConfig {
            model_uri: "/app/models/food_ppe-trained.pt".into(),
            model_sha256: None,
            confidence_threshold: PpeYoloConfig::DEFAULT_CONFIDENCE_THRESHOLD,
            // Match the v0 ontology we trained on (food_ppe / Roboflow):
            // hat / apron / head_covering. Add `glove` / `mask` once a
            // v1 model covers them; v0 doesn't, so listing them here
            // would silently drop nothing (filter is permissive).
            classes: Some(vec![
                "person".into(),
                "hat".into(),
                "apron".into(),
                "head_covering".into(),
            ]),
            // Dining-room cameras don't run PPE checks (no compliance
            // question there). `office` matches the v2 role taxonomy
            // even though the v1 starter doesn't ship it yet — cheap
            // forward-compat.
            suppress_for_roles: vec!["dining".into(), "office".into()],
        }),
    }
}

/// The v1 output schema. Worker parses this shape into compliance
/// reports (`attire_compliant`, `hygiene_compliant`, etc.).
fn output_schema_v1() -> serde_json::Value {
    json!({
        "type": "object",
        "required": ["attire_compliant", "hygiene_compliant", "confidence", "notes"],
        "properties": {
            "attire_compliant": { "type": "boolean" },
            "hygiene_compliant": { "type": "boolean" },
            "missing_items": { "type": "array", "items": { "type": "string" } },
            "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
            "notes": { "type": "string" }
        }
    })
}

// Output-schema reminder shared by all role prompts. The worker
// continues parsing the existing JSON shape regardless of role.
//
// The "if no person is clearly visible" rule is load-bearing: without
// it the VLM correctly observes "no compliant attire is visible" on
// an empty frame and returns `attire_compliant: false`, which our
// compliance-summary SQL then counts as a violation. The fix is to
// tell the model up front that absent-of-person is the no-violation
// path.
const OUTPUT_SCHEMA_REMINDER: &str = r#"Return ONLY valid JSON with this shape:
{
  "attire_compliant":  true | false,
  "hygiene_compliant": true | false,
  "missing_items":     ["hat", "apron", ...],
  "confidence":        0.0 .. 1.0,
  "notes":             "one short sentence with the visual evidence"
}

CRITICAL: if no person is clearly visible in the frame, return
`attire_compliant: true`, `hygiene_compliant: true`, `confidence: 0.0`,
and explain in `notes` (e.g. "no person visible"). Absence of a
worker is NOT a violation.

Otherwise be specific. If you can see a person but can't determine
attire confidently, lower `confidence` (below 0.5) and explain —
do not invent findings.
"#;

// Prompts use Jinja2 (rendered by minijinja on the backend for
// validation, by python-jinja2 on the worker at runtime). The
// `{{ variables.required_attire | join(", ") }}` pattern requires
// `required_attire` to be a list — matches the JSON shape we
// initialize variables with above.

const GENERAL_PROMPT: &str = concat!(
    "You are a site safety auditor looking at one CCTV frame.\n",
    "A computer vision pipeline has detected a worker (tracker_id={{ track_id }}) who\n",
    "has been visible in this camera view for {{ dwell_seconds | int }} seconds. Decide\n",
    "whether they appear to be following the required protocols.\n",
    "\n",
    r#"Required attire items:  {{ variables.required_attire | join(", ") if variables.required_attire else "(none)" }}"#,
    "\n",
    r#"Required hygiene items: {{ variables.required_hygiene | join(", ") if variables.required_hygiene else "(none)" }}"#,
    "\n\n",
    // OUTPUT_SCHEMA_REMINDER appended at end via a runtime concat below
);

const KITCHEN_PROMPT: &str = concat!(
    "You are a kitchen-line food-safety auditor reviewing one CCTV frame from\n",
    "the hot line / cook station. A worker (tracker_id={{ track_id }}) has been\n",
    "in view for {{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - PPE: chef's hat or hairnet, clean (no visible soiling) jacket / apron, closed-toe shoes\n",
    "  - Hands: gloves on for ready-to-eat handling; no bare-hand contact\n",
    "    with cooked food\n",
    "  - Cross-contamination: no raw protein near ready-to-eat surfaces\n",
    "  - Knife / hot-surface handling: no obvious unsafe behavior\n",
    "\n",
    r#"Required attire items:  {{ variables.required_attire | join(", ") if variables.required_attire else "(none)" }}"#,
    "\n",
    r#"Required hygiene items: {{ variables.required_hygiene | join(", ") if variables.required_hygiene else "(none)" }}"#,
    "\n\n",
);

const PREP_PROMPT: &str = concat!(
    "You are a back-of-house prep auditor reviewing one CCTV frame from the\n",
    "prep counter. A worker (tracker_id={{ track_id }}) has been in view for\n",
    "{{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - Handwashing cadence — gloves changed when switching tasks\n",
    "  - Cutting boards separated by use (raw protein vs produce vs RTE)\n",
    "  - No raw protein contacting ready-to-eat surfaces or tools\n",
    "  - PPE: hat or hairnet, apron, gloves where required\n",
    "\n",
    r#"Required attire items:  {{ variables.required_attire | join(", ") if variables.required_attire else "(none)" }}"#,
    "\n",
    r#"Required hygiene items: {{ variables.required_hygiene | join(", ") if variables.required_hygiene else "(none)" }}"#,
    "\n\n",
);

const DINING_PROMPT: &str = concat!(
    "You are a front-of-house auditor reviewing one CCTV frame from the\n",
    "dining area. A person (tracker_id={{ track_id }}) has been in view for\n",
    "{{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - Table state — cleared, set, dirty, abandoned with food\n",
    "  - Spills, broken glass, or other floor hazards\n",
    "  - Visible customer-service issues (long wait, unattended table)\n",
    "  - Staff PPE if visible (uniform / hat per house policy)\n",
    "\n",
    r#"Required attire items:  {{ variables.required_attire | join(", ") if variables.required_attire else "(none)" }}"#,
    "\n",
    r#"Required hygiene items: {{ variables.required_hygiene | join(", ") if variables.required_hygiene else "(none)" }}"#,
    "\n\n",
);

// ── Retail loss prevention ──────────────────────────────────────────────────
//
// Roles map to the high-shrink zones at a typical retail store. The
// output schema is intentionally identical to the restaurant pack
// (attire_compliant / hygiene_compliant) so the worker's parser is
// shared — see the design doc § "Output schema enforcement". A
// future v2 pack body can introduce a domain-specific schema once
// the worker grows a generic parser.

const RETAIL_LP_VERSION: &str = "1.0.0";

fn retail_lp_body() -> PackBody {
    let mut variables = BTreeMap::new();
    // Customers carrying these in the store are normal; the
    // variable lets the operator narrow the set per location.
    variables.insert(
        "expected_attire".into(),
        json!(["shirt", "shoes", "store_uniform"]),
    );
    // What the operator is watching for. "Suspicious behavior" is
    // intentionally fuzzy — the VLM is the policy interpreter, not
    // the operator.
    variables.insert(
        "watch_for".into(),
        json!([
            "bag_stuffing",
            "ticket_switch",
            "concealed_item",
            "unauthorized_exit"
        ]),
    );

    PackBody {
        schema_version: 1,
        output_schema: output_schema_v1(),
        variables,
        roles: vec![
            RolePrompt {
                role_key: "register".into(),
                label: "Register".into(),
                hint: "Point of sale — sweethearting + ticket switch".into(),
                prompt: with_schema_reminder(REGISTER_PROMPT),
            },
            RolePrompt {
                role_key: "aisle".into(),
                label: "Aisle".into(),
                hint: "Sales floor — bag-stuffing + concealment".into(),
                prompt: with_schema_reminder(AISLE_PROMPT),
            },
            RolePrompt {
                role_key: "receiving".into(),
                label: "Receiving".into(),
                hint: "Back of house — inbound + employee theft".into(),
                prompt: with_schema_reminder(RECEIVING_PROMPT),
            },
            RolePrompt {
                role_key: "exit".into(),
                label: "Exit".into(),
                hint: "Doorway — unauthorized exit + EAS".into(),
                prompt: with_schema_reminder(EXIT_PROMPT),
            },
            RolePrompt {
                role_key: "other".into(),
                label: "Other".into(),
                hint: "Generic safety + presence".into(),
                prompt: with_schema_reminder(GENERIC_LP_PROMPT),
            },
        ],
        // No PPE focus for retail-LP — bag inspection / behavior is
        // what these prompts watch for; bbox detectors aren't useful
        // until we have a "concealed item" model worth shipping.
        ppe_yolo: None,
    }
}

const REGISTER_PROMPT: &str = concat!(
    "You are a retail loss-prevention reviewer looking at one CCTV frame from\n",
    "the register / point of sale. A person (tracker_id={{ track_id }}) has been\n",
    "in view for {{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - Ticket switching: tag visibly altered, mismatched between item and SKU\n",
    "  - Sweethearting: cashier passes item without scanning, voids partway\n",
    "  - Refund fraud: high-value item refunded without a receipt match\n",
    "  - Drawer open without a transaction\n",
    "\n",
    r#"Watch for: {{ variables.watch_for | join(", ") if variables.watch_for else "(none)" }}"#,
    "\n\n",
);

const AISLE_PROMPT: &str = concat!(
    "You are a retail loss-prevention reviewer looking at one CCTV frame from\n",
    "a sales aisle. A person (tracker_id={{ track_id }}) has been in view for\n",
    "{{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - Bag-stuffing: items placed into a personal bag without payment\n",
    "  - Concealment: items hidden under clothing or inside containers\n",
    "  - Tag-removal: security tags removed at the shelf\n",
    "  - Suspicious dwell: prolonged stationary presence near high-value goods\n",
    "\n",
    r#"Watch for: {{ variables.watch_for | join(", ") if variables.watch_for else "(none)" }}"#,
    "\n\n",
);

const RECEIVING_PROMPT: &str = concat!(
    "You are a retail loss-prevention reviewer looking at one CCTV frame from\n",
    "the back-of-house receiving area. A person (tracker_id={{ track_id }}) has\n",
    "been in view for {{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - Unattended pallet: open inventory left without an employee present\n",
    "  - Employee theft: merchandise moved toward exits / personal vehicles\n",
    "  - Document mismatch: receiving items without scanning / signing\n",
    "  - Dock door open with no active delivery\n",
    "\n",
    r#"Watch for: {{ variables.watch_for | join(", ") if variables.watch_for else "(none)" }}"#,
    "\n\n",
);

const EXIT_PROMPT: &str = concat!(
    "You are a retail loss-prevention reviewer looking at one CCTV frame from\n",
    "a store exit. A person (tracker_id={{ track_id }}) has been in view for\n",
    "{{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - Unauthorized exit: visible item leaving without a receipt or bag\n",
    "  - EAS alarm: person walking through detection pedestals (visible tags)\n",
    "  - Rushed exit: running or unusually fast pace away from registers\n",
    "  - Bag check refusal: declining a flagged stop\n",
    "\n",
    r#"Watch for: {{ variables.watch_for | join(", ") if variables.watch_for else "(none)" }}"#,
    "\n\n",
);

const GENERIC_LP_PROMPT: &str = concat!(
    "You are a retail loss-prevention reviewer looking at one CCTV frame.\n",
    "A person (tracker_id={{ track_id }}) has been in view for\n",
    "{{ dwell_seconds | int }} seconds. Decide whether they appear to be\n",
    "behaving normally for a customer or staff member.\n",
    "\n",
    r#"Watch for: {{ variables.watch_for | join(", ") if variables.watch_for else "(none)" }}"#,
    "\n\n",
);

// ── Factory safety ──────────────────────────────────────────────────────────

const FACTORY_SAFETY_VERSION: &str = "1.0.0";

fn factory_safety_body() -> PackBody {
    let mut variables = BTreeMap::new();
    variables.insert(
        "required_ppe".into(),
        json!([
            "hard_hat",
            "safety_vest",
            "safety_glasses",
            "steel_toe_boots"
        ]),
    );
    variables.insert(
        "restricted_actions".into(),
        json!([
            "phone_use_on_floor",
            "running",
            "bypassing_guard",
            "entering_restricted_zone"
        ]),
    );

    PackBody {
        schema_version: 1,
        output_schema: output_schema_v1(),
        variables,
        roles: vec![
            RolePrompt {
                role_key: "production_line".into(),
                label: "Production line".into(),
                hint: "Active machinery — PPE + restricted reach".into(),
                prompt: with_schema_reminder(PRODUCTION_LINE_PROMPT),
            },
            RolePrompt {
                role_key: "loading_dock".into(),
                label: "Loading dock".into(),
                hint: "Forklift zone — pedestrian/vehicle separation".into(),
                prompt: with_schema_reminder(LOADING_DOCK_PROMPT),
            },
            RolePrompt {
                role_key: "mezzanine".into(),
                label: "Mezzanine".into(),
                hint: "Elevated walkway — fall protection".into(),
                prompt: with_schema_reminder(MEZZANINE_PROMPT),
            },
            RolePrompt {
                role_key: "other".into(),
                label: "Other".into(),
                hint: "Generic PPE + safe-behavior audit".into(),
                prompt: with_schema_reminder(GENERIC_FACTORY_PROMPT),
            },
        ],
        // Factory PPE shares the construction-PPE ontology (hardhat,
        // vest, mask). Punt on shipping a baked-in URI until we have
        // a vetted construction-safety model — the existing
        // HuggingFace yolov8n-construction-safety is plumbing-test
        // grade, not production. Operators who want the overlay can
        // still flip on `PPE_YOLO_MODEL` via env in Phase A.
        ppe_yolo: None,
    }
}

const PRODUCTION_LINE_PROMPT: &str = concat!(
    "You are a factory safety auditor reviewing one CCTV frame from a\n",
    "production line / machine cell. A worker (tracker_id={{ track_id }}) has\n",
    "been in view for {{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - PPE: required items worn correctly (not just present)\n",
    "  - Machine guards: physical barriers in place and not bypassed\n",
    "  - Hands inside guard zone while machine is running\n",
    "  - Lockout/tagout: any maintenance must have a visible LOTO tag\n",
    "\n",
    r#"Required PPE: {{ variables.required_ppe | join(", ") if variables.required_ppe else "(none)" }}"#,
    "\n",
    r#"Restricted actions: {{ variables.restricted_actions | join(", ") if variables.restricted_actions else "(none)" }}"#,
    "\n\n",
);

const LOADING_DOCK_PROMPT: &str = concat!(
    "You are a factory safety auditor reviewing one CCTV frame from a\n",
    "loading dock. A person (tracker_id={{ track_id }}) has been in view for\n",
    "{{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - PPE: high-visibility vest + hard hat in a forklift area\n",
    "  - Pedestrian/forklift separation: people in marked walkways only\n",
    "  - Dock edge: no one within 1m of an open dock door without protection\n",
    "  - Trailer chock: trailer wheels chocked before loading/unloading\n",
    "\n",
    r#"Required PPE: {{ variables.required_ppe | join(", ") if variables.required_ppe else "(none)" }}"#,
    "\n\n",
);

const MEZZANINE_PROMPT: &str = concat!(
    "You are a factory safety auditor reviewing one CCTV frame from an\n",
    "elevated walkway / mezzanine. A worker (tracker_id={{ track_id }}) has\n",
    "been in view for {{ dwell_seconds | int }} seconds.\n",
    "\n",
    "Check ALL of:\n",
    "  - Fall protection: harness clipped to a rated anchor when within 2m of edge\n",
    "  - Guardrails: top rail + mid rail + toe board intact\n",
    "  - Stair use: handrail held on travel, no items carried two-handed\n",
    "  - PPE: hard hat (falling-object risk to floor below)\n",
    "\n",
    r#"Required PPE: {{ variables.required_ppe | join(", ") if variables.required_ppe else "(none)" }}"#,
    "\n\n",
);

const GENERIC_FACTORY_PROMPT: &str = concat!(
    "You are a factory safety auditor reviewing one CCTV frame.\n",
    "A worker (tracker_id={{ track_id }}) has been in view for\n",
    "{{ dwell_seconds | int }} seconds. Decide whether they appear to be\n",
    "following site safety protocols.\n",
    "\n",
    r#"Required PPE: {{ variables.required_ppe | join(", ") if variables.required_ppe else "(none)" }}"#,
    "\n",
    r#"Restricted actions: {{ variables.restricted_actions | join(", ") if variables.restricted_actions else "(none)" }}"#,
    "\n\n",
);

/// Append the shared output-schema reminder to a role prompt body.
/// Called from [`restaurant_body`] when building each `RolePrompt`.
fn with_schema_reminder(prompt: &str) -> String {
    let mut s = String::with_capacity(prompt.len() + OUTPUT_SCHEMA_REMINDER.len());
    s.push_str(prompt);
    s.push_str(OUTPUT_SCHEMA_REMINDER);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restaurant_starter_has_four_roles() {
        let body = body_for("restaurant").unwrap();
        assert_eq!(body.roles.len(), 4);
        let keys: Vec<_> = body.roles.iter().map(|r| r.role_key.as_str()).collect();
        assert_eq!(keys, vec!["kitchen", "prep", "dining", "other"]);
    }

    #[test]
    fn retail_lp_starter_shape() {
        let body = body_for("retail_lp").unwrap();
        let keys: Vec<_> = body.roles.iter().map(|r| r.role_key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["register", "aisle", "receiving", "exit", "other"]
        );
        assert!(body.variables.contains_key("watch_for"));
    }

    #[test]
    fn factory_safety_starter_shape() {
        let body = body_for("factory_safety").unwrap();
        let keys: Vec<_> = body.roles.iter().map(|r| r.role_key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["production_line", "loading_dock", "mezzanine", "other"]
        );
        assert!(body.variables.contains_key("required_ppe"));
    }

    #[test]
    fn all_starters_listed_have_bodies() {
        for s in list() {
            assert!(
                body_for(s.key).is_some(),
                "starter `{}` listed but body_for returned None",
                s.key
            );
            assert_eq!(version_for(s.key), Some(s.version));
        }
    }

    #[test]
    fn restaurant_starter_variables() {
        let body = body_for("restaurant").unwrap();
        assert!(body.variables.contains_key("required_attire"));
        assert!(body.variables.contains_key("required_hygiene"));
    }

    #[test]
    fn prompt_for_falls_back_to_other_on_unknown_role() {
        let body = body_for("restaurant").unwrap();
        let r = body.prompt_for(Some("nonsense")).unwrap();
        assert_eq!(r.role_key, "other");
    }

    #[test]
    fn prompt_for_handles_none_role() {
        let body = body_for("restaurant").unwrap();
        let r = body.prompt_for(None).unwrap();
        assert_eq!(r.role_key, "other");
    }

    #[test]
    fn unknown_starter_returns_none() {
        assert!(body_for("does_not_exist").is_none());
    }

    #[test]
    fn version_for_matches_published_version() {
        // Tracks `RESTAURANT_VERSION` — bump in lockstep with the constant
        // rather than pinning a literal, so the next prompt-tweak doesn't
        // require touching the test.
        assert_eq!(version_for("restaurant"), Some(RESTAURANT_VERSION));
        assert_eq!(version_for("unknown"), None);
    }
}
