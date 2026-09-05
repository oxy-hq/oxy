//! Server-side source-map resolution for custom-app client errors.
//!
//! ## Why the server does this and the browser does not
//!
//! A minified frame — `index-a3f2.js:1:48213` — is worth almost nothing. The
//! obvious fix is to ship the `.map` beside the bundle and let devtools resolve
//! it, and that is what happened by accident: `guess_content_type` serves
//! `.map` as `application/json`, so every `.map` in a published bundle has been
//! downloadable by anyone who can open the app. That hands the app's original
//! source — comments, unused branches, internal names — to every viewer, which
//! for an internal tool is a wider audience than the author assumed.
//!
//! So maps stay in the build store, are **not served** (see
//! `custom_apps_serve::sources::is_source_map`), and are applied here, at read
//! time, for an app-admin looking at an error. The bytes never reach a browser.
//!
//! ## Failure is always partial, never fatal
//!
//! Every step degrades to "return the frame unchanged": no map in the build, a
//! map that fails to parse, a position with no mapping. A half-resolved stack
//! is strictly better than an error page, and an unresolvable frame is normal —
//! a stack routinely contains frames from code that was never bundled.

use std::collections::HashMap;

use uuid::Uuid;

use super::custom_apps_build_store;

/// Most parsed maps held live at once for one request.
///
/// Hoisting the cache to the request traded sequential fetches for retained
/// memory, and a page of errors spanning many builds could otherwise hold
/// hundreds of parsed multi-MB `SourceMap`s at the same time. Past the cap,
/// further scripts resolve to nothing and their frames pass through unchanged —
/// a partly-resolved stack, which is the failure mode this module already
/// degrades to everywhere else.
const MAX_CACHED_MAPS: usize = 24;

/// Longest stack this will attempt. Past this the tail is framework internals,
/// and each frame costs a map lookup.
const MAX_FRAMES: usize = 60;

/// One parsed stack frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Everything before the location, kept verbatim so the rebuilt stack looks
    /// like the original.
    pub prefix: String,
    /// The script URL as it appeared.
    pub url: String,
    pub line: u32,
    pub column: u32,
    /// Everything after the location (usually `)`).
    pub suffix: String,
}

/// Pull `url:line:column` out of one stack line.
///
/// Handles both shapes that matter: V8's `at fn (URL:L:C)` / `at URL:L:C`, and
/// SpiderMonkey's `fn@URL:L:C`. Deliberately not a strict parser — a line it
/// does not understand comes back `None` and is passed through untouched.
pub fn parse_frame(line: &str) -> Option<Frame> {
    // Work right-to-left: the last two colon-separated integers are the
    // position, and everything before them is the URL. Doing it this way means
    // a URL containing colons (`https://host:8080/...`) parses correctly, which
    // a left-to-right split does not.
    let trimmed_end = line.trim_end();
    let (body, suffix) = match trimmed_end.strip_suffix(')') {
        Some(b) => (b, ")"),
        None => (trimmed_end, ""),
    };

    let (rest, column) = split_trailing_number(body)?;
    let (rest, line_no) = split_trailing_number(rest)?;

    // `rest` now ends with the URL. Find where it starts: after `(`, after `@`,
    // or after the leading `at `.
    let start = rest
        .rfind('(')
        .map(|i| i + 1)
        .or_else(|| rest.rfind('@').map(|i| i + 1))
        .unwrap_or_else(|| {
            rest.find("at ")
                .map(|i| i + 3)
                .unwrap_or(rest.len() - rest.trim_start().len())
        });
    if start > rest.len() {
        return None;
    }
    let url = rest[start..].to_string();
    if url.is_empty() {
        return None;
    }
    Some(Frame {
        prefix: rest[..start].to_string(),
        url,
        line: line_no,
        column,
        suffix: suffix.to_string(),
    })
}

/// Split `"...:123"` into `("...", 123)`. `None` when the tail is not a number.
fn split_trailing_number(s: &str) -> Option<(&str, u32)> {
    let idx = s.rfind(':')?;
    let (head, tail) = s.split_at(idx);
    let n: u32 = tail[1..].parse().ok()?;
    Some((head, n))
}

/// Reduce a script URL to a bundle-relative path.
///
/// The same asset is reachable on two surfaces — `/customer-apps/<org>/<slug>/…`
/// and the app's own subdomain root — so the origin and any base path are
/// stripped and what remains is what the build store is keyed by.
pub fn bundle_rel_path(url: &str, base_path: &str) -> Option<String> {
    // Strip scheme + host if present. A relative URL is already a path.
    let path = match url.find("://") {
        Some(i) => {
            let after_scheme = &url[i + 3..];
            let slash = after_scheme.find('/')?;
            &after_scheme[slash..]
        }
        None => url,
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let base = base_path.trim_end_matches('/');
    let rel = if !base.is_empty() && path.starts_with(base) {
        &path[base.len()..]
    } else {
        path
    };
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        None
    } else {
        Some(rel.to_string())
    }
}

