//! VLM ↔ YOLO agreement signal — Phase 1 of "utilizing disagreements."
//!
//! Computed at compliance-report ingest from the two pipelines'
//! existing outputs:
//!
//!   - VLM `structured_json.missing_items: Vec<String>` — classes the
//!     VLM flagged as missing on the worker in frame.
//!   - YOLO `detections_json.detections: Vec<{class, confidence,
//!     bbox}>` — classes the bbox detector saw in the same frame.
//!
//! Goal: drive a filterable column on the compliance_reports row so
//! the operator UI can surface a "review queue" without scanning
//! every event. Class detail is preserved in a JSON column for the
//! arbitration UI.
//!
//! ## Canonicalization
//!
//! VLM prompts and YOLO models talk about the same physical things
//! using different names — `hat` vs `head_covering`, `hairnet` vs
//! `head_covering`. We normalize via a small alias map to compare
//! apples-to-apples. The mapping is intentionally restaurant-PPE
//! biased for v0; growing other domains adds entries here.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Confidence floor below which a YOLO detection doesn't count as
/// "present" in the agreement math. The compliance pipeline already
/// applies a 0.5 floor in `service::compliance::VIOLATION_FILTER`;
/// this matches it so the per-row signal and the dashboard agree.
const YOLO_PRESENCE_THRESHOLD: f64 = 0.5;

/// Coarse agreement signal stamped on the compliance report row. The
/// UI filters on this for the review queue; per-class detail lives
/// in `agreement_detail_json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgreementStatus {
    /// VLM and YOLO concur on every flagged + detected class.
    Agree,
    /// At least one class differs. Drives the review queue.
    Disagree,
    /// One or both sides didn't produce comparable data (no YOLO
    /// envelope, no `missing_items` field, low VLM confidence). The
    /// review UI skips these — comparing apples to nothing isn't
    /// information.
    Inconclusive,
}

impl AgreementStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgreementStatus::Agree => "agree",
            AgreementStatus::Disagree => "disagree",
            AgreementStatus::Inconclusive => "inconclusive",
        }
    }
}

/// Per-class outcome from the agreement comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassAgreement {
    /// Both pipelines agree on this class's presence/absence.
    Agree,
    /// VLM flagged the class as missing, but YOLO saw it (above
    /// the presence threshold). Either the VLM is over-conservative
    /// or YOLO mis-detected — the operator decides via arbitration.
    VlmOnlyMissing,
    /// YOLO didn't see the class, but VLM didn't list it as missing
    /// either (and the class was in the model's relevant set). VLM
    /// might be hallucinating presence, or YOLO might be blind from
    /// the angle.
    YoloOnlyMissing,
}

/// Full per-class result — what lands in `agreement_detail_json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgreementDetail {
    /// Per-class outcome keyed by canonical class name.
    pub per_class: HashMap<String, ClassAgreement>,
}

