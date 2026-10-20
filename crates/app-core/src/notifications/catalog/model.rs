//! The catalogue's vocabulary: levels, actions, category paths.
//!
//! Every rule here is shared by the publish check (`validate`), the runtime
//! read of a build's catalogue (`parse`) and Oxygen's own (`platform`), so the
//! three cannot disagree about what a path or an action is.

use std::collections::BTreeMap;

use serde::Serialize;

/// The implicit category of a kind that names none.
pub const GENERAL: &str = "general";
pub const GENERAL_LABEL: &str = "General";
/// Deepest category path: `a/b/c`.
pub const MAX_DEPTH: usize = 3;
/// Most actions one kind offers: what a lock-screen notification shows.
pub const MAX_ACTIONS: usize = 3;
/// Longest action label: a button on a phone.
pub const MAX_ACTION_LABEL_CHARS: usize = 24;
pub const MAX_LABEL_CHARS: usize = 64;
pub const MAX_DESCRIPTION_CHARS: usize = 200;
/// Longest path segment and action id.
const MAX_SEGMENT_CHARS: usize = 32;

/// How loudly a kind interrupts. Only `critical` ignores mutes. Mobile maps
/// them to iOS interruption levels and Android importance
/// (`apns::interruption_level`, `fcm::message`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Passive,
    #[default]
    Active,
    TimeSensitive,
    Critical,
}

impl Level {
    pub const ALL: [Level; 4] = [
        Level::Passive,
        Level::Active,
        Level::TimeSensitive,
        Level::Critical,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Passive => "passive",
            Level::Active => "active",
            Level::TimeSensitive => "time_sensitive",
            Level::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_str() == s)
    }
}

/// A button a kind offers. No `function` is "open": it follows the deep link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub label: String,
    /// A function the app declares, called as the person who tapped.
    pub function: Option<String>,
    pub destructive: bool,
}

/// An action as a client sees it: whether it runs something, never which
/// function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionView {
    pub id: String,
    pub label: String,
    pub destructive: bool,
    pub runs_function: bool,
}

impl From<&Action> for ActionView {
    fn from(a: &Action) -> Self {
        Self {
            id: a.id.clone(),
            label: a.label.clone(),
            destructive: a.destructive,
            runs_function: a.function.is_some(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategoryDef {
    pub label: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KindDef {
    /// `None` is the implicit [`GENERAL`].
    pub category: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    /// `None`: from the functions that send it (`critical: true` → critical),
    /// else active.
    pub level: Option<Level>,
    pub actions: Vec<Action>,
}

/// A source's declared tree: categories by path, kinds by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalogue {
    pub categories: BTreeMap<String, CategoryDef>,
    pub kinds: BTreeMap<String, KindDef>,
}

impl Catalogue {
    pub fn is_empty(&self) -> bool {
        self.categories.is_empty() && self.kinds.is_empty()
    }
}

/// One path segment, or an action id: `[a-z0-9][a-z0-9_-]{0,31}`.
pub fn valid_segment(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    s.len() <= MAX_SEGMENT_CHARS
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
}

/// Every segment well formed, ignoring depth.
pub fn well_formed_path(path: &str) -> bool {
    path.split('/').all(valid_segment)
}

/// The number of segments in `path`.
pub fn depth(path: &str) -> usize {
    path.split('/').count()
}

/// A category path a manifest may declare: well formed and at most
/// [`MAX_DEPTH`] deep.
pub fn valid_path(path: &str) -> bool {
    well_formed_path(path) && depth(path) <= MAX_DEPTH
}

/// `path` and every category above it, root first: `a/b/c` → `a`, `a/b`,
/// `a/b/c`.
pub fn ancestors(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for segment in path.split('/') {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(segment);
        out.push(acc.clone());
    }
    out
}

/// The category directly above `path`, `None` at the top.
pub fn parent(path: &str) -> Option<&str> {
    path.rsplit_once('/').map(|(p, _)| p)
}

/// Whether `path` is `scope` or below it. `""` is the whole source.
pub fn is_within(path: &str, scope: &str) -> bool {
    scope.is_empty()
        || path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// A label for a name nobody labelled: `list_overdue` → `List overdue`,
/// `food-safety` → `Food safety`, `delivery.received` → `Delivery received`,
/// `oxy.run_failed` → `Run failed` (Oxygen's prefix is dropped: the source
/// already says Oxygen). Also the native push banner for an undescribed kind.
pub fn humanize(name: &str) -> String {
    let name = name
        .strip_prefix(crate::notifications::input::RESERVED_KIND_PREFIX)
        .unwrap_or(name);
    let words: Vec<String> = name
        .split(|c: char| matches!(c, '_' | '.' | '-') || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let text = words.join(" ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "New notification".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_one_to_three_lowercase_segments() {
        for ok in ["operations", "operations/food-safety", "a/b/c", "0ps/x_y"] {
            assert!(valid_path(ok), "{ok}");
        }
        for bad in [
            "",
            "Operations",
            "a//b",
            "/a",
            "a/",
            "-a",
            "a b",
            "a/b/c/d",
            &"x".repeat(33),
        ] {
            assert!(!valid_path(bad), "{bad:?}");
        }
        assert!(well_formed_path("a/b/c/d"), "too deep is not malformed");
    }

    #[test]
    fn a_scope_covers_its_subtree_and_nothing_beside_it() {
        assert!(is_within("ops/food", ""));
        assert!(is_within("ops/food", "ops"));
        assert!(is_within("ops", "ops"));
        assert!(!is_within("ops", "ops/food"));
        assert!(!is_within("opsx/food", "ops"), "a prefix is not a parent");
        assert_eq!(ancestors("a/b/c"), vec!["a", "a/b", "a/b/c"]);
        assert_eq!(parent("a/b"), Some("a"));
        assert_eq!(parent("a"), None);
    }

    #[test]
    fn names_humanize_and_levels_round_trip() {
        assert_eq!(humanize("list_overdue"), "List overdue");
        assert_eq!(humanize("food-safety"), "Food safety");
        for l in Level::ALL {
            assert_eq!(Level::parse(l.as_str()), Some(l));
        }
        assert_eq!(Level::parse("urgent"), None);
        assert_eq!(
            serde_json::to_value(Level::TimeSensitive).unwrap(),
            "time_sensitive"
        );
    }
}
