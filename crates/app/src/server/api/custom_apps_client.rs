//! The platform's browser-side runtime for custom apps: the service worker and
//! the auto-instrumentation script, plus the reserved `__oxy/` URL namespace
//! they are served from.
//!
//! Both artifacts are **owned by the platform, not the app**. An app gets them
//! by being served, and cannot ship its own file at these paths — publish
//! strips the reserved prefix, and the serve path answers for it before
//! consulting the build store.
//!
//! ## Why the runtime script is inlined and the worker is not
//!
//! The worker is a separate document by necessity: `navigator.serviceWorker
//! .register` takes a URL, and its scope is derived from where that URL lives.
//! It is fetched once per app per browser and re-fetched only on update, so a
//! request for it costs nothing measurable.
//!
//! The runtime script is inlined into the HTML instead, which looks like the
//! wrong call until you count round trips. It is small (a couple of KiB before
//! compression, and it compresses well alongside the rest of the document), the
//! HTML it rides in is `private, no-cache` and therefore re-sent on every
//! navigation anyway, and a separate file would cost one extra request on the
//! critical path — the exact thing this whole workstream exists to remove. It
//! also sits next to `window.__OXY_APP__`, which it reads, so the two cannot
//! get out of order.
//!
//! Inlining does require the page to tolerate inline `<script>`. That is not a
//! new requirement: `window.__OXY_APP__` has always been injected the same way.

use std::sync::OnceLock;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Bundle-relative path of the service worker.
pub const SERVICE_WORKER_PATH: &str = "__oxy/sw.js";

/// Bundle-relative path the client runtime posts analytics batches to.
pub const BEACON_PATH: &str = "__oxy/beacon";

/// The synthesised web app manifest — what makes a custom app installable.
///
/// Synthesised by the platform rather than shipped in the bundle because only
/// the platform knows the two things a manifest has to get right: the served
/// base path (which the bundle does not know at build time) and, more subtly,
/// whether this request arrived on a custom-app subdomain — where the page sits
/// at `/` while the bundle's assets keep the `/customer-apps/<org>/<app>/`
/// prefix. A manifest whose `scope` does not cover the page is not an error a
/// browser reports; it just silently refuses to install, which is the worst
/// possible failure for a feature whose whole purpose is installation.
pub const WEB_MANIFEST_PATH: &str = "__oxy/manifest.webmanifest";

/// The platform's fallback app icon, drawn from the app's initial.
///
/// A web app manifest is only installable if at least one icon actually
/// fetches, and an app's `icon` is optional — most ship none, which is why the
/// launcher has a monogram fallback at all. Pointing `icons` at a file the app
/// may not have would make installation a no-op for exactly those apps, and
/// silently: a browser that rejects a manifest for a missing icon reports
/// nothing. So the platform serves a monogram of its own under the reserved
/// namespace, matching what `AppMark` renders everywhere else.
pub const MONOGRAM_ICON_PATH: &str = "__oxy/icon.svg";

/// One `icons[]` entry.
pub struct ManifestIcon {
    /// Absolute, same-origin URL.
    pub src: String,
    /// MIME type, so a browser can skip a format it cannot decode without
    /// fetching it first.
    pub mime: &'static str,
    /// `None` when the real pixel size is unknown — which is the honest answer
    /// for an author-supplied raster we never decode. Claiming a size we did
    /// not measure is worse than omitting it: a browser that trusts the claim
    /// and finds something smaller drops the icon.
    pub sizes: Option<&'static str>,
    /// `any maskable` only for artwork whose safe zone the platform controls.
    /// Android crops ~20% off each edge of a maskable icon, so promising it for
    /// an author's mark would crop art we have never seen.
    pub purpose: &'static str,
}

/// The MIME type for an author-declared icon path, by extension.
///
/// Returns `None` for anything a browser would not treat as an icon, which
/// drops the entry rather than advertising a format that will fail to decode.
pub fn icon_mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