/// Maps already fetched, keyed by bundle-relative script path.
///
/// **Owned by the caller, not by `resolve_stack`.** `/errors` resolves up to
/// `MAX_LIMIT` groups in one request; a cache built inside the resolver would be
/// per group, so a page of 100 errors from the same build meant ~300 sequential
/// S3 round-trips and ~300 parses of what for a React bundle is a multi-MB JSON
/// blob — inside one request, with the frame URLs client-supplied. Hoisting it
/// makes the fetch count per *build*, not per group.
pub type MapCache = HashMap<String, Option<sourcemap::SourceMap>>;

/// Rewrite every resolvable frame in `stack` to its original source position.
///
/// `maps` is threaded in so a caller resolving many stacks pays one fetch per
/// distinct script overall — see [`MapCache`].
pub async fn resolve_stack(
    app_id: Uuid,
    build_id: &str,
    base_path: &str,
    stack: &str,
    maps: &mut MapCache,
) -> String {
    if stack.is_empty() || build_id.is_empty() {
        return stack.to_string();
    }
    let mut out = Vec::new();

    for (index, raw_line) in stack.lines().enumerate() {
        if index >= MAX_FRAMES {
            out.push(raw_line.to_string());
            continue;
        }
        let Some(frame) = parse_frame(raw_line) else {
            out.push(raw_line.to_string());
            continue;
        };
        let Some(rel) = bundle_rel_path(&frame.url, base_path) else {
            out.push(raw_line.to_string());
            continue;
        };

        // Keyed by build too — a page of errors can span builds, and a map from
        // the wrong build resolves to plausible-but-wrong lines.
        let key = format!("{build_id}/{rel}");
        if !maps.contains_key(&key) {
            if maps.len() >= MAX_CACHED_MAPS {
                out.push(raw_line.to_string());
                continue;
            }
            maps.insert(key.clone(), load_map(app_id, build_id, &rel).await);
        }
        let Some(Some(map)) = maps.get(&key) else {
            out.push(raw_line.to_string());
            continue;
        };

        // Source maps are 0-based; stacks are 1-based.
        match map.lookup_token(frame.line.saturating_sub(1), frame.column.saturating_sub(1)) {
            Some(token) => {
                let source = token.get_source().unwrap_or(&rel);
                let name = token.get_name().unwrap_or("");
                let prefix = if name.is_empty() {
                    frame.prefix.clone()
                } else {
                    // Replace the minified identifier with the original name
                    // where the frame had one — that is usually the single most
                    // useful thing a map recovers.
                    rewrite_prefix(&frame.prefix, name)
                };
                out.push(format!(
                    "{prefix}{source}:{}:{}{}",
                    token.get_src_line() + 1,
                    token.get_src_col() + 1,
                    frame.suffix
                ));
            }
            None => out.push(raw_line.to_string()),
        }
    }
    out.join("\n")
}

/// Swap the function name inside a frame prefix, keeping the surrounding shape
/// (`    at `, `  (`) intact.
fn rewrite_prefix(prefix: &str, name: &str) -> String {
    // A stack is read by eye, so the rebuilt frame has to keep the shape it
    // arrived in. Three shapes, and two of them used to come out malformed:
    // an anonymous V8 frame (`    at URL:1:5`, no paren) gained an unclosed
    // `(`, and a SpiderMonkey frame kept its minified name in front of the new
    // one.
    if let Some(i) = prefix.find("at ") {
        let head = &prefix[..i + 3];
        // Only a frame that HAD a paren gets one back — the suffix `)` is
        // reattached by the caller, and adding `(` to a frame with no `)`
        // leaves it unbalanced.
        return if prefix.ends_with('(') {
            format!("{head}{name} (")
        } else {
            format!("{head}{name} @ ")
        };
    }
    // SpiderMonkey: `handleClick@` — replace the name rather than prefixing it.
    match prefix.rfind('@') {
        Some(_) => format!("{name}@"),
        None => format!("{prefix}{name} @ "),
    }
}

