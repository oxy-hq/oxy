//! Oxygen's own catalogue: the four platform notice kinds, in code.
//!
//! The kinds themselves, their channels and which one is critical are
//! `platform::kinds`; this places them in the tree and labels them. A kind
//! from a newer binary that is not here lands in `general`, as an app's
//! undescribed kind does.

use std::collections::BTreeMap;

use super::model::{Action, Catalogue, CategoryDef, KindDef, Level};
use super::source::{Source, SourceCatalogue};
use crate::server::api::notifications::platform::kinds::{
    ANOMALY, BILLING_PAST_DUE, RUN_FAILED, TEST, WORKSPACE_READY,
};
use crate::server::api::notifications::platform::self_test::TITLE as TEST_TITLE;

/// (kind, label, category, category label, level).
const PLATFORM: [(&str, &str, &str, &str, Level); 4] = [
    (
        BILLING_PAST_DUE,
        "Payment past due",
        "billing",
        "Billing",
        Level::Critical,
    ),
    (
        WORKSPACE_READY,
        "Workspace ready",
        "workspaces",
        "Workspaces",
        Level::Active,
    ),
    (
        RUN_FAILED,
        "Run failed",
        "runs",
        "Automations and pipelines",
        Level::TimeSensitive,
    ),
    (
        ANOMALY,
        "Anomaly found",
        "monitoring",
        "Monitoring",
        Level::Active,
    ),
];

/// Every platform notice opens what it is about: its rendered link.
fn open() -> Action {
    Action {
        id: "open".to_string(),
        label: "Open".to_string(),
        function: None,
        destructive: false,
    }
}

/// Oxygen's tree.
pub fn oxygen() -> SourceCatalogue {
    let mut categories = BTreeMap::new();
    let mut kinds = BTreeMap::new();
    for (kind, label, category, category_label, level) in PLATFORM {
        categories.insert(
            category.to_string(),
            CategoryDef {
                label: Some(category_label.to_string()),
                description: None,
            },
        );
        kinds.insert(
            kind.to_string(),
            KindDef {
                category: Some(category.to_string()),
                label: Some(label.to_string()),
                description: None,
                level: Some(level),
                actions: vec![open()],
            },
        );
    }
    // A test notice a person sent themselves: in `general`, and given no
    // settings switch (`KindInfo::has_switch`) — no mute applies to it.
    kinds.insert(
        TEST.to_string(),
        KindDef {
            category: None,
            label: Some(TEST_TITLE.to_string()),
            description: None,
            level: Some(Level::Active),
            actions: vec![open()],
        },
    );
    SourceCatalogue {
        source: Source::Oxygen,
        catalogue: Catalogue { categories, kinds },
        declared: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::notifications::platform::kinds::{KINDS, is_critical};

    #[test]
    fn every_platform_kind_is_placed_and_only_billing_is_critical() {
        let c = oxygen();
        for kind in KINDS {
            let info = c.describe(kind);
            if kind == TEST {
                continue;
            }
            assert_ne!(info.category, "general", "{kind} is placed");
            assert_eq!(info.critical(), is_critical(kind), "{kind}");
            assert_eq!(info.actions.len(), 1, "{kind}");
            assert!(info.actions[0].function.is_none(), "{kind} opens");
        }
        let test = c.describe(TEST);
        assert_eq!(
            (test.category.as_str(), test.label.as_str()),
            ("general", "Test notification")
        );
        assert!(
            !test.mutable && !test.has_switch(),
            "no mute applies to a test"
        );
        assert!(c.describe(WORKSPACE_READY).mutable);
        assert!(!c.describe(BILLING_PAST_DUE).mutable && c.describe(BILLING_PAST_DUE).has_switch());
        let run = c.describe(RUN_FAILED);
        assert_eq!(run.category_labels, vec!["Automations and pipelines"]);
        assert_eq!(run.level, Level::TimeSensitive);
        assert_eq!(
            run.channels,
            crate::server::api::notifications::platform::kinds::channels(RUN_FAILED)
        );
        let newer = c.describe("oxy.something_new");
        assert_eq!(
            (newer.category.as_str(), newer.label.as_str()),
            ("general", "Something new")
        );
    }
}
