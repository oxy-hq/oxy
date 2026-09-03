//! `GET /api/_catalog` — the route table, served by the deployment that owns it.
//!
//! # Why this exists
//!
//! `oxyc` (the TypeScript CLI, `sdk/cli`) is the discovery surface for anyone
//! who has a token and nothing else: no checkout, no doc site, no Swagger UI
//! they can browse. It has to be able to answer "what can I call here?" — and
//! it cannot generate the answer, because the answer comes from a build-time
//! walk of this crate's router source (`build_route_catalog.rs`).
//!
//! The Rust `oxy api --routes` answered it from the table baked into the
//! binary. That has one flaw this endpoint fixes: the baked table describes
//! what the BINARY could mount, and several mounts are mode-conditional
//! (`/setup/*` and the git routes exist only in local mode), so a listed path
//! could still 404 on the deployment in front of you. Asking the deployment
//! is strictly more truthful, and it is the only form of the question that
//! stays correct as a caller moves between local, dev and production.
//!
//! # Why it is authenticated
//!
//! It sits on the protected router. A complete route table for a multi-tenant
//! SaaS — admin surfaces, partner console, billing, every path parameter — is
//! a reconnaissance gift, and there is no caller who needs it before they have
//! a token: the CLI holds one for every other command it runs. The client
//! caches per host, so the cost is one call an hour, not one per command.
//!
//! # Scope
//!
//! Whatever `server::route_catalog` covers: the `/api` and `/external/api`
//! surfaces. See that module for what is deliberately outside those bounds
//! (the custom-app bundle tree, the worker health port, the internal loopback
//! router).

use axum::Json;
use axum::extract::Query;
use serde::{Deserialize, Serialize};

use crate::server::route_catalog::{self, RouteDescription};

/// `?filter=` narrows the table server-side.
#[derive(Deserialize, Default)]
pub struct CatalogQuery {
    /// Substring matched against method, path, surface and description.
    filter: Option<String>,
}

/// The document `oxyc` caches per host.
#[derive(Serialize)]
pub struct CatalogResponse {
    /// Every route on the `/api` and `/external/api` surfaces.
    routes: Vec<RouteDescription>,
    /// Surfaces in display order, each with the credential it expects.
    surfaces: Vec<CatalogSurface>,
    /// The build this table was generated from, so a stale client cache and a
    /// redeployed server can be told apart without guessing from timestamps.
    version: &'static str,
}

#[derive(Serialize)]
pub struct CatalogSurface {
    id: &'static str,
    label: &'static str,
    credential: &'static str,
}