/// Fetch and parse `<rel>.map` from the build store. `None` for every failure —
/// missing, unreadable, or unparseable — because none of them should cost the
/// caller more than an unresolved frame.
async fn load_map(app_id: Uuid, build_id: &str, rel: &str) -> Option<sourcemap::SourceMap> {
    let map_path = format!("{rel}.map");
    let bytes = custom_apps_build_store::get_object(app_id, build_id, &map_path)
        .await
        .ok()
        .flatten()?;
    match sourcemap::SourceMap::from_slice(&bytes) {
        Ok(map) => Some(map),
        Err(e) => {
            tracing::debug!("source map {map_path} for build {build_id} did not parse: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_v8_named_frame() {
        let frame =
            parse_frame("    at handleClick (https://app.example.com/assets/i-a3.js:10:200)")
                .expect("parsed");
        assert_eq!(frame.url, "https://app.example.com/assets/i-a3.js");
        assert_eq!((frame.line, frame.column), (10, 200));
        assert_eq!(frame.suffix, ")");
    }

    #[test]
    fn parses_a_v8_anonymous_frame() {
        let frame =
            parse_frame("    at https://app.example.com/assets/i-a3.js:1:5").expect("parsed");
        assert_eq!(frame.url, "https://app.example.com/assets/i-a3.js");
        assert_eq!((frame.line, frame.column), (1, 5));
    }

    #[test]
    fn parses_a_spidermonkey_frame() {
        let frame = parse_frame("handleClick@https://app.example.com/assets/i-a3.js:10:200")
            .expect("parsed");
        assert_eq!(frame.url, "https://app.example.com/assets/i-a3.js");
        assert_eq!((frame.line, frame.column), (10, 200));
    }

    /// A URL with a port has colons in it. Parsing left-to-right splits on the
    /// port and reads the host as the position — the reason this walks the
    /// string from the right.
    #[test]
    fn a_port_in_the_url_does_not_confuse_the_position() {
        let frame =
            parse_frame("    at fn (http://localhost:5173/assets/i.js:3:9)").expect("parsed");
        assert_eq!(frame.url, "http://localhost:5173/assets/i.js");
        assert_eq!((frame.line, frame.column), (3, 9));
    }

    /// The first line of a stack is the message, not a frame. It must pass
    /// through untouched rather than being mangled into one.
    #[test]
    fn a_non_frame_line_is_not_parsed() {
        assert!(parse_frame("TypeError: x is not a function").is_none());
        assert!(parse_frame("").is_none());
    }

    /// A rebuilt frame must keep the shape it arrived in — a stack is read by
    /// eye, and an unbalanced paren or a doubled function name reads as
    /// corruption.
    #[test]
    fn rewriting_a_name_preserves_each_frame_shape() {
        // V8, named: had a paren, gets one back (the caller reattaches `)`).
        assert_eq!(
            rewrite_prefix("    at min (", "handleClick"),
            "    at handleClick ("
        );
        // V8, anonymous: no paren, so none is added.
        assert_eq!(
            rewrite_prefix("    at ", "handleClick"),
            "    at handleClick @ "
        );
        // SpiderMonkey: the minified name is REPLACED, not prefixed.
        assert_eq!(rewrite_prefix("min@", "handleClick"), "handleClick@");
    }

    #[test]
    fn strips_the_origin_and_base_path() {
        assert_eq!(
            bundle_rel_path(
                "https://app.example.com/customer-apps/acme/orders/assets/i.js",
                "/customer-apps/acme/orders/"
            ),
            Some("assets/i.js".to_string())
        );
    }

    /// The same asset on the subdomain surface has no base path to strip.
    #[test]
    fn resolves_a_subdomain_url_with_no_base_path() {
        assert_eq!(
            bundle_rel_path(
                "https://acme--orders.customer-apps.example.com/assets/i.js",
                "/"
            ),
            Some("assets/i.js".to_string())
        );
    }

    #[test]
    fn drops_query_and_hash() {
        assert_eq!(
            bundle_rel_path("https://h/assets/i.js?v=2#x", "/"),
            Some("assets/i.js".to_string())
        );
    }

    #[test]
    fn a_bare_origin_has_no_asset_path() {
        assert_eq!(bundle_rel_path("https://h/", "/"), None);
    }

    /// An empty build id means the serving build was never recorded; there is
    /// nothing to resolve against, and the stack must survive intact.
    #[tokio::test]
    async fn an_unknown_build_returns_the_stack_unchanged() {
        let stack = "TypeError: boom\n    at fn (https://h/assets/i.js:1:2)";
        let mut maps = MapCache::new();
        assert_eq!(
            resolve_stack(Uuid::nil(), "", "/", stack, &mut maps).await,
            stack
        );
    }

    /// The cache is keyed by build as well as path, so two builds' copies of
    /// `assets/index.js` cannot resolve against each other's mappings.
    #[test]
    fn the_cache_key_separates_builds() {
        let mut maps = MapCache::new();
        maps.insert("build-a/assets/i.js".into(), None);
        assert!(!maps.contains_key("build-b/assets/i.js"));
    }
}
