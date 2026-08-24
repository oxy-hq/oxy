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
mod tests {
    use super::*;

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
