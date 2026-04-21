use super::*;

#[test]
fn missing_member_round_trips() {
    let member = MissingMember {
        name: "revenue_per_customer".into(),
        kind: MissingMemberKind::Measure,
        description: "Average revenue divided by distinct customer count".into(),
    };
    let json = serde_json::to_value(&member).unwrap();
    assert_eq!(json["kind"], "measure");

    let back: MissingMember = serde_json::from_value(json).unwrap();
    assert_eq!(back, member);
}

#[test]
fn hypothesis_with_missing_members_deserializes() {
    let json = serde_json::json!({
        "summary": "user wants revenue per customer",
        "question_type": "SingleValue",
        "confidence": 0.9,
        "semantic_confidence": 0.4,
        "missing_members": [
            {
                "name": "revenue_per_customer",
                "kind": "measure",
                "description": "total revenue / distinct customers"
            }
        ]
    });
    let h: DomainHypothesis = serde_json::from_value(json).unwrap();
    assert_eq!(h.missing_members.len(), 1);
    assert_eq!(h.missing_members[0].kind, MissingMemberKind::Measure);
}

#[test]
fn hypothesis_without_missing_members_defaults_to_empty() {
    let json = serde_json::json!({
        "summary": "simple question",
        "question_type": "Trend",
        "confidence": 0.9,
        "semantic_confidence": 0.9,
    });
    let h: DomainHypothesis = serde_json::from_value(json).unwrap();
    assert!(h.missing_members.is_empty());
}
