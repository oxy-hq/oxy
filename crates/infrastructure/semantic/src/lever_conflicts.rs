//! Which pinned levers cannot be simulated together, and the refusal itself.
//!
//! # Why this lives here and not next to a handler
//!
//! The refusal used to live ONLY in the browser client
//! (`web-app/src/pages/ide/MetricTree/scenario/leverConflicts.ts` /
//! `useScenario.ts`): a non-empty conflicts list set `blocked = true` and the
//! UI simply never issued the request. That made the refusal a property of one
//! CLIENT rather than of the model — curl, `oxyc`, an SDK integration, an
//! agentic analytics tool, or a scheduled custom-app function could ask the
//! exact same ambiguous pinned-lever set and get back a confident
//! `PredictResult` that had silently picked one of the two readings.
//!
//! Moving it into the HTTP handlers closed that for the two `/predict` routes
//! and missed the caller with the strongest claim on it: the analytics tool
//! (`agentic-analytics`) builds its own tree and calls
//! [`engine::metric_tree_ops::predict`] directly, and there an LLM picks the
//! levers off the tree it was just shown — so pinning a measure and something
//! upstream of it is a natural move rather than an operator error.
//!
//! `agentic-analytics` cannot reach into `oxy-app`, and must not depend on
//! `oxy-semantic` (that is an agentic → platform edge the architecture forbids;
//! see `internal-docs/backend-architecture.md`). What every caller DOES already
//! depend on is this crate. So the rule lives here, beside
//! [`crate::gate_semantic_write`], as one definition rather than a third copy —
//! `oxy_semantic` re-exports it, and `oxy-app`'s `reject_lever_conflicts`
//! delegates to it.
//!
//! The TypeScript copy stays, deliberately: it is a pre-flight that keeps the
//! UI from firing a request it already knows will be refused, not the
//! enforcing copy.

use std::collections::{HashMap, HashSet, VecDeque};

use airlayer::engine::metric_tree::MetricTree;

/// A pair of levers where one is reachable from the other, making the
/// scenario ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LeverConflict {
    /// The lever the effect flows from.
    pub upstream: String,
    /// The lever that sits downstream of it, so its pinned value and its
    /// propagated value disagree.
    pub downstream: String,
}

/// Lever pairs that cannot be simulated together.
///
/// Pinning both `revenue` and a driver of `revenue` is ambiguous: the model
/// cannot tell whether revenue holds at the pinned value despite the driver
/// change, or moves to the implied one. Both readings are defensible, so the
/// caller refuses rather than picking one silently.
///
/// Unknown ids are ignored — validating lever existence is the caller's job
/// and produces a different, clearer error.
pub fn lever_conflicts(tree: &MetricTree, levers: &[String]) -> Vec<LeverConflict> {
    let mut fwd: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &tree.edges {
        fwd.entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }

    // Dedupe: the same lever listed twice is one lever, not a conflict.
    let mut unique: Vec<&str> = Vec::new();
    for lever in levers {
        if !unique.contains(&lever.as_str()) {
            unique.push(lever.as_str());
        }
    }
    let pinned: HashSet<&str> = unique.iter().copied().collect();

    let mut conflicts = Vec::new();
    for &start in &unique {
        // Forward BFS from `start`; any other pinned node reached is downstream.
        let mut seen: HashSet<&str> = HashSet::from([start]);
        let mut queue: VecDeque<&str> = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for &next in fwd.get(node).map(Vec::as_slice).unwrap_or(&[]) {
                if !seen.insert(next) {
                    continue;
                }
                if pinned.contains(next) {
                    conflicts.push(LeverConflict {
                        upstream: start.to_string(),
                        downstream: next.to_string(),
                    });
                }
                queue.push_back(next);
            }
        }
    }
    conflicts
}