/// The route table for this deployment, with what each endpoint does and which
/// credential its surface expects.
///
/// Consumed by `oxyc routes` / `oxyc schema`; the OpenAPI document at
/// `/apidoc/openapi.json` carries the request/response schemas for the
/// curated subset that has them.
///
/// `?filter=<substring>` narrows it here rather than in the client. The whole
/// table is ~670 routes and a few hundred KB of prose; a caller looking for
/// three of them should not have to download all of it, and a filtered request
/// is also the shape a cache-less caller wants.
pub async fn get_catalog(Query(query): Query<CatalogQuery>) -> Json<CatalogResponse> {
    let needle = query
        .filter
        .as_deref()
        .map(str::trim)
        .filter(|f| !f.is_empty());
    Json(CatalogResponse {
        routes: route_catalog::search(needle)
            .into_iter()
            .map(route_catalog::describe)
            .collect(),
        surfaces: route_catalog::surfaces()
            .iter()
            .map(|(id, label, credential)| CatalogSurface {
                id,
                label,
                credential,
            })
            .collect(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The endpoint's whole job is to be non-empty and to carry both surfaces.
    /// An empty catalog is indistinguishable, on the client, from "this
    /// deployment has no such route" — which is why `oxyc` refuses one rather
    /// than serving it, and why the server must never produce one.
    #[tokio::test]
    async fn catalog_carries_both_surfaces_and_is_not_empty() {
        let Json(catalog) = get_catalog(Query(CatalogQuery::default())).await;

        assert!(
            catalog.routes.len() > 100,
            "the catalog shrank to {} routes — the build-time walk probably stopped following the router",
            catalog.routes.len()
        );
        assert!(
            catalog.routes.iter().any(|r| r.path.starts_with("/api/")),
            "no /api routes in the catalog"
        );
        assert!(
            catalog
                .routes
                .iter()
                .any(|r| r.path.starts_with("/external/api/")),
            "no /external/api routes in the catalog"
        );
        assert!(!catalog.surfaces.is_empty(), "no surfaces described");
    }

    /// Every route reports a role, because `oxyc routes` filters on it: the
    /// default view hides `ide-only` and `worker-only` mounts, which a caller
    /// hitting the load balancer cannot reach directly.
    #[tokio::test]
    async fn every_route_carries_a_role() {
        let Json(catalog) = get_catalog(Query(CatalogQuery::default())).await;
        for route in &catalog.routes {
            assert!(
                matches!(route.role, "fleet-ok" | "ide-only" | "worker-only"),
                "{} {} reported an unknown role {:?}",
                route.method,
                route.path,
                route.role
            );
        }
    }

    /// The catalog must describe ITSELF. A caller that cannot discover the
    /// discovery endpoint has to be told about it out of band, which is the
    /// situation this whole surface exists to remove.
    #[tokio::test]
    async fn the_catalog_lists_itself() {
        let Json(catalog) = get_catalog(Query(CatalogQuery::default())).await;
        assert!(
            catalog.routes.iter().any(|r| r.path == "/api/_catalog"),
            "/api/_catalog is missing from its own route table — the build-time \
             walk did not see its mount"
        );
    }

    /// `?filter=` narrows server-side, so a client after three routes does not
    /// download all ~670.
    #[tokio::test]
    async fn the_filter_narrows_the_table() {
        let Json(all) = get_catalog(Query(CatalogQuery::default())).await;
        let Json(filtered) = get_catalog(Query(CatalogQuery {
            filter: Some("threads".into()),
        }))
        .await;

        assert!(
            filtered.routes.len() < all.routes.len(),
            "the filter returned everything — it is not being applied"
        );
        assert!(!filtered.routes.is_empty(), "no route matched `threads`");
        // The haystack is built from EXACTLY the fields `search` reads, via the
        // same function it reads them with. Listing them here by hand made the
        // assertion a superset of the real filter, so it passed whether or not
        // `description` was searched — which is precisely how the endpoint's
        // doc came to claim a field the filter did not match.
        for route in &filtered.routes {
            let matched = route_catalog::routes()
                .iter()
                .find(|r| r.method == route.method && r.path == route.path)
                .map(route_catalog::searchable_fields)
                .expect("a filtered route is in the catalog")
                .iter()
                .any(|field| field.to_lowercase().contains("threads"));
            assert!(
                matched,
                "{} {} matched `threads` but contains it in none of the searchable fields",
                route.method, route.path
            );
        }
    }

    /// `description` really is searched, not merely documented as searched.
    ///
    /// The generic filter test cannot show this: `threads` appears in the path
    /// of every route it returns, so it would pass against a filter that
    /// ignored descriptions entirely. This needs a needle that lives ONLY in a
    /// description.
    #[tokio::test]
    async fn the_filter_reaches_descriptions() {
        // A word from some handler's doc comment that is in no path.
        let needle = route_catalog::routes()
            .iter()
            .filter(|r| !r.description.is_empty())
            .find_map(|r| {
                r.description
                    .split_whitespace()
                    .map(|w| {
                        w.trim_matches(|c: char| !c.is_alphanumeric())
                            .to_lowercase()
                    })
                    .find(|w| {
                        w.len() > 7
                            && route_catalog::routes()
                                .iter()
                                .all(|other| !other.path.to_lowercase().contains(w.as_str()))
                    })
            })
            .expect("some description carries a word that appears in no path");

        let Json(got) = get_catalog(Query(CatalogQuery {
            filter: Some(needle.clone()),
        }))
        .await;
        assert!(
            !got.routes.is_empty(),
            "`?filter={needle}` matched nothing — it only appears in a description, so \
             the filter is not reading descriptions"
        );
    }

    /// An empty or whitespace-only filter means "everything", not "nothing" —
    /// `?filter=` from a client that built the query string unconditionally
    /// must not come back as an empty catalog, which every caller reads as
    /// "this deployment has no routes".
    #[tokio::test]
    async fn a_blank_filter_is_not_a_filter() {
        let Json(all) = get_catalog(Query(CatalogQuery::default())).await;
        for blank in ["", "   "] {
            let Json(got) = get_catalog(Query(CatalogQuery {
                filter: Some(blank.into()),
            }))
            .await;
            assert_eq!(
                got.routes.len(),
                all.routes.len(),
                "a blank filter ({blank:?}) narrowed the table"
            );
        }
    }
}
