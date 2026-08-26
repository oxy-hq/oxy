//! The HTTP route table, as data.
//!
//! Axum keeps no route table at runtime — the router is a tower service, not a
//! list — so the catalog is extracted from the router source at build time by
//! `crates/app/build_route_catalog.rs` and included here. That is what lets
//! `oxy api --help` / `oxy api --routes` enumerate the API without a server
//! running and without a hand-maintained list that rots.
//!
//! # Scope
//!
//! **The `/api` and `/external/api` surfaces**, which is what `oxy api` can
//! call. `paths_are_well_formed` enforces the boundary, so these are excluded
//! by construction, not by accident:
//!
//! - `ANY /customer-apps/{*path}` — the custom-app bundle serve tree, mounted
//!   at the top level because it is browser-facing (`cli/commands/serve.rs`).
//! - `/apidoc` and `/apidoc/openapi.json` — Swagger UI and the spec itself.
//! - The worker health port (`/healthz`, `/readyz`, `/metrics`), served only
//!   when `oxy worker --health-port` is set.
//! - `internal_api_router` — the unauthenticated loopback port
//!   (`--internal-port`), which mirrors API routes without auth.
//!
//! A route inside those bounds that is *missing* is a walker bug. A route
//! outside them was never in scope.
//!
//! The tests below are the completeness gate: they fail if the walk stops
//! reaching a surface or a landmark route, so a router refactor the lexical
//! walker can no longer follow shows up in CI rather than as a quietly
//! shrinking `oxy api --help`.

use serde::Serialize;

use super::role_manifest;

/// One endpoint, exactly as the router mounts it.
pub struct GeneratedRoute {
    /// Uppercase HTTP method (`GET`, `POST`, …; `ANY` for method-agnostic mounts).
    pub method: &'static str,
    /// Full path pattern including the `/api` prefix, e.g.
    /// `/api/{workspace_id}/threads`.
    pub path: &'static str,
    /// The handler the route dispatches to, for cross-referencing the source.
    /// Empty when the handler is a closure or another non-path expression.
    pub handler: &'static str,
    /// Which authentication surface the route sits on — see [`surfaces`].
    pub surface: &'static str,
    /// The handler's own doc comment, flattened to one line — what the
    /// endpoint does. Empty when the handler carries none (roughly half of
    /// them do not).
    pub description: &'static str,
    /// The comment the router source carries on this mount — usually *why* the
    /// route sits where it does, and the gotcha that put it there. Empty when
    /// the mount has none.
    ///
    /// Both `description` and `note` are harvested at build time from the
    /// engineers' own comments, so they are a hint, never a contract. The
    /// schemas live in the OpenAPI document (`oxy api --openapi`).
    pub note: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/route_catalog_generated.rs"));

/// Every route on the `/api` and `/external/api` surfaces, sorted by surface
/// then path. See the module docs for what those bounds leave out.
pub fn routes() -> &'static [GeneratedRoute] {
    GENERATED_ROUTES
}

/// The pre-rendered listing `oxy api --help` appends to its help text.
pub fn listing() -> &'static str {
    GENERATED_ROUTE_LISTING
}

/// Surfaces in display order: `(id, label, the credential it expects)`.
pub fn surfaces() -> &'static [(&'static str, &'static str, &'static str)] {
    GENERATED_SURFACES
}

/// A route rendered for machine consumption (`oxy api --routes --json`).
#[derive(Serialize)]
pub struct RouteDescription {
    pub method: &'static str,
    pub path: &'static str,
    pub surface: &'static str,
    /// The credential this surface expects, spelled out rather than implied.
    pub credential: &'static str,
    /// `{…}` segments of the path, in order — the values a caller substitutes.
    pub path_parameters: Vec<&'static str>,
    pub description: &'static str,
    pub note: &'static str,
    pub handler: &'static str,
    /// `ide-only` when the handler needs the workspace working copy / `.git`,
    /// so a stateless serve replica forwards or refuses it; `worker-only` for
    /// the queue-drainer surface; `fleet-ok` otherwise. Straight from
    /// [`role_manifest`], the same table the server enforces.
    pub role: &'static str,
}

