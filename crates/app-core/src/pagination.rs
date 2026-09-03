//! `Link` headers for list endpoints that page on the server.
//!
//! ## The gap this closes
//!
//! Much of the API pages perfectly well and is **silent about it to the
//! client**. `admin/users`, `admin/orgs/meta` and `admin/workspaces` take
//! `page`/`page_size`; `admin/audit`, `/assume/history` and the partner
//! console's audit read take `limit`/`offset`. Every one of them answers with a
//! bare JSON array carrying no total, no `has_more` and no link — so a caller
//! holding the first fifty rows has no way to learn there are three thousand
//! more. `oxyc api --paginate` returned page one and it read as the whole table.
//!
//! The fix is a **header, not an envelope**. Wrapping the rows in
//! `{items, pagination}` would break every existing consumer of these endpoints
//! at once; `Link` is additive, invisible to anything that does not look for it,
//! and is the shape `gh api --paginate` and every HTTP client already
//! understands. `oxyc`'s `--paginate` reads `Link: rel="next"` BEFORE any of its
//! body heuristics — its own comment calls that "free correctness if one ever
//! appears" — so an endpoint that adopts this becomes correctly walkable with no
//! client change at all.
//!
//! ## Why `rel="first"` is emitted even on the last page
//!
//! Because absence of a header cannot be allowed to mean two things. A last page
//! with no `Link` at all is byte-for-byte identical to an endpoint that has
//! never heard of pagination, so a client that warns "this response carried no
//! pagination signal" — which `oxyc --paginate` does, and should — would fire
//! that warning on precisely the endpoints this module fixes, every time the
//! result fits in one page. `rel="first"` is always knowable (the cursor
//! parameters removed), costs one header, and makes the presence of `Link` mean
//! "this endpoint pages" independently of whether *this* response has a
//! successor.
//!
//! ## Why the link, and not a page number
//!
//! Because the admin surface has **two page-index conventions** and a header
//! makes the question moot. `admin/explorer.rs` documents `page`/`page_size` as
//! 1-indexed; `users_admin`, `orgs_admin`, `workspaces_admin` and
//! `admin/billing` are 0-indexed under the same parameter names. A client that
//! has to construct the next request must know which; a client handed the next
//! URL does not.
//!
//! It also carries the FILTERS forward, which is the half that is easy to get
//! wrong. [`page`] rewrites only the parameters it is given and preserves the
//! rest, so the next page of `?search=acme&status=active` is still that search.
//! A link that silently dropped the query would page a different result set and
//! present it as a continuation.
//!
//! ## Knowing whether a next page exists
//!
//! Over-fetch by one: ask for `limit + 1` rows and hand them to
//! [`trim_overfetch`], which trims back to `limit` and reports whether the extra
//! row was there. One row, no `COUNT(*)`, and no window where a count and a page
//! disagree because a row landed between the two queries. `workspaces/ops.rs`
//! already does this for `git log`; this is the same trick with a name.
//!
//! **A caller-supplied page size of zero is what makes that dangerous**, and it
//! is why every adopter clamps to a MINIMUM as well as a maximum. With `limit =
//! 0` the over-fetch reads one row, [`trim_overfetch`] throws it away and still
//! reports "more", and the cursor advances by zero — a `Link` pointing at the
//! URL that produced it, i.e. an endless chain of empty pages for any client
//! that follows links. [`page`] refuses to emit a `rel="next"` that does not
//! advance the cursor, as a net under any future adopter that forgets the
//! clamp; the clamp is still the fix, because a cursor can also advance while
//! the page stays empty (`page + 1` with `page_size = 0`).

use std::collections::HashMap;

