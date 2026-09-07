//! The `example_new` fixture must present as ONE metric tree.
//!
//! The fixture's whole job is to be the thing a person opens the Metric Tree
//! on, and a canvas of eighteen disconnected islands demonstrates nothing. The
//! connections are load-bearing but invisible: a driver edge naming a measure
//! that does not exist is dropped in silence by `MetricTree::build` (see the
//! `node_ids.contains(from_id)` guard), so a typo re-fragments the graph
//! without failing anything. This test is the alarm.
//!
//! It reads the fixture off disk rather than embedding a copy — the point is
//! to check the file a person actually edits.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Repo-root-relative path to the fixture's views.
fn views_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../example_new/semantics/views")
        .canonicalize()
        .expect("example_new fixture should exist at the repo root")
}

fn fixture_layer() -> oxy_airlayer_compat::SemanticLayer {
    let mut views = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(views_dir())
        .expect("views dir should be readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".view.yml"))
        .collect();
    // Deterministic order, so a failure message is reproducible.
    files.sort();
    assert!(!files.is_empty(), "fixture should declare views");

    for path in files {
        let yaml = std::fs::read_to_string(&path).expect("view file should be readable");
        let view = oxy_airlayer_compat::parse_view_yaml(&yaml)
            .unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));
        views.push(view);
    }
    oxy_airlayer_compat::SemanticLayer::new(views, None)
}

/// Connected components of the tree, treated as UNDIRECTED — the question is
/// whether the canvas draws one graph, and an arrow's direction does not
/// change what a person sees.
fn components(tree: &oxy_airlayer_compat::engine::metric_tree::MetricTree) -> Vec<Vec<String>> {
    let mut parent: HashMap<&str, &str> = HashMap::new();
    for node in &tree.nodes {
        parent.insert(node.id.as_str(), node.id.as_str());
    }

    fn find<'a>(parent: &HashMap<&'a str, &'a str>, mut x: &'a str) -> &'a str {
        while parent[x] != x {
            x = parent[x];
        }
        x
    }

    for edge in &tree.edges {
        let (a, b) = (find(&parent, &edge.from), find(&parent, &edge.to));
        if a != b {
            parent.insert(a, b);
        }
    }

    let mut groups: HashMap<&str, Vec<String>> = HashMap::new();
    for node in &tree.nodes {
        groups
            .entry(find(&parent, &node.id))
            .or_default()
            .push(node.id.clone());
    }
    let mut out: Vec<Vec<String>> = groups.into_values().collect();
    for g in &mut out {
        g.sort();
    }
    out.sort_by_key(|g| std::cmp::Reverse(g.len()));
    out
}

#[test]
fn the_fixture_is_a_single_connected_tree() {
    let tree = oxy_airlayer_compat::engine::metric_tree::MetricTree::build(&fixture_layer());
    let groups = components(&tree);

    assert_eq!(
        groups.len(),
        1,
        "example_new fragmented into {} trees; the orphaned groups are {:#?}",
        groups.len(),
        &groups[1..]
    );
    assert_eq!(
        groups[0].len(),
        tree.nodes.len(),
        "every measure should be in the one component"
    );
}

#[test]
fn every_declared_driver_resolves_to_a_real_measure() {
    // `MetricTree::build` drops an unresolvable driver silently, so a typo
    // shows up only as a missing edge. Check the declarations against the
    // node set directly, which names the offender instead of leaving a
    // component count to be interpreted.
    let layer = fixture_layer();
    let ids: HashSet<String> = layer
        .views
        .iter()
        .flat_map(|v| {
            v.measures_list()
                .into_iter()
                .map(move |m| format!("{}.{}", v.name, m.name))
        })
        .collect();

    let mut dangling = Vec::new();
    for view in &layer.views {
        for measure in view.measures_list() {
            for driver in measure.drivers.iter().flatten() {
                if !ids.contains(&driver.measure) {
                    dangling.push(format!(
                        "{}.{} <- {}",
                        view.name, measure.name, driver.measure
                    ));
                }
            }
        }
    }
    assert!(dangling.is_empty(), "unresolvable drivers: {dangling:#?}");
}

#[test]
fn no_store_days_lever_reaches_the_check_grain() {
    // The grain bridge points checks -> store_days on purpose. The scenario
    // baseline walks FORWARD from a lever, so a lever pinned in `store_days`
    // must still resolve inside one grain — that is what keeps every
    // multiplicative impact sizable. An edge added in the other direction
    // would silently degrade the fixture's whole scenario story to
    // "unquantifiable", with nothing failing to say so.
    let tree = oxy_airlayer_compat::engine::metric_tree::MetricTree::build(&fixture_layer());

    let mut forward: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &tree.edges {
        forward
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }

    for start in tree.nodes.iter().filter(|n| n.view == "store_days") {
        let mut seen: HashSet<&str> = HashSet::from([start.id.as_str()]);
        let mut queue = vec![start.id.as_str()];
        while let Some(node) = queue.pop() {
            for next in forward.get(node).map(Vec::as_slice).unwrap_or(&[]) {
                if seen.insert(next) {
                    queue.push(next);
                }
            }
        }
        let escaped: Vec<&&str> = seen
            .iter()
            .filter(|id| !id.starts_with("store_days."))
            .collect();
        assert!(
            escaped.is_empty(),
            "a lever on {} reaches another grain: {escaped:?}",
            start.id
        );
    }
}