/// Compute the agreement signal for one compliance report. Takes the
/// raw JSON values that landed on the ingest payload; never panics —
/// missing fields just push toward `Inconclusive`.
pub fn compute(
    structured_json: &serde_json::Value,
    detections_json: Option<&serde_json::Value>,
) -> (AgreementStatus, AgreementDetail) {
    let (Some(vlm_missing), Some(yolo_envelope)) = (
        extract_missing_items(structured_json),
        detections_json.and_then(extract_yolo_classes),
    ) else {
        return (AgreementStatus::Inconclusive, AgreementDetail::default());
    };

    // Both vocabularies normalized to canonical names. The union is
    // the set of classes worth comparing — a class neither side
    // mentioned is not interesting (the prompt's "relevant set"
    // determines what matters, and either pipeline would have flagged
    // it if it were).
    let canonical_vlm: HashSet<String> =
        vlm_missing.iter().map(|s| canonicalize_class(s)).collect();
    let canonical_yolo: HashSet<String> = yolo_envelope
        .iter()
        .map(|s| canonicalize_class(s))
        .collect();

    let mut detail = AgreementDetail::default();
    for class in canonical_vlm.union(&canonical_yolo) {
        let vlm_says_missing = canonical_vlm.contains(class);
        let yolo_sees_present = canonical_yolo.contains(class);
        let outcome = match (vlm_says_missing, yolo_sees_present) {
            // VLM and YOLO both say "present" (VLM didn't flag, YOLO saw).
            (false, true) => ClassAgreement::Agree,
            // VLM and YOLO both say "missing" (VLM flagged, YOLO didn't see).
            (true, false) => ClassAgreement::Agree,
            // VLM says missing, YOLO sees it.
            (true, true) => ClassAgreement::VlmOnlyMissing,
            // Neither side mentioned it — shouldn't reach here given the
            // union iteration, but kept as a defensive default.
            (false, false) => ClassAgreement::Agree,
        };
        detail.per_class.insert(class.clone(), outcome);
    }

    // Also flag the YOLO-detected classes that the VLM SHOULD have
    // listed but didn't. Today the VLM only emits a `missing_items`
    // array, so we infer "VLM thinks this is present" from absence.
    // The asymmetry is intentional: `yolo_only_missing` would require
    // knowing the VLM's "relevant class set," which lives in the
    // pack prompt and we don't parse here. Phase 3 of the disagreement
    // arc will surface that — for now we lean on `missing_items`
    // alone.

    let any_disagree = detail
        .per_class
        .values()
        .any(|c| !matches!(c, ClassAgreement::Agree));
    let status = if any_disagree {
        AgreementStatus::Disagree
    } else {
        AgreementStatus::Agree
    };
    (status, detail)
}

/// Pull `missing_items` out of the VLM's `structured_json`. Returns
/// `None` when the field is absent, the wrong shape, or the VLM
/// reported low confidence (< 0.5 — matches the violation filter).
fn extract_missing_items(structured: &serde_json::Value) -> Option<Vec<String>> {
    // Honor the VLM's own confidence floor. A `confidence: 0.2` row
    // with `missing_items: ["hat"]` means "I don't really know" —
    // not useful for the agreement comparison.
    let confidence = structured
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if confidence < 0.5 {
        return None;
    }
    let items = structured.get("missing_items")?.as_array()?;
    let names: Vec<String> = items
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    Some(names)
}

/// Pull the set of class names YOLO saw above the presence floor.
/// Returns `None` when the envelope is missing or has no detections
/// array — distinguishes "ran, saw nothing" (Some(empty Vec)) from
/// "edge worker doesn't do YOLO yet" (None).
fn extract_yolo_classes(envelope: &serde_json::Value) -> Option<Vec<String>> {
    let detections = envelope.get("detections")?.as_array()?;
    let names: Vec<String> = detections
        .iter()
        .filter(|d| {
            d.get("confidence")
                .and_then(|c| c.as_f64())
                .map(|c| c >= YOLO_PRESENCE_THRESHOLD)
                .unwrap_or(false)
        })
        .filter_map(|d| d.get("class").and_then(|c| c.as_str()).map(String::from))
        .collect();
    Some(names)
}