use axum::Json;
use axum::http::{HeaderValue, Uri, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
// `url` already re-exports it; a direct dependency would be a second copy.
use url::form_urlencoded;

/// Trim an over-fetched page back to `limit`, reporting whether more exist.
///
/// `rows` must have been fetched with `limit + 1`. Returns true when that extra
/// row was present — i.e. when there is a next page — and removes it, so the
/// caller never has to remember to.
///
/// `limit` must be at least 1. At zero every non-empty result reports "more"
/// while returning nothing, which is a next page that can never be reached.
pub fn trim_overfetch<T>(rows: &mut Vec<T>, limit: u64) -> bool {
    debug_assert!(
        limit > 0,
        "clamp the page size to >= 1 before over-fetching"
    );
    let limit = limit as usize;
    let more = rows.len() > limit;
    rows.truncate(limit);
    more
}

/// One page of a list response: the rows, and where the neighbouring pages are.
///
/// Serializes as a **bare JSON array** — exactly what these handlers returned
/// before — with the links as a header. That is what makes adopting it a
/// non-event for existing callers.
pub struct Paged<T> {
    items: Vec<T>,
    link: Option<HeaderValue>,
}

/// One page, with `rel="first"` always and `rel="next"` when `more`.
///
/// `params` are the query parameters that identify a page — `[("page", "3")]` or
/// `[("offset", "150")]`, holding the value the NEXT page needs. Everything else
/// in `uri`'s query is carried over untouched, and `rel="first"` is the same URL
/// with those parameters removed (absent means the first page in every adopter).
pub fn page<T>(items: Vec<T>, more: bool, uri: &Uri, params: &[(&str, String)]) -> Paged<T> {
    let mut links = Vec::with_capacity(2);
    // A `rel="next"` that does not move the cursor is a link to the response
    // holding it. Following it is an infinite loop, so it is never emitted —
    // see the module docs on a zero page size.
    if more && advances(uri, params) {
        links.push(format!("{}; rel=\"next\"", reference(uri, params, &[])));
    }
    let cursor_keys: Vec<&str> = params.iter().map(|(k, _)| *k).collect();
    links.push(format!(
        "{}; rel=\"first\"",
        reference(uri, &[], &cursor_keys)
    ));

    Paged {
        items,
        link: HeaderValue::from_str(&links.join(", ")).ok(),
    }
}

/// True when at least one of `params` names a value the request did not already
/// carry. An absent parameter counts as different: it may well default to the
/// value being set, but that default belongs to the handler and is not knowable
/// here, and erring towards emitting the link keeps this a net rather than a
/// second place that decides whether a page exists.
fn advances(uri: &Uri, params: &[(&str, String)]) -> bool {
    let current: HashMap<String, String> = query_pairs(uri).into_iter().collect();
    params
        .iter()
        .any(|(key, value)| current.get(*key) != Some(value))
}

/// `<path?query>`, with `set` overridden and `remove`d keys dropped.
///
/// A RELATIVE reference, deliberately. The absolute URL would have to be
/// reconstructed from `Host` and `X-Forwarded-Proto`, which behind Oxy's proxy
/// and the org/custom-app subdomain dispatch is a way to emit a link pointing at
/// the wrong host. A relative one resolves against whatever the client already
/// used to reach us, which is by construction correct. `oxyc` handles both
/// (`next.startsWith("http")`).
fn reference(uri: &Uri, set: &[(&str, String)], remove: &[&str]) -> String {
    let mut pairs = query_pairs(uri);
    pairs.retain(|(k, _)| !remove.contains(&k.as_str()));

    for (key, value) in set {
        // Replace in place so the link keeps the caller's parameter order, and
        // push only when the parameter was absent — a `page` appearing twice
        // would be resolved by serde taking the FIRST, i.e. the old one.
        match pairs.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value.clone(),
            None => pairs.push(((*key).to_string(), value.clone())),
        }
    }

    if pairs.is_empty() {
        return format!("<{}>", uri.path());
    }
    let query = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    format!("<{}?{}>", uri.path(), query)
}

