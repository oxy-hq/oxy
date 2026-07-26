//! Turning what an operator typed into an org id, for `oxy assume`.
//!
//! `--org` accepts a slug, a UUID, or a URL, and the URL case is the whole
//! point of the feature: the address bar is what you actually have in front of
//! you. A UUID needs no lookup and a URL carries its slug in the hostname; only
//! a bare slug has to be resolved against the deployment.
//!
//! Slug resolution walks the surfaces the caller *might* hold rather than
//! assuming which population they belong to — staff read the admin directory,
//! partners read their client list, and a plain member reads their own orgs.
//! Each is best-effort: a 403 from one is not an error, it just means "not this
//! one".

use oxy_shared::errors::OxyError;
use serde_json::Value;
use uuid::Uuid;

use super::assume::Connection;
use super::env_url;

/// GET a JSON array, or `None` when the caller can't reach that surface.
pub(super) async fn get_rows(
    conn: &Connection,
    path: &str,
    query: &[(&str, &str)],
) -> Option<Vec<Value>> {
    let resp = conn
        .client
        .get(format!("{}{path}", conn.target))
        .query(query)
        .bearer_auth(&conn.token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    match resp.json::<Value>().await.ok()? {
        Value::Array(rows) => Some(rows),
        _ => None,
    }
}

/// The id of the row whose `slug` matches. Orgs are serialized as `id` in some
/// DTOs and `org_id` in others, so both are accepted.
fn id_for_slug(rows: &[Value], slug: &str) -> Option<Uuid> {
    rows.iter()
        .find(|r| r.get("slug").and_then(Value::as_str) == Some(slug))
        .and_then(|r| {
            r.get("id")
                .or_else(|| r.get("org_id"))
                .and_then(Value::as_str)
        })
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// A partner's assigned clients, across every partner they operate. One
/// unreadable partner doesn't abort the search — the next one may hold the org.
async fn partner_client_id(conn: &Connection, slug: &str) -> Option<Uuid> {
    let partners = get_rows(conn, "/api/partners", &[]).await?;
    for p in partners {
        let Some(partner_id) = p.get("partner_id").and_then(Value::as_str) else {
            continue;
        };
        let path = format!("/api/partners/{partner_id}/orgs");
        let Some(rows) = get_rows(conn, &path, &[]).await else {
            continue;
        };
        if let Some(id) = id_for_slug(&rows, slug) {
            return Some(id);
        }
    }
    None
}

/// Slug → org id. Tries the surfaces the caller might actually hold, cheapest
/// first: their own orgs, then the staff directory, then a partner's clients.
async fn org_id_for_slug(conn: &Connection, slug: &str) -> Result<Uuid, OxyError> {
    if let Some(rows) = get_rows(conn, "/api/orgs", &[]).await
        && let Some(id) = id_for_slug(&rows, slug)
    {
        return Ok(id);
    }
    // `search` is user input — pass it as a query parameter so reqwest escapes
    // it, rather than pasting it into the path.
    let admin_query = [("search", slug), ("page_size", "200")];
    if let Some(rows) = get_rows(conn, "/api/admin/orgs-meta", &admin_query).await
        && let Some(id) = id_for_slug(&rows, slug)
    {
        return Ok(id);
    }
    if let Some(id) = partner_client_id(conn, slug).await {
        return Ok(id);
    }
    Err(OxyError::RuntimeError(format!(
        "no organization with slug '{slug}' is visible to you on {}. Check the slug, or pass the org UUID directly with --org.",
        conn.target
    )))
}

/// Read `--org` (or the org an `--env` URL named) as an org id.
pub(super) async fn resolve_org(conn: &Connection, org: Option<&str>) -> Result<Uuid, OxyError> {
    let raw = org
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            OxyError::ConfigurationError(
                "no organization given. Pass --org <slug|uuid|url>, or point --env at an org URL (e.g. --env https://poke-house.oxygen-hq.com)."
                    .into(),
            )
        })?;

    if let Ok(id) = Uuid::parse_str(raw) {
        return Ok(id);
    }
    let slug = if env_url::looks_like_url(raw) {
        env_url::parse_env_url(raw)
            .and_then(|r| r.org_slug)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!(
                    "could not read an org from '{raw}'. Org URLs look like https://<org-slug>.oxygen-hq.com; otherwise pass the slug or UUID."
                ))
            })?
    } else {
        raw.to_string()
    };
    org_id_for_slug(conn, &slug).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn id_for_slug_reads_both_dto_shapes() {
        let id = Uuid::new_v4();
        // `/orgs` and `/admin/orgs-meta` use `id`; partner `ChildOrg` uses `org_id`.
        let rows = vec![
            json!({ "slug": "other", "id": Uuid::new_v4().to_string() }),
            json!({ "slug": "poke-house", "id": id.to_string() }),
        ];
        assert_eq!(id_for_slug(&rows, "poke-house"), Some(id));

        let rows = vec![json!({ "slug": "poke-house", "org_id": id.to_string() })];
        assert_eq!(id_for_slug(&rows, "poke-house"), Some(id));

        assert_eq!(id_for_slug(&rows, "absent"), None);
    }

    #[test]
    fn a_url_org_is_read_without_any_lookup() {
        // The URL forms carry their slug, so these never need the network —
        // pinned here because it's what makes `--org <url>` usable offline of
        // the admin directory.
        for (url, want) in [
            ("https://poke-house.oxygen-hq.com", "poke-house"),
            (
                "https://poke-house--bookkeeping.customer-apps.oxygen-hq.com",
                "poke-house",
            ),
            ("https://poke-house.oxygen-hq.com/apps/sales", "poke-house"),
        ] {
            let slug = env_url::parse_env_url(url).and_then(|r| r.org_slug);
            assert_eq!(slug.as_deref(), Some(want), "{url}");
        }
        // A product URL names no org — the caller must say which.
        assert_eq!(
            env_url::parse_env_url("https://app.oxygen-hq.com").and_then(|r| r.org_slug),
            None
        );
    }
}