/// Map vendor-specific class names to a small shared vocabulary so
/// the comparison doesn't trip on synonyms. Intentionally conservative
/// for v0 — easier to add an alias when a new disagreement type shows
/// up than to debug a too-aggressive merge later.
fn canonicalize_class(name: &str) -> String {
    match name.to_lowercase().as_str() {
        // Head coverings — VLM prompts say "hat", YOLO ontologies
        // from Roboflow Universe commonly emit "head_covering" or
        // "hairnet". Treat them as equivalent for compliance math;
        // the operator-visible UI keeps the original strings.
        "hat" | "head_covering" | "head-cover" | "hairnet" | "headcover" => "head_covering".into(),
        // Torso PPE.
        "apron" | "apron_worn" => "apron".into(),
        // Hand PPE.
        "glove" | "gloves" | "glove_worn" => "glove".into(),
        // Face PPE.
        "mask" | "face_mask" | "facemask" => "mask".into(),
        // Generic person bbox — never a violation, always YOLO-side.
        "person" => "person".into(),
        // Unknown classes flow through unchanged so a new alias
        // appears in the per-class breakdown without crashing.
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vlm(confidence: f64, missing: &[&str]) -> serde_json::Value {
        json!({
            "attire_compliant": missing.is_empty(),
            "hygiene_compliant": true,
            "missing_items": missing,
            "confidence": confidence,
            "notes": "test"
        })
    }

    fn yolo(detections: &[(&str, f64)]) -> serde_json::Value {
        let arr: Vec<serde_json::Value> = detections
            .iter()
            .map(|(class, conf)| {
                json!({
                    "class": class,
                    "confidence": conf,
                    "bbox": { "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.2 },
                })
            })
            .collect();
        json!({ "model": "test", "frame_offset_ms": 0,
                "frame_width": 1280, "frame_height": 720,
                "detections": arr })
    }

    #[test]
    fn both_agree_compliant() {
        let s = vlm(0.9, &[]);
        let y = yolo(&[("hat", 0.92), ("apron", 0.88), ("person", 0.95)]);
        let (status, _) = compute(&s, Some(&y));
        assert_eq!(status, AgreementStatus::Agree);
    }

    #[test]
    fn both_agree_violation() {
        let s = vlm(0.9, &["hat", "apron"]);
        let y = yolo(&[("person", 0.95)]); // no hat or apron detected
        let (status, detail) = compute(&s, Some(&y));
        assert_eq!(status, AgreementStatus::Agree);
        assert_eq!(
            detail.per_class.get("head_covering"),
            Some(&ClassAgreement::Agree)
        );
    }

    #[test]
    fn vlm_says_missing_yolo_sees_present() {
        let s = vlm(0.9, &["hat"]);
        let y = yolo(&[("hat", 0.91), ("person", 0.95)]);
        let (status, detail) = compute(&s, Some(&y));
        assert_eq!(status, AgreementStatus::Disagree);
        assert_eq!(
            detail.per_class.get("head_covering"),
            Some(&ClassAgreement::VlmOnlyMissing)
        );
    }

    #[test]
    fn canonicalization_treats_synonyms_equal() {
        // VLM says "hat" missing; YOLO model uses "head_covering" label.
        // Should NOT be a disagreement just because the labels differ.
        let s = vlm(0.9, &["hat"]);
        let y = yolo(&[("head_covering", 0.93)]);
        let (status, _) = compute(&s, Some(&y));
        assert_eq!(status, AgreementStatus::Disagree); // they ARE genuinely disagreeing on this class's presence
    }

    #[test]
    fn yolo_below_threshold_not_counted_present() {
        // YOLO confidence 0.3 — below the 0.5 floor; VLM rightly
        // flags missing; agreement should be Agree.
        let s = vlm(0.9, &["hat"]);
        let y = yolo(&[("hat", 0.3)]);
        let (status, _) = compute(&s, Some(&y));
        assert_eq!(status, AgreementStatus::Agree);
    }

    #[test]
    fn vlm_low_confidence_returns_inconclusive() {
        let s = vlm(0.2, &["hat"]);
        let y = yolo(&[("person", 0.95)]);
        let (status, _) = compute(&s, Some(&y));
        assert_eq!(status, AgreementStatus::Inconclusive);
    }

    #[test]
    fn missing_yolo_envelope_returns_inconclusive() {
        let s = vlm(0.9, &["hat"]);
        let (status, _) = compute(&s, None);
        assert_eq!(status, AgreementStatus::Inconclusive);
    }

    #[test]
    fn missing_structured_fields_returns_inconclusive() {
        let s = json!({});
        let y = yolo(&[("hat", 0.91)]);
        let (status, _) = compute(&s, Some(&y));
        assert_eq!(status, AgreementStatus::Inconclusive);
    }
}