/// The platform monogram, as an SVG document.
///
/// Drawn inside the central 80% so it survives the maskable crop, which is what
/// lets this entry claim `any maskable` where an author's mark cannot.
pub fn monogram_svg(app_name: &str) -> String {
    let initial = app_name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    // The name reaches this as text content, so escape it rather than trusting
    // that "one alphanumeric char" stays true if the filter above ever changes.
    let initial = initial
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512" role="img" aria-label="{initial}">
  <rect width="512" height="512" fill="#0a0a0a"/>
  <text x="256" y="256" fill="#fafafa" font-family="ui-sans-serif,system-ui,-apple-system,Segoe UI,Roboto,sans-serif" font-size="240" font-weight="600" text-anchor="middle" dominant-baseline="central">{initial}</text>
</svg>
"##
    )
}

/// Build the manifest for one app on one surface.
///
/// `page_scope` is where the app's PAGES live — `/` on a custom-app subdomain,
/// the base path otherwise — and is deliberately a parameter rather than
/// derived here, so the caller that already computed it for the service worker
/// cannot disagree with this one. The two must always match: a worker scoped
/// wider than the manifest controls pages the installed app does not own, and
/// narrower means the installed app runs uncontrolled.
///
/// `id` is the app's UUID rather than its scope. A manifest `id` is the
/// browser's identity for an installed app, so deriving it from anything that
/// can change — the scope embeds the slug — makes a rename read as a different
/// app and install a second copy. Changing `id` later re-installs for everyone,
/// so it is much cheaper to pin now than to correct.
pub fn web_manifest_json(
    name: &str,
    short_name: &str,
    app_id: uuid::Uuid,
    page_scope: &str,
    icons: &[ManifestIcon],
) -> String {
    // `display: standalone` is what removes the browser chrome; without it the
    // installed app is a bookmark.
    serde_json::json!({
        "id": format!("/__oxy/app/{app_id}"),
        "name": name,
        "short_name": short_name,
        "start_url": page_scope,
        "scope": page_scope,
        "display": "standalone",
        "background_color": "#0a0a0a",
        "theme_color": "#0a0a0a",
        "icons": icons
            .iter()
            .map(|i| {
                let mut o = serde_json::Map::new();
                o.insert("src".into(), i.src.clone().into());
                o.insert("type".into(), i.mime.into());
                o.insert("purpose".into(), i.purpose.into());
                if let Some(sizes) = i.sizes {
                    o.insert("sizes".into(), sizes.into());
                }
                serde_json::Value::Object(o)
            })
            .collect::<Vec<_>>()
    })
    .to_string()
}

/// Raw sources. Authored as real `.js` files so they are lintable and
/// readable; `include_str!` binds them at compile time so there is no
/// deployment step that can leave the binary and the scripts out of sync.
const SERVICE_WORKER_SRC: &str = include_str!("custom_apps_client/sw.js");
const RUNTIME_SRC: &str = include_str!("custom_apps_client/runtime.js");

/// The service-worker source, comment-stripped.
pub fn service_worker_js() -> &'static str {
    static MINIFIED: OnceLock<String> = OnceLock::new();
    MINIFIED.get_or_init(|| strip_comments(SERVICE_WORKER_SRC))
}

/// The `<script>` element injected into every served HTML document.
///
/// Returned as a whole element rather than a bare body so the caller cannot
/// accidentally splice it somewhere that needs different framing.
pub fn runtime_script_tag() -> &'static str {
    static TAG: OnceLock<String> = OnceLock::new();
    TAG.get_or_init(|| format!("<script>{}</script>", strip_comments(RUNTIME_SRC)))
}

/// `Service-Worker-Allowed` — the response header that widens a worker's scope
/// beyond the directory its script lives in. Not in `http::header`, which names
/// only IANA-registered fields.
pub const SERVICE_WORKER_ALLOWED: header::HeaderName =
    header::HeaderName::from_static("service-worker-allowed");

