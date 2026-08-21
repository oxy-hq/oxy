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
    let a = app_id_for(Uuid::from_u128(1), APP_SLUG);
    let b = app_id_for(Uuid::from_u128(2), APP_SLUG);
    assert_ne!(a, b);
    // Deterministic, so a re-seed updates rather than duplicates.
    assert_eq!(a, app_id_for(Uuid::from_u128(1), APP_SLUG));
}

#[test]
fn app_id_differs_per_slug_so_one_org_can_hold_both_seeded_apps() {
    // Acme gets the open app AND the restricted one. Keyed on slug as well as org,
    // or the second deployment would collide onto the first's id and the seed would
    // silently overwrite instead of adding.
    let org = Uuid::from_u128(7);
    assert_ne!(
        app_id_for(org, APP_SLUG),
        app_id_for(org, RESTRICTED_APP_SLUG)
    );
}

#[test]
fn the_restricted_target_keeps_the_org_but_changes_slug_and_visibility() {
    let open = AppTarget::open(Uuid::from_u128(3), "acme".into(), Uuid::from_u128(4));
    assert_eq!(open.slug, APP_SLUG);
    assert!(
        open.restrict_to_team.is_none(),
        "the default deployment must stay org-visible — an org's only app must never \
         be the restricted one, or the launcher looks broken"
    );

    let team = Uuid::from_u128(5);
    let restricted = open.restricted(team);
    assert_eq!(restricted.org_id, open.org_id);
    assert_eq!(restricted.workspace_id, open.workspace_id);
    assert_eq!(restricted.slug, RESTRICTED_APP_SLUG);
    assert_ne!(restricted.slug, open.slug, "the two must not collide");
    assert_eq!(restricted.restrict_to_team, Some(team));
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

/// The viewer panel must address `shell-context` with the **injected** project
/// id, never a baked one.
///
/// The whole point of `window.__OXY_APP__` is that one bundle serves any org: a
/// hardcoded id would work in whichever workspace it was copied from and read
/// somebody else's identity — or 403 — everywhere else. Pattern 3 already makes
/// this claim in prose; pattern 4 is the second place it has to hold.
#[tokio::test]
async fn example_bundle_reads_the_viewer_from_the_injected_project_id() {
    let html = body(&example_bundle().await, "index.html");
    assert!(
        html.contains("/api/projects/${app.projectId}/shell-context"),
        "the viewer panel must template the injected projectId into shell-context"
    );
}

/// A viewer's name and email are **user-controlled free text** — they come from
/// the person's own IdP profile — so the bundle must put them in the DOM as
/// text, never as markup.
///
/// This is the one XSS surface a single-file example can plausibly grow: the
/// warehouse-value helper already carries a "textContent, never innerHTML"
/// comment, and identity is the second source of untrusted strings to arrive.
/// Pinned at the file level because the fix is trivial and the regression is
/// silent — a stored display name of `<img onerror=…>` renders once and works
/// for whoever opens the app next.
///
/// Scans **every** markup/script file in the bundle, not just `index.html`: the
/// rule is stated as a bundle-wide invariant, and a bundle that later grows an
/// `app.js` would otherwise be exempt from it while this docstring still claimed
/// otherwise.
#[tokio::test]
async fn example_bundle_never_writes_untrusted_strings_as_html() {
    // Match the SINKS, not the word: the bundle carries a "textContent, never
    // innerHTML" comment, and a test that fails on its own guidance is a test
    // nobody keeps.
    const SINKS: [&str; 7] = [
        "innerHTML =",
        "innerHTML=",
        "outerHTML =",
        "outerHTML=",
        "insertAdjacentHTML(",
        "createContextualFragment(",
        "document.write(",
    ];

    let files = example_bundle().await;
    let scanned: Vec<String> = files
        .iter()
        .map(|(path, _)| path.clone())
        .filter(|p| p.ends_with(".html") || p.ends_with(".js"))
        .collect();
    assert!(
        !scanned.is_empty(),
        "no .html/.js in the bundle — the scan would pass vacuously"
    );

    for path in scanned {
        let source = body(&files, &path);
        for sink in SINKS {
            assert!(
                !source.contains(sink),
                "{path} uses {sink:?} — this bundle renders warehouse rows AND viewer \
                 identity, both untrusted strings, so every write must go through \
                 textContent (the `text()` helper)"
            );
        }
        // `srcdoc` parses its value as a document — the same sink wearing an
        // attribute. Checked separately: it has no trailing `(` or `=` shape the
        // list above would catch.
        assert!(
            !source.contains("srcdoc"),
            "{path} sets `srcdoc`, which parses its value as markup"
        );
    }
}