/// The `{name}` (and `{*name}`) segments of a path pattern, in order.
fn path_parameters(path: &'static str) -> Vec<&'static str> {
    path.split('/')
        .filter_map(|seg| {
            seg.strip_prefix('{')?
                .strip_suffix('}')
                .map(|n| n.trim_start_matches('*'))
        })
        .collect()
}

/// Routes whose method or path contains `needle` (case-insensitive), or every
/// route when `needle` is `None`.
pub fn search(needle: Option<&str>) -> Vec<&'static GeneratedRoute> {
    let needle = needle.map(str::to_lowercase);
    routes()
        .iter()
        .filter(|r| match &needle {
            None => true,
            Some(n) => {
                r.path.to_lowercase().contains(n)
                    || r.method.to_lowercase().contains(n)
                    || r.surface.to_lowercase().contains(n.as_str())
            }
        })
        .collect()
}

pub fn describe(route: &'static GeneratedRoute) -> RouteDescription {
    RouteDescription {
        method: route.method,
        path: route.path,
        surface: route.surface,
        credential: surfaces()
            .iter()
            .find(|(id, _, _)| *id == route.surface)
            .map(|(_, _, credential)| *credential)
            .unwrap_or_default(),
        path_parameters: path_parameters(route.path),
        description: route.description,
        note: route.note,
        handler: route.handler,
        role: role_manifest::classify(route.method, route.path).as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Route groups that must survive any router refactor.
    ///
    /// The bare count floor below cannot see a *shaped* loss: drop every route
    /// under one nested builder and the total barely moves. This list is the
    /// guard for that — one fragment per subtree the walker has to keep
    /// reaching. It is deliberately about groups, not individual routes, so
    /// adding or renaming an endpoint does not churn it.
    ///
    /// A crate that mounts at **N** seams needs **N** entries, not one: a group
    /// another builder already satisfies pins nothing. `oxy-api-onboarding`
    /// fills two seams, and `/api/orgs` alone is satisfied by
    /// `build_global_routes` — so without the org entry below, dropping its
    /// org seed would lose eight endpoints with every guard still green.
    ///
    /// (A full golden file would catch more, at the cost of a regenerate step
    /// in every routing PR. If that trade ever looks right, this is the thing
    /// to replace.)
    const REQUIRED_GROUPS: &[&str] = &[
        "/api/health",
        "/api/auth/",
        "/api/orgs",
        // The org seam of `oxy-api-onboarding`; `/api/orgs` above does not
        // cover it (see the note on N seams).
        "/api/orgs/{org_id}/onboarding",
        "/api/customer-apps",
        "/api/assume",
        "/api/airhouse/",
        "/api/partners/",
        "/api/user/github/",
        "/api/admin/apps",
        "/api/admin/compiles",
        "/api/admin/internal-jobs",
        "/api/admin/feature-flags",
        "/api/control/",
        "/api/fleet/",
        "/api/{workspace_id}/threads",
        "/api/{workspace_id}/agents",
        "/api/{workspace_id}/files",
        "/api/{workspace_id}/databases",
        "/api/{workspace_id}/secrets",
        "/api/{workspace_id}/apps",
        "/api/{workspace_id}/api-keys",
        "/api/{workspace_id}/tests",
        "/api/{workspace_id}/traces",
        "/api/{workspace_id}/metrics",
        "/api/{workspace_id}/execution-analytics",
        "/api/{workspace_id}/semantic",
        "/api/{workspace_id}/analytics",
        "/api/{workspace_id}/agentic-workflows",
        "/api/{workspace_id}/agentic-airway",
        "/api/{workspace_id}/agentic-schedules",
        "/api/{workspace_id}/world-model",
        "/api/{workspace_id}/sql/",
        "/api/{workspace_id}/integrations",
        "/api/{workspace_id}/repositories",
        "/api/{workspace_id}/onboarding",
        "/api/{workspace_id}/cameras",
        "/external/api/",
    ];

    /// Files that mount routes outside the trees the walker scans, each for a
    /// reason. Kept explicit so a *new* one fails
    /// [`every_route_tree_is_scanned`] instead of silently going unlisted —
    /// which is how a whole sibling API crate would otherwise disappear.
    const UNSCANNED_ROUTE_FILES: &[&str] = &[
        // The walker itself: `.route(` appears as a string literal.
        "crates/app/build_route_catalog.rs",
        // Mounts `/customer-apps/{*path}` and the SwaggerUI tree at the top
        // level, outside `/api` — deliberately out of scope (see module docs).
        "crates/app/src/cli/commands/serve.rs",
        // The `oxy worker --health-port` surface: /healthz, /readyz, /metrics.
        "crates/app/src/server/worker_health.rs",
    ];

    /// The floor exists so a router refactor the lexical walker can no longer
    /// follow fails here instead of quietly emptying `oxy api --help`.
    #[test]
    fn catalog_covers_the_whole_surface() {
        assert!(
            routes().len() > 400,
            "route catalog collapsed to {} entries — build_route_catalog.rs can no longer \
             follow the router source. Check the SEEDS list and the .nest/.merge walk.",
            routes().len()
        );
    }

    /// Complements the count floor: a nested subtree can vanish whole without
    /// moving the total much, but not without emptying one of these.
    #[test]
    fn every_route_group_survives() {
        for group in REQUIRED_GROUPS {
            assert!(
                routes().iter().any(|r| r.path.starts_with(group)),
                "no routes left under {group:?} — a builder the walk used to \
                 reach is no longer being followed"
            );
        }
    }

    /// The catalog is only as complete as `SOURCE_DIRS`, and adding a tree to
    /// that list is a step someone will forget — adding `oxy-api-partner-console`
    /// took four separate list edits. This walks the workspace and fails on any
    /// file that mounts routes from outside the scanned trees and is not a
    /// known, reasoned exception.
    ///
    /// Reads the checkout, so it no-ops where the sources are not present.
    #[test]
    fn every_route_tree_is_scanned() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let crates = root.join("crates");
        if !crates.is_dir() {
            return;
        }

        let mut unscanned = Vec::new();
        collect_route_files(&crates, &root, &mut unscanned);
        unscanned.retain(|rel| {
            !GENERATED_SCANNED_DIRS
                .iter()
                .any(|(dir, _)| rel.starts_with(&format!("{dir}/")))
                && !UNSCANNED_ROUTE_FILES.contains(&rel.as_str())
        });
        unscanned.sort();

        assert!(
            unscanned.is_empty(),
            "these files mount routes but sit outside every scanned tree, so \
             `oxy api --routes` will not list them: {unscanned:#?}\n\
             Add the tree to SOURCE_DIRS in crates/app/build_route_catalog.rs, \
             or add the file to UNSCANNED_ROUTE_FILES with the reason."
        );

        for known in UNSCANNED_ROUTE_FILES {
            assert!(
                root.join(known).exists(),
                "UNSCANNED_ROUTE_FILES lists {known:?}, which no longer exists — \
                 remove the stale entry."
            );
        }
    }

    /// Every scanned tree has to actually produce routes.
    ///
    /// `every_route_tree_is_scanned` closes one half — a tree that declares
    /// routes and is not listed. This closes the other: a tree that *is*
    /// listed but is never walked, because whoever added it to `SOURCE_DIRS`
    /// forgot the matching `SEEDS` entry. That combination is otherwise
    /// completely silent — the crate gets indexed, no seed warning fires
    /// (those only report a seed that is listed and no longer resolves), and
    /// no count moves.
    ///
    /// Granularity is the *tree*, which is the unit a new sibling crate
    /// arrives as. A single builder going unreached inside a tree that still
    /// contributes elsewhere is not caught here — `every_route_group_survives`
    /// is the guard for that shape.
    #[test]
    fn every_scanned_tree_contributes_routes() {
        for (dir, contributed) in GENERATED_SCANNED_DIRS {
            assert!(
                *contributed > 0,
                "{dir:?} is in SOURCE_DIRS but produced no routes. Most likely its \
                 builder has no entry in SEEDS (crates/app/build_route_catalog.rs), so \
                 the tree is indexed and never walked — remove the tree or wire up the \
                 seed. Also possible, if rarer: every route it mounts duplicates \
                 another tree's, since the count is taken after `collect`'s dedup."
            );
        }
    }

    /// Repo-relative paths of every non-test source file containing a
    /// `.route(` mount.
    ///
    /// Mirrors two of the walker's skip rules by hand — `tests` directories
    /// and the `#[cfg(test)]` truncation. Kept in step with `collect_dir` and
    /// `truncate_at_test_module` in `crates/app/build_route_catalog.rs`: if
    /// either changes, change this too, or the test starts reporting files the
    /// walker would never have looked at.
    fn collect_route_files(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if name != "tests" && name != "target" && name != "node_modules" {
                    collect_route_files(&path, root, out);
                }
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            // `#[cfg(test)]` modules build probe routers that never ship.
            let src = text.split("\n#[cfg(test)]").next().unwrap_or_default();
            if src.contains(".route(")
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    #[test]
    fn every_surface_is_reachable() {
        let found: HashSet<&str> = routes().iter().map(|r| r.surface).collect();
        for (surface, _, _) in surfaces() {
            assert!(
                found.contains(surface),
                "no routes found for the {surface:?} surface — its seed in \
                 build_route_catalog.rs no longer resolves"
            );
        }
    }

    /// One landmark per surface. These are load-bearing endpoints; if the walk
    /// stops reaching one, the help is lying to whoever reads it.
    #[test]
    fn landmark_routes_are_present() {
        for (method, path) in [
            ("GET", "/api/health"),
            ("GET", "/api/user"),
            ("GET", "/api/orgs"),
            ("GET", "/api/orgs/{org_id}/workspaces"),
            ("GET", "/api/admin/apps"),
            ("GET", "/api/{workspace_id}/threads"),
            ("GET", "/api/{workspace_id}/agents"),
            ("POST", "/api/{workspace_id}/sql/query"),
            ("POST", "/api/{workspace_id}/semantic"),
            ("POST", "/external/api/{workspace_id}/sql/query"),
        ] {
            assert!(
                routes()
                    .iter()
                    .any(|r| r.method == method && r.path == path),
                "landmark route {method} {path} missing from the catalog"
            );
        }
    }

    #[test]
    fn paths_are_well_formed() {
        for r in routes() {
            assert!(
                r.path.starts_with("/api/") || r.path.starts_with("/external/api/"),
                "route {} {} is mounted outside the API surface",
                r.method,
                r.path
            );
            assert!(
                !r.path.contains("//"),
                "route {} has an empty path segment",
                r.path
            );
            assert!(
                r.method.chars().all(|c| c.is_ascii_uppercase()),
                "method {:?} should be uppercase",
                r.method
            );
        }
    }

    #[test]
    fn entries_are_unique() {
        let mut seen = HashSet::new();
        for r in routes() {
            assert!(
                seen.insert((r.method, r.path, r.surface)),
                "duplicate catalog entry {} {} ({})",
                r.method,
                r.path,
                r.surface
            );
        }
    }

    #[test]
    fn path_parameters_are_extracted_in_order() {
        assert_eq!(
            path_parameters("/api/{workspace_id}/threads/{id}/messages"),
            vec!["workspace_id", "id"]
        );
        assert_eq!(path_parameters("/api/health"), Vec::<&str>::new());
        // Axum's catch-all keeps its name without the star.
        assert_eq!(
            path_parameters("/api/{workspace_id}/cameras/{cam_id}/preview/hls/{*tail}"),
            vec!["workspace_id", "cam_id", "tail"]
        );
    }

    /// The point of harvesting prose is that a caller learns what a route does
    /// without the source. If the harvest breaks, the routes are still listed
    /// and nothing else fails — so assert on it explicitly.
    #[test]
    fn a_good_share_of_routes_carry_prose() {
        let documented = routes()
            .iter()
            .filter(|r| !r.description.is_empty() || !r.note.is_empty())
            .count();
        assert!(
            documented * 3 > routes().len(),
            "only {documented} of {} routes carry a description or a note — the doc \
             harvest in build_route_catalog.rs stopped resolving handlers",
            routes().len()
        );
    }

    #[test]
    fn describe_reports_the_surface_credential() {
        let health = routes()
            .iter()
            .find(|r| r.path == "/api/health")
            .expect("health route");
        let described = describe(health);
        assert_eq!(described.surface, "public");
        assert!(described.credential.contains("no credential"));
    }

    #[test]
    fn search_filters_by_path_fragment() {
        let hits = search(Some("threads"));
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|r| r.path.contains("threads")));
        assert_eq!(search(None).len(), routes().len());
    }

    #[test]
    fn describe_reports_the_fleet_role() {
        // `/files` reads the working copy, so it is pinned to the ide; the
        // thread list is served from Postgres and runs anywhere.
        let files = routes()
            .iter()
            .find(|r| r.method == "GET" && r.path == "/api/{workspace_id}/files")
            .expect("files listing route");
        assert_eq!(describe(files).role, "ide-only");

        let threads = routes()
            .iter()
            .find(|r| r.method == "GET" && r.path == "/api/{workspace_id}/threads")
            .expect("threads listing route");
        assert_eq!(describe(threads).role, "fleet-ok");
    }

    #[test]
    fn listing_renders_every_surface() {
        for (_, label, _) in surfaces() {
            assert!(
                listing().contains(*label),
                "the --help listing is missing the {label:?} section"
            );
        }
        assert!(listing().contains("/api/health"));
        assert!(listing().contains("/external/api/"));
    }
}