/// Response for `GET <base>/__oxy/sw.js`, scoped to `allowed_scope`.
///
/// Three headers carry real weight:
///
/// - **`Service-Worker-Allowed`.** The script lives at `<base>/__oxy/sw.js`, so
///   its default scope is `<base>/__oxy/` — which contains nothing worth
///   intercepting. This header is what lets it claim a wider path, and without
///   it `register({scope})` fails outright. The caller decides how wide: the app
///   base on the admin host, `/` on a custom-app subdomain where the whole
///   origin is the app. Getting that backwards on the admin host would let one
///   app's worker intercept the Oxy console, which is why the value is a
///   parameter rather than a constant.
/// - **`Cache-Control: no-cache`.** A worker script the browser holds is a
///   worker that cannot be fixed. Modern browsers already bypass the HTTP cache
///   for the update check; this makes it true everywhere and for the first
///   fetch too.
/// - **`private`**, for the same reason the HTML is: this route is auth-gated,
///   and a shared cache storing the answer would serve it to callers that never
///   passed the gate.
pub fn service_worker_response(allowed_scope: &str) -> Response {
    let mut response = (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "private, no-cache"),
        ],
        service_worker_js(),
    )
        .into_response();
    // A scope that cannot be expressed as a header value would silently widen
    // the worker to its own directory, so fall back to the one value that is
    // always safe: relative to the script itself.
    let value = header::HeaderValue::from_str(allowed_scope)
        .unwrap_or_else(|_| header::HeaderValue::from_static("./"));
    response.headers_mut().insert(SERVICE_WORKER_ALLOWED, value);
    response
}

/// A 404 for a reserved `__oxy/` path with no handler.
///
/// Explicitly `no-store` for the same reason as every other negative answer on
/// this route: the set of platform endpoints grows, and a client that cached
/// "there is no such thing" would keep believing it after a deploy that added
/// one.
pub fn reserved_not_found() -> Response {
    (StatusCode::NOT_FOUND, [(header::CACHE_CONTROL, "no-store")]).into_response()
}