/// Refuse a `predict` request that pins two levers where one is reachable from
/// the other. **This is the enforcing copy** — see the module doc for why it
/// sits in this crate.
///
/// Deliberately a different check than the deduping [`lever_conflicts`] already
/// does internally: the same lever id pinned twice is one pinned value
/// restated, not a conflict, and stays allowed here. An upstream/downstream
/// OVERLAP is a different shape of problem — pinning `revenue` and a driver of
/// `revenue` leaves the model unable to tell whether `revenue` holds at the
/// pinned value despite the driver change, or moves to the value the driver
/// relationship implies. Both readings are defensible, so this refuses rather
/// than picking one.
///
/// Returns the message rather than an error type, because its three callers
/// answer in three different error currencies —
/// `MetricTreeError::BadRequest`, `err_with_code(.., "lever_conflict")` on the
/// `projects/` twin, and `ToolError::BadParams` in the analytics tool — and a
/// shared error enum here would have to be convertible into all of them.
pub fn reject_lever_conflicts(tree: &MetricTree, changes: &[(String, f64)]) -> Result<(), String> {
    let levers: Vec<String> = changes.iter().map(|(measure, _)| measure.clone()).collect();
    let conflicts = lever_conflicts(tree, &levers);
    if conflicts.is_empty() {
        return Ok(());
    }
    let pairs = conflicts
        .iter()
        .map(|c| format!("`{}` → `{}`", c.upstream, c.downstream))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "ambiguous scenario: {pairs} are pinned together, but the second is reachable from \
         the first, so it is unclear whether it should hold at its pinned value or move to \
         the value the first lever's change implies. Pin only the upstream lever, or drop one \
         of the two."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use airlayer::SemanticLayer;

    fn chain_tree() -> MetricTree {
        let view = crate::parse_view_yaml(
            r#"
name: orders
table: public.orders
dialect: postgres
measures:
  - name: revenue
    type: sum
    expr: amount
  - name: cost
    type: sum
    expr: cost
  - name: profit
    type: number
    expr: "{{orders.revenue}} - {{orders.cost}}"
  - name: margin_score
    type: number
    expr: "{{orders.profit}} * 2"
"#,
        )
        .expect("view parses");
        MetricTree::build(&SemanticLayer::new(vec![view], None))
    }

    #[test]
    fn no_conflict_for_a_single_lever() {
        let tree = chain_tree();
        assert!(lever_conflicts(&tree, &["orders.revenue".to_string()]).is_empty());
    }

    #[test]
    fn no_conflict_for_independent_levers() {
        let tree = chain_tree();
        let conflicts = lever_conflicts(
            &tree,
            &["orders.revenue".to_string(), "orders.cost".to_string()],
        );
        assert!(
            conflicts.is_empty(),
            "siblings are independent: {conflicts:?}"
        );
    }

    #[test]
    fn direct_downstream_lever_conflicts() {
        let tree = chain_tree();
        let conflicts = lever_conflicts(
            &tree,
            &["orders.revenue".to_string(), "orders.profit".to_string()],
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].upstream, "orders.revenue");
        assert_eq!(conflicts[0].downstream, "orders.profit");
    }

    #[test]
    fn transitive_downstream_lever_conflicts() {
        let tree = chain_tree();
        let conflicts = lever_conflicts(
            &tree,
            &[
                "orders.revenue".to_string(),
                "orders.margin_score".to_string(),
            ],
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].upstream, "orders.revenue");
        assert_eq!(conflicts[0].downstream, "orders.margin_score");
    }

    #[test]
    fn the_same_lever_twice_is_not_a_conflict() {
        let tree = chain_tree();
        let conflicts = lever_conflicts(
            &tree,
            &["orders.revenue".to_string(), "orders.revenue".to_string()],
        );
        assert!(conflicts.is_empty(), "a node is not downstream of itself");
    }

    #[test]
    fn unknown_lever_ids_are_ignored_rather_than_panicking() {
        let tree = chain_tree();
        let conflicts = lever_conflicts(
            &tree,
            &["orders.revenue".to_string(), "orders.nope".to_string()],
        );
        assert!(conflicts.is_empty());
    }

    #[test]
    fn reject_names_both_ends_of_every_conflicting_pair() {
        let tree = chain_tree();
        let changes = vec![
            ("orders.revenue".to_string(), 100.0),
            ("orders.profit".to_string(), 50.0),
        ];
        let message = reject_lever_conflicts(&tree, &changes).expect_err("upstream/downstream");
        assert!(message.contains("ambiguous scenario"), "{message}");
        assert!(message.contains("orders.revenue"), "{message}");
        assert!(message.contains("orders.profit"), "{message}");
    }

    #[test]
    fn reject_allows_independent_levers() {
        let tree = chain_tree();
        let changes = vec![
            ("orders.revenue".to_string(), 100.0),
            ("orders.cost".to_string(), 50.0),
        ];
        assert!(reject_lever_conflicts(&tree, &changes).is_ok());
    }

    #[test]
    fn reject_does_not_flag_the_same_lever_pinned_twice() {
        // One pinned value restated is not two readings to choose between.
        let tree = chain_tree();
        let changes = vec![
            ("orders.revenue".to_string(), 100.0),
            ("orders.revenue".to_string(), 100.0),
        ];
        assert!(reject_lever_conflicts(&tree, &changes).is_ok());
    }
}
