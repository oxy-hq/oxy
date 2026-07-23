//! Tests for `seed_apps.rs`, split out to keep that file under the
//! ~400-line limit. Same `#[path]` convention as `worker_tests.rs`;
//! `use super::*` gives access to the module's private items.

use super::*;

fn files(pairs: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    pairs
        .iter()
        .map(|(p, b)| (p.to_string(), b.as_bytes().to_vec()))
        .collect()
}

#[test]
fn build_id_changes_when_bundle_content_changes() {
    // The whole point of hashing: an edited bundle must not reuse the
    // build id a running server has already cached bytes for.
    let a = build_id_for(&files(&[("index.html", "<h1>a</h1>")]));
    let b = build_id_for(&files(&[("index.html", "<h1>b</h1>")]));
    assert_ne!(a, b);
}

#[test]
fn build_id_changes_when_a_file_is_renamed() {
    // Hashing bytes alone would collide here — the path is part of the
    // build, so it must be part of the hash.
    let a = build_id_for(&files(&[("index.html", "x")]));
    let b = build_id_for(&files(&[("index.htm", "x")]));
    assert_ne!(a, b);
}

#[test]
fn build_id_is_stable_for_identical_bundles() {
    let a = build_id_for(&files(&[
        ("index.html", "<h1>a</h1>"),
        ("icon.svg", "<svg/>"),
    ]));
    let b = build_id_for(&files(&[
        ("index.html", "<h1>a</h1>"),
        ("icon.svg", "<svg/>"),
    ]));
    assert_eq!(a, b);
    assert_eq!(a.len(), 16);
}

#[test]
fn app_id_differs_per_org_so_one_bundle_can_back_two_deployments() {
    let a = app_id_for(Uuid::from_u128(1));
    let b = app_id_for(Uuid::from_u128(2));
    assert_ne!(a, b);
    // Deterministic, so a re-seed updates rather than duplicates.
    assert_eq!(a, app_id_for(Uuid::from_u128(1)));
}

#[test]
fn manifest_of_reads_the_bundled_manifest() {
    // A synthetic bundle, deliberately not the checked-in one: this covers
    // extraction, not the example's own manifest (that's
    // `example_bundle_manifest_matches_the_shipping_schema`).
    let f = files(&[
        ("index.html", "<html></html>"),
        ("oxy-app.json", r#"{"schemaVersion":2,"slug":"synthetic"}"#),
    ]);
    let m = manifest_of(&f).expect("manifest");
    assert_eq!(m["slug"], "synthetic");
}

#[test]
fn manifest_of_is_none_without_a_manifest() {
    assert!(manifest_of(&files(&[("index.html", "<html></html>")])).is_none());
}

// ── The checked-in example bundle ────────────────────────────────────────
//
// These read the bundle at `<workspace>/{BUNDLE_REL_PATH}` off disk (i.e.
// `examples/customer_apps/oxy-starter/`) and hold it to the contracts the
// serve path enforces at runtime. The bundle has no build
// step and no test of its own, so without these a broken example ships and
// is only discovered by a developer whose first `oxy seed` hands them a
// dead app — the worst possible first impression, and a slow one to debug
// because every failure mode here is silent (a warn!, a None, an empty
// card) rather than an error.
//
// They deliberately use the SHIPPING types and predicates, not copies.

fn example_bundle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(BUNDLE_REL_PATH)
}

async fn example_bundle() -> Vec<(String, Vec<u8>)> {
    let dir = example_bundle_dir();
    assert!(
        dir.is_dir(),
        "example bundle missing at {} — seed_example_apps() skips silently when the \
         directory is absent, so a moved bundle would quietly stop deploying",
        dir.display()
    );
    read_bundle(&dir).await.expect("read example bundle")
}

fn body(files: &[(String, Vec<u8>)], rel: &str) -> String {
    let (_, bytes) = files
        .iter()
        .find(|(p, _)| p == rel)
        .unwrap_or_else(|| panic!("{rel} missing from the example bundle"));
    String::from_utf8(bytes.clone()).unwrap_or_else(|e| panic!("{rel} is not UTF-8: {e}"))
}

#[tokio::test]
async fn example_bundle_ships_the_files_the_seed_deploys() {
    let files = example_bundle().await;
    for required in ["index.html", "oxy-app.json"] {
        assert!(
            files.iter().any(|(p, _)| p == required),
            "example bundle has no {required}; found {:?}",
            files.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }
    // README.md documents the example; a real publish ships a build output
    // directory, which wouldn't carry it.
    assert!(
        !files.iter().any(|(p, _)| p == "README.md"),
        "README.md should be excluded from the deployed bundle"
    );
}

#[tokio::test]
async fn example_bundle_manifest_matches_the_shipping_schema() {
    let files = example_bundle().await;
    let raw = manifest_of(&files).expect("oxy-app.json parses as JSON");
    // The real type, so a schema change breaks the example loudly here
    // instead of quietly emptying its launcher card.
    let manifest: crate::server::api::custom_apps_manifest::OxyAppManifest =
        serde_json::from_value(raw).expect("oxy-app.json matches OxyAppManifest");
    assert_eq!(
        manifest.schema_version, 2,
        "read_manifest rejects anything but schemaVersion 2"
    );
    assert_eq!(
        manifest.slug, APP_SLUG,
        "the manifest slug should match the slug the seed registers"
    );
    assert!(
        manifest.description.is_some(),
        "the launcher card reads description from the manifest"
    );
}

#[tokio::test]
async fn example_bundle_index_html_has_the_injection_point() {
    // inject_app_config splices window.__OXY_APP__ in before `</head>`, and
    // when there's no `</head>` it only warns. The app would then load with
    // no identity and never query — a broken example with a clean log.
    let files = example_bundle().await;
    assert!(
        body(&files, "index.html").contains("</head>"),
        "index.html needs a </head> for runtime identity injection"
    );
}

#[tokio::test]
async fn example_bundle_icon_and_art_resolve_to_real_files() {
    let files = example_bundle().await;
    let raw = manifest_of(&files).expect("manifest");
    let manifest: crate::server::api::custom_apps_manifest::OxyAppManifest =
        serde_json::from_value(raw).expect("manifest");

    for (field, path) in [("icon", manifest.icon), ("art", manifest.art)] {
        let path = path.unwrap_or_else(|| panic!("manifest declares no {field}"));
        // The launcher's own predicate: a path it rejects is dropped from
        // the card silently.
        assert!(
            crate::server::api::workspace_custom_apps::safe_relative_art_path(&path),
            "manifest {field} {path:?} would be rejected by the launcher"
        );
        assert!(
            files.iter().any(|(p, _)| *p == path),
            "manifest {field} points at {path:?}, which the bundle doesn't contain — \
             the card would 404 on it"
        );
    }
}

#[tokio::test]
async fn example_bundle_index_html_is_self_contained() {
    // The bundle is served verbatim from Oxy's origin with no build step to
    // inline anything, and custom apps must not depend on third-party
    // hosts being reachable (or on what they might serve).
    let html = body(&example_bundle().await, "index.html");
    for needle in ["src=\"http", "href=\"http", "@import", "//cdn."] {
        assert!(
            !html.contains(needle),
            "index.html references an external resource ({needle}); the example bundle \
             must be self-contained"
        );
    }
}