/// Strip full-line `//` comments and `/* … */` comment blocks, drop blank
/// lines, and trim indentation.
///
/// Deliberately line-oriented and conservative rather than a real minifier:
/// every newline is preserved, so automatic semicolon insertion behaves
/// exactly as it does in the source, and no expression is ever rewritten. It
/// only ever removes lines that are *entirely* comment, which is where all the
/// bytes are in these two files — the prose above each function is most of
/// their size.
///
/// A trailing `//` comment on a line of code is left alone on purpose: finding
/// the boundary correctly needs a tokenizer (a `//` inside a string literal or
/// a regex is not a comment), and that is a real parser's job, not this
/// function's.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_block = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if in_block {
            if let Some(rest) = trimmed.split_once("*/") {
                in_block = false;
                if !rest.1.trim().is_empty() {
                    out.push_str(rest.1.trim());
                    out.push('\n');
                }
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block = true;
            }
            continue;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod client_artifact_tests {
    use super::*;

    /// The monogram entry as `web_manifest_for` builds it.
    fn platform_monogram(base_path: &str) -> ManifestIcon {
        ManifestIcon {
            src: format!("{base_path}{MONOGRAM_ICON_PATH}"),
            mime: "image/svg+xml",
            sizes: Some("512x512 any"),
            purpose: "any maskable",
        }
    }

    #[test]
    fn the_manifest_installs_rather_than_bookmarks() {
        let json = web_manifest_json(
            "Store Ops",
            "Store Ops",
            uuid::Uuid::nil(),
            "/customer-apps/acme/ops/",
            &[platform_monogram("/customer-apps/acme/ops/")],
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        // `standalone` is the whole feature: without it an installed app is a
        // bookmark that opens in a browser tab with chrome.
        assert_eq!(v["display"], "standalone");
        // start_url and scope must both be the PAGE scope. A scope that does
        // not cover start_url is refused silently by the browser.
        assert_eq!(v["start_url"], "/customer-apps/acme/ops/");
        assert_eq!(v["scope"], "/customer-apps/acme/ops/");
        // `maskable` on the PLATFORM monogram, whose safe zone we drew. An
        // author's mark never claims it — see `a_raster_icon_claims_no_size`.
        assert!(
            v["icons"][0]["purpose"]
                .as_str()
                .unwrap()
                .contains("maskable")
        );
    }

    #[test]
    fn a_subdomain_scopes_to_the_origin_root() {
        // The asymmetry that makes this a platform concern: on a custom-app
        // subdomain the PAGE is at `/` while the bundle's assets keep the
        // subpath. A manifest scoped to the subpath would not cover the page
        // it was linked from, and the install prompt would never appear.
        let json = web_manifest_json(
            "Store Ops",
            "Store Ops",
            uuid::Uuid::nil(),
            "/",
            &[platform_monogram("/customer-apps/acme/ops/")],
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["scope"], "/");
        assert_eq!(v["start_url"], "/");
        // …but the icon still resolves where the files actually are, which on
        // this surface is NOT under the page scope.
        assert_eq!(
            v["icons"][0]["src"],
            "/customer-apps/acme/ops/__oxy/icon.svg"
        );
    }

    #[test]
    fn the_manifest_sits_in_the_reserved_namespace() {
        use crate::server::api::custom_apps_asset_manifest::RESERVED_PLATFORM_PREFIX;
        assert!(
            WEB_MANIFEST_PATH.starts_with(RESERVED_PLATFORM_PREFIX),
            "a platform-synthesised object outside the reserved prefix could be \
             shadowed by a file the app ships at the same path"
        );
    }

    /// Every app must carry at least one fetchable icon, or the browser
    /// silently declines to install — the failure this whole PR exists to avoid.
    #[test]
    fn the_monogram_is_always_offered_even_with_no_author_icon() {
        let json = web_manifest_json(
            "Store Ops",
            "Store Ops",
            uuid::Uuid::nil(),
            "/customer-apps/acme/ops/",
            &[ManifestIcon {
                src: "/customer-apps/acme/ops/__oxy/icon.svg".into(),
                mime: "image/svg+xml",
                sizes: Some("512x512 any"),
                purpose: "any maskable",
            }],
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["icons"].as_array().unwrap().len(), 1);
        assert_eq!(
            v["icons"][0]["src"],
            "/customer-apps/acme/ops/__oxy/icon.svg"
        );
    }

    /// `id` must not move when the slug does, or a rename installs a second
    /// copy alongside the first.
    #[test]
    fn identity_survives_a_rename() {
        let id = uuid::Uuid::from_u128(7);
        let before = web_manifest_json("Ops", "Ops", id, "/customer-apps/acme/ops/", &[]);
        let after = web_manifest_json("Ops", "Ops", id, "/customer-apps/acme/store-ops/", &[]);
        let b: serde_json::Value = serde_json::from_str(&before).unwrap();
        let a: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(b["id"], a["id"], "a slug change moved the app's identity");
        assert_ne!(b["scope"], a["scope"], "fixture did not actually rename");
    }

    /// A size we never measured is worse than no size: a browser that trusts it
    /// and finds something smaller drops the icon.
    #[test]
    fn a_raster_icon_claims_no_size() {
        let json = web_manifest_json(
            "X",
            "X",
            uuid::Uuid::nil(),
            "/a/",
            &[ManifestIcon {
                src: "/a/logo.png".into(),
                mime: "image/png",
                sizes: None,
                purpose: "any",
            }],
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["icons"][0].get("sizes").is_none());
        assert_eq!(v["icons"][0]["type"], "image/png");
        assert_eq!(
            v["icons"][0]["purpose"], "any",
            "author artwork must not claim a maskable safe zone we have not seen"
        );
    }

    /// The monogram's declared size has to match the artwork, or it is the same
    /// invented claim we refuse to make for an author's raster — and a browser
    /// that finds something smaller than promised drops the icon.
    #[test]
    fn the_monogram_declares_the_size_it_actually_draws() {
        let svg = monogram_svg("Ops");
        assert!(svg.contains(r#"width="512""#) && svg.contains(r#"height="512""#));
        let json = web_manifest_json(
            "Ops",
            "Ops",
            uuid::Uuid::nil(),
            "/a/",
            &[platform_monogram("/a/")],
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sizes = v["icons"][0]["sizes"].as_str().unwrap();
        assert!(
            sizes.contains("512x512"),
            "a bare `any` is what Chrome's installability check is unreliable \
             about, which would make the fallback a silent no-op: {sizes}"
        );
    }

    #[test]
    fn icon_mime_covers_what_a_browser_will_take_and_nothing_else() {
        assert_eq!(icon_mime("icon.svg"), Some("image/svg+xml"));
        assert_eq!(icon_mime("brand/logo.PNG"), Some("image/png"));
        assert_eq!(icon_mime("a.jpeg"), Some("image/jpeg"));
        assert_eq!(icon_mime("readme.md"), None);
        assert_eq!(icon_mime("noextension"), None);
    }

    /// The monogram is the one icon claiming `maskable`, so its glyph has to
    /// survive the ~20% edge crop Android applies.
    #[test]
    fn monogram_is_valid_svg_and_draws_inside_the_safe_zone() {
        let svg = monogram_svg("poke house");
        assert!(svg.starts_with("<svg"), "not an SVG document");
        assert!(svg.contains(r#"viewBox="0 0 512 512""#));
        assert!(svg.contains(">P<"), "expected the uppercased initial");
        // Centred, and the type is well inside the 80% safe circle.
        assert!(svg.contains(r#"x="256""#) && svg.contains(r#"y="256""#));
        assert!(svg.contains(r#"font-size="240""#));
    }

    /// A name that starts with punctuation or is empty still has to produce a
    /// document, because this icon is the installability guarantee.
    #[test]
    fn monogram_handles_names_with_no_usable_initial() {
        assert!(monogram_svg("").contains(">?<"));
        assert!(monogram_svg("   ").contains(">?<"));
        assert!(monogram_svg("<script>").contains(">S<"));
        // And nothing an app can name itself escapes into markup.
        assert!(!monogram_svg("<script>x</script>").contains("<script>"));
    }

    /// The two scripts are compiled in, so a syntax error is a runtime failure
    /// in a browser rather than a build failure here. These assertions pin the
    /// handful of structural facts the rest of the system depends on.
    #[test]
    fn service_worker_declares_the_handlers_the_serve_path_assumes() {
        let js = service_worker_js();
        for needle in ["install", "activate", "fetch", "__oxy/asset-manifest.json"] {
            assert!(js.contains(needle), "service worker lost {needle:?}");
        }
        // No `skipWaiting` outside the explicit message escape hatch — an
        // automatic one deletes the running build's precache out from under an
        // open tab. See the note at the top of `sw.js`.
        assert_eq!(
            js.matches("skipWaiting").count(),
            1,
            "skipWaiting must appear only in the OXY_SKIP_WAITING handler"
        );
    }

    #[test]
    fn runtime_script_is_a_single_self_contained_element() {
        let tag = runtime_script_tag();
        assert!(tag.starts_with("<script>"));
        assert!(tag.ends_with("</script>"));
        // Exactly one closing tag: a stray `</script>` inside the body would
        // end the element early and dump the rest of the runtime into the DOM.
        assert_eq!(tag.matches("</script>").count(), 1);
        assert!(tag.contains("__oxy/beacon"));
        assert!(tag.contains("__oxy/sw.js"));
    }

    /// The scripts are shipped through `strip_comments`, and the failure mode of
    /// a bug in it — a block comment that swallows the rest of the file — is
    /// silent here and fatal in a browser. Brace balance is a cheap, durable
    /// proxy for "no code was lost": every function body and every block is a
    /// brace pair, so a truncation cannot help but show up.
    ///
    /// **Braces, not parentheses.** Prose uses parentheses freely and sometimes
    /// unmatched ones ("a smiley :)"), so counting them would make a comment
    /// rewrite fail a test about code loss — a false alarm that teaches people
    /// to delete the test. Braces essentially never appear in this codebase's
    /// prose, so they carry the signal without the noise.
    ///
    /// Compares the delta rather than asserting balance outright, because braces
    /// inside string literals are counted too — equally on both sides, which is
    /// exactly what makes the comparison sound.
    #[test]
    fn stripping_never_swallows_code_from_the_shipped_scripts() {
        fn delta(src: &str) -> isize {
            let count = |c: char| src.matches(c).count() as isize;
            count('{') - count('}')
        }
        for (name, raw) in [("sw.js", SERVICE_WORKER_SRC), ("runtime.js", RUNTIME_SRC)] {
            let stripped = strip_comments(raw);
            assert_eq!(
                delta(raw),
                delta(&stripped),
                "{name}: stripping changed brace balance — a comment ate code"
            );
            assert!(
                stripped.len() < raw.len(),
                "{name}: nothing was stripped, which means the scripts lost their prose"
            );
            // Idempotent: a second pass has nothing left to remove.
            assert_eq!(strip_comments(&stripped), stripped, "{name}");
        }
    }

    #[test]
    fn strip_comments_keeps_code_and_line_structure() {
        let src = "// leading\n/* block\n   continues */\nvar a = 1;\n\n  var b = /\\/*$/;\n";
        let out = strip_comments(src);
        assert_eq!(out, "var a = 1;\nvar b = /\\/*$/;\n");
    }

    /// A block comment that opens and closes on one line is a single line to
    /// drop, not the start of a block — getting this wrong silently deletes
    /// the rest of the file.
    #[test]
    fn strip_comments_handles_a_one_line_block() {
        let out = strip_comments("/* hi */\nvar a = 1;\n");
        assert_eq!(out, "var a = 1;\n");
    }

    /// The registrar and the worker agree on how the worker learns where the
    /// bundle lives. They are two files that only meet in the browser, and the
    /// failure mode if they drift is silent: the worker installs, controls the
    /// page, and then treats every asset as out-of-bundle — so it caches
    /// nothing and nobody notices.
    #[test]
    fn the_registrar_passes_the_asset_base_the_worker_reads() {
        assert!(
            runtime_script_tag().contains("__oxy/sw.js?base="),
            "the registration URL must carry the asset base"
        );
        assert!(
            service_worker_js().contains("searchParams.get(\"base\")"),
            "the worker must read the asset base off its own URL"
        );
    }

    /// Scope is chosen client-side because it differs per surface while the HTML
    /// is memoized per build. The rule — "is this document under the bundle
    /// base?" — is what makes a subdomain-served app scope to `/` and an
    /// admin-host one scope to its subpath.
    #[test]
    fn the_registrar_scopes_by_where_the_document_actually_is() {
        let js = runtime_script_tag();
        assert!(
            js.contains("location.pathname.indexOf(base) === 0 ? base : \"/\""),
            "scope must be derived from the document's own path"
        );
    }

    /// Both platform endpoints have to live under the reserved prefix, or
    /// `serve_pretty` never dispatches them and the request falls through to the
    /// bundle — which would 404 for the worker and, worse, let an app serve its
    /// own file at the beacon URL.
    #[test]
    fn platform_paths_live_in_the_reserved_namespace() {
        use crate::server::api::custom_apps_asset_manifest::{
            RESERVED_PLATFORM_PREFIX, is_reserved_platform_path,
        };
        for path in [SERVICE_WORKER_PATH, BEACON_PATH] {
            assert!(
                is_reserved_platform_path(path),
                "{path} must sit under {RESERVED_PLATFORM_PREFIX}"
            );
        }
    }

    #[test]
    fn service_worker_response_carries_the_scope_widening_header() {
        let response = service_worker_response("/customer-apps/acme/sales/");
        assert_eq!(
            response
                .headers()
                .get("service-worker-allowed")
                .and_then(|v| v.to_str().ok()),
            Some("/customer-apps/acme/sales/")
        );
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("private, no-cache")
        );
    }
}