fn query_pairs(uri: &Uri) -> Vec<(String, String)> {
    uri.query()
        .map(|q| {
            form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

impl<T: Serialize> IntoResponse for Paged<T> {
    fn into_response(self) -> Response {
        let mut response = Json(self.items).into_response();
        if let Some(link) = self.link {
            response.headers_mut().insert(header::LINK, link);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_of<T: Serialize>(paged: &Paged<T>) -> &str {
        paged.link.as_ref().unwrap().to_str().unwrap()
    }

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }

    #[test]
    fn over_fetching_by_one_is_how_more_is_known() {
        let mut rows = vec![1, 2, 3, 4];
        assert!(
            trim_overfetch(&mut rows, 3),
            "the 4th row means a next page"
        );
        assert_eq!(rows, vec![1, 2, 3], "and it must not be served");

        let mut exact = vec![1, 2, 3];
        assert!(!trim_overfetch(&mut exact, 3), "a full page is not more");
        assert_eq!(exact, vec![1, 2, 3]);

        let mut short = vec![1];
        assert!(!trim_overfetch(&mut short, 3));
    }

    /// THE ONE THAT MATTERS. A next link that drops the filters pages a
    /// different query and presents it as a continuation — the caller sees
    /// unrelated rows appended to their search results and has no way to tell.
    #[test]
    fn the_next_link_carries_every_other_parameter() {
        let uri = uri("/api/admin/users?search=acme&status=active&page=0");
        let link = link_of(&page(vec![1], true, &uri, &[("page", "1".to_string())])).to_string();
        assert!(link.contains("search=acme"), "{link}");
        assert!(link.contains("status=active"), "{link}");
        assert!(link.contains("page=1"), "{link}");
        assert!(
            !link.contains("page=0>"),
            "the old page must be replaced: {link}"
        );
        assert!(link.contains("rel=\"next\""), "{link}");
    }

    /// A parameter the caller never sent still has to appear, or the second page
    /// repeats the first: an absent `?offset` means offset 0.
    #[test]
    fn a_parameter_the_caller_omitted_is_added() {
        let paged = page(
            vec![1],
            true,
            &uri("/api/admin/audit"),
            &[("offset", "100".into())],
        );
        assert_eq!(
            link_of(&paged),
            "</api/admin/audit?offset=100>; rel=\"next\", </api/admin/audit>; rel=\"first\""
        );
    }

    /// THE INFINITE LOOP THIS FORBIDS.
    ///
    /// `?limit=0` clamped only at the top survives as a page size of zero: the
    /// over-fetch reads one row, `trim_overfetch` discards it and still reports
    /// "more", and the cursor advances by zero — so the `rel="next"` points at
    /// the very request that produced it. A link-following client walks empty
    /// pages forever. The adopters clamp to >= 1 so this cannot arise; this is
    /// the net under the next one that forgets.
    #[test]
    fn a_next_link_that_does_not_advance_is_never_emitted() {
        let paged = page(
            vec![1],
            true,
            &uri("/api/admin/audit?limit=0&offset=0"),
            &[("offset", "0".to_string())],
        );
        let link = link_of(&paged);
        assert!(
            !link.contains("rel=\"next\""),
            "would loop on itself: {link}"
        );
        assert!(link.contains("rel=\"first\""), "{link}");
    }

    /// A response that carries NO `Link` is indistinguishable from an endpoint
    /// that never heard of pagination — which is how the client's "no
    /// pagination signal" warning came to fire on exactly the endpoints this
    /// module fixes. `rel="first"` is always emitted so the header's presence
    /// alone answers "does this endpoint page?".
    #[test]
    fn the_last_page_still_says_that_it_pages() {
        let paged = page(
            vec![1],
            false,
            &uri("/api/admin/users?search=acme&page=3"),
            &[("page", "4".to_string())],
        );
        let link = link_of(&paged);
        assert!(
            !link.contains("rel=\"next\""),
            "there is no next page: {link}"
        );
        assert!(link.contains("rel=\"first\""), "{link}");
        // `first` is the same query with the cursor dropped — filters kept.
        assert!(link.contains("search=acme"), "{link}");
        assert!(!link.contains("page="), "the cursor must be gone: {link}");
    }

    /// A value that would break the header — or the query — must be encoded,
    /// not concatenated. `HeaderValue::from_str` rejects control characters, so
    /// the alternative to encoding is a page with no link at all. A raw comma
    /// would also split the header into two links for any RFC 8288 reader.
    #[test]
    fn values_are_url_encoded() {
        let uri = uri("/api/admin/users?search=a%20b%2Cc");
        let link = link_of(&page(vec![1], true, &uri, &[("page", "1".to_string())])).to_string();
        assert!(link.contains("search=a+b%2Cc"), "{link}");
    }
}
