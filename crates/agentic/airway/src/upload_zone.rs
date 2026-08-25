//! Where an uploadable source's reports live, derived rather than declared.
//!
//! One landing zone serves every uploadable source kind — see
//! [`UPLOADABLE_SOURCE_KINDS`], which is `ubereats` today.
//!
//! **The bucket is server configuration, never the pipeline's.** A pipeline's
//! `base_path` is customer-editable and the pod role reaches every
//! `<cluster>-*` bucket, so trusting it would let a customer name
//! `s3://oxy-dev-customer-apps/…` and have the server write their bytes there
//! with its own credentials — a confused deputy. The bucket and root prefix
//! come from [`ZONE_VAR`]; the pipeline only ever contributes a *segment*
//! below it.
//!
//! # Why this lives in the airway crate
//!
//! Three callers need the identical answer and none of them can see each
//! other: the upload endpoint (`oxy-app`) writing a report, the task executor
//! (`agentic-pipeline`) filling in an omitted `base_path` before a run, and
//! the source itself reading the zone. `oxy-app` and `agentic-pipeline` both
//! depend on this crate and neither depends on the other, so this is the only
//! place all three can share one derivation.
//!
//! Sharing it is the point, not tidiness. The zone a report is WRITTEN to and
//! the zone a run READS from are the same string or the report is invisible —
//! silently, since both halves look fine alone. Two copies of this arithmetic
//! would be free to drift; one cannot.

//! # `OXY_SOURCE_UPLOAD_ZONE` must be IDENTICAL on every role
//!
//! Not merely present on the API one. The upload endpoint reads it on the API
//! role; [`derive_base_path`] reads it again on the **durable worker fleet**,
//! where airway runs execute. Two failure shapes follow, and only one is loud:
//!
//! - **Unset on the worker** — uploads succeed, every run of a pipeline that
//!   omits `base_path` fails with `omits base_path and one cannot be derived`.
//!   Loud, and says what to do.
//! - **Set to a DIFFERENT value on the worker** — nothing fails. Reports land
//!   in the API's zone, the run reads the worker's, and the run finishes empty
//!   and green. The equality check in the upload endpoint cannot catch this:
//!   it compares two values computed inside the same API process, so a
//!   cross-role divergence is invisible to it.
//!
//! That second one is the drift this module exists to remove, relocated from
//! operator-vs-server to role-vs-role. There is no code that can catch it —
//! it is a deployment invariant, so it is written down here and belongs in the
//! deploy manifest beside the variable.

use uuid::Uuid;

/// Source kinds whose reports arrive by upload rather than over an API.
///
/// Adding a kind here is one entry rather than a second bucket, a second env
/// var and a second route — the kind is a path segment, not a separate zone.
pub const UPLOADABLE_SOURCE_KINDS: &[&str] = &["ubereats"];

/// The landing zone, as a URL — `s3://bucket/prefix`.
pub const ZONE_VAR: &str = "OXY_SOURCE_UPLOAD_ZONE";

/// Why a zone could not be resolved.
///
/// Deliberately free of any HTTP type: this crate is a domain crate, and the
/// upload endpoint is only one of the callers. `oxy-app` maps these onto
/// status codes; the executor renders them into a run error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ZoneError {
    /// No zone configured. Uploads are off, deliberately — no zone, no uploads.
    #[error("report uploads require {ZONE_VAR} (e.g. `s3://oxy-dev-source-uploads`)")]
    NotConfigured,
    /// Configured, but not an `s3://` URL.
    #[error("{ZONE_VAR} must be an `s3://bucket/prefix` URL, got `{0}`")]
    NotS3Url(String),
    /// An `s3://` URL naming no bucket.
    #[error("{ZONE_VAR} names no bucket")]
    NoBucket,
    /// The pipeline path reduces to nothing usable as a key segment.
    ///
    /// Rejected rather than sanitized: the segment is part of the object key,
    /// and the key is the merge identity, so a silently-rewritten one lands
    /// rows under an id the caller does not expect.
    #[error("`{0}` has no usable path segment for an upload zone")]
    UnusablePipelineRef(String),
}

/// Whether reports for this source kind arrive by upload.
pub fn is_uploadable(source_kind: &str) -> bool {
    UPLOADABLE_SOURCE_KINDS.contains(&source_kind)
}

/// The configured zone split into `(bucket, root_prefix)`.
pub fn zone_from_env() -> Result<(String, String), ZoneError> {
    let raw = std::env::var(ZONE_VAR)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or(ZoneError::NotConfigured)?;
    parse_zone(&raw)
}

/// The pure half of [`zone_from_env`], split out so it is testable **without
/// mutating process environment** — `setenv` racing another thread's
/// `std::env::var` is a data race, and this crate reads env on several paths.
pub fn parse_zone(raw: &str) -> Result<(String, String), ZoneError> {
    let rest = raw.trim().trim_end_matches('/');
    let rest = rest
        .strip_prefix("s3://")
        .ok_or_else(|| ZoneError::NotS3Url(rest.to_string()))?;
    let (bucket, prefix) = match rest.split_once('/') {
        Some((b, p)) => (b, p),
        None => (rest, ""),
    };
    if bucket.is_empty() {
        return Err(ZoneError::NoBucket);
    }
    Ok((bucket.to_string(), prefix.to_string()))
}

/// A pipeline ref reduced to one safe path segment.
///
/// `pipelines/ubereats.airway.yml` → `pipelines__ubereats`: every component
/// participates, joined with `__`. The directory is in there because
/// `east/ue.airway.yml` and `west/ue.airway.yml` otherwise reduce to one
/// segment, one base path and one key prefix — and `<kind>` does not separate
/// them either, since both are `ubereats`. Since the object name is a content
/// hash, the same report for the same period would land on the same key for
/// two different pipelines and merge into the wrong table.
pub fn pipeline_slug(pipeline_ref: &str) -> Option<String> {
    let path = std::path::Path::new(pipeline_ref);
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())?
        .trim_end_matches(".yml")
        .trim_end_matches(".airway");

    // `..` and absolutes are REFUSED, not dropped. Dropping them collapses
    // `../ue.airway.yml` onto `ue.airway.yml` — two different files, one slug,
    // one key prefix, and since the object name is a content hash, one merge
    // identity. That is the very collision `__` was chosen to prevent.
    //
    // Both of today's callers reject these before calling (the handler's
    // `validate_pipeline_ref` and `agentic_pipeline`'s), so this is not
    // reachable now. It is checked here anyway because the helper moved into a
    // shared crate on the premise that a THIRD caller will use it, and a
    // comment asserting what unseen callers guarantee is not a guarantee.
    //
    // `.` is different and is still dropped: `./a.yml` and `a.yml` are the
    // SAME file, so collapsing them is correct rather than a collision.
    if path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return None;
    }

    let mut parts: Vec<&str> = path
        .parent()
        .into_iter()
        .flat_map(|p| p.components())
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => n.to_str(),
            _ => None,
        })
        .collect();
    parts.push(stem);

    let safe = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    };
    if !parts.iter().all(|p| safe(p)) {
        return None;
    }

    // `__` as the separator: `/` would open a key segment the caller controls,
    // and a single `_` is legal inside a component, so `a_b/c` and `a/b_c`
    // would collide again.
    let slug = parts.join("__");
    (slug.len() <= 128).then_some(slug)
}

/// The zone a pipeline's reports live under — everything above the period.
///
/// This is what a declared `base_path` must equal, and what an omitted one is
/// filled in with.
pub fn pipeline_base_path(
    bucket: &str,
    root: &str,
    workspace_id: Uuid,
    source_kind: &str,
    pipeline_slug: &str,
) -> String {
    let tail = format!("{workspace_id}/{source_kind}/{pipeline_slug}");
    if root.is_empty() {
        format!("s3://{bucket}/{tail}")
    } else {
        format!("s3://{bucket}/{root}/{tail}")
    }
}

/// The whole derivation, from the environment to the string a pipeline reads.
///
/// The one call every consumer should reach for. Composing the three steps by
/// hand at each call site is exactly the drift this module exists to prevent.
pub fn derive_base_path(
    workspace_id: Uuid,
    source_kind: &str,
    pipeline_ref: &str,
) -> Result<String, ZoneError> {
    let (bucket, root) = zone_from_env()?;
    let slug = pipeline_slug(pipeline_ref)
        .ok_or_else(|| ZoneError::UnusablePipelineRef(pipeline_ref.to_string()))?;
    Ok(pipeline_base_path(
        &bucket,
        &root,
        workspace_id,
        source_kind,
        &slug,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WS: Uuid = Uuid::from_u128(0x7067a766_a618_4aad_9104_46b24f35a47a);

    #[test]
    fn a_zone_without_a_prefix_puts_the_workspace_directly_under_the_bucket() {
        let (bucket, root) = parse_zone("s3://oxy-dev-source-uploads").expect("parses");
        assert_eq!(bucket, "oxy-dev-source-uploads");
        assert_eq!(root, "");
        assert_eq!(
            pipeline_base_path(&bucket, &root, WS, "ubereats", "pipelines__ubereats"),
            "s3://oxy-dev-source-uploads/7067a766-a618-4aad-9104-46b24f35a47a/ubereats/pipelines__ubereats"
        );
    }

    #[test]
    fn a_zone_with_a_prefix_keeps_it_above_the_workspace() {
        let (bucket, root) = parse_zone("s3://bucket/reports/").expect("parses");
        assert_eq!((bucket.as_str(), root.as_str()), ("bucket", "reports"));
        assert_eq!(
            pipeline_base_path(&bucket, &root, WS, "ubereats", "p__ue"),
            "s3://bucket/reports/7067a766-a618-4aad-9104-46b24f35a47a/ubereats/p__ue"
        );
    }

    #[test]
    fn a_zone_must_be_an_s3_url() {
        assert_eq!(
            parse_zone("gs://nope"),
            Err(ZoneError::NotS3Url("gs://nope".into()))
        );
        // `s3://` alone reads as a MALFORMED URL, not a bucket-less one: the
        // trailing-slash trim runs before the prefix check, so the string the
        // check sees is `s3:`. Behaviour preserved from the parser this was
        // lifted out of rather than "corrected" — both answers refuse, and
        // changing which one a deployment sees is not this change's business.
        assert_eq!(parse_zone("s3://"), Err(ZoneError::NotS3Url("s3:".into())));
        // The bucket-less shape that does reach `NoBucket`.
        assert_eq!(parse_zone("s3:///prefix"), Err(ZoneError::NoBucket));
    }

    /// Every component participates, so two pipelines of the same name in
    /// different directories cannot share a zone.
    #[test]
    fn the_slug_carries_the_whole_path() {
        assert_eq!(
            pipeline_slug("pipelines/ubereats.airway.yml").as_deref(),
            Some("pipelines__ubereats")
        );
        assert_ne!(
            pipeline_slug("east/ue.airway.yml"),
            pipeline_slug("west/ue.airway.yml")
        );
    }

    /// `__` rather than `_`, so a `_` legal inside a component cannot collide
    /// two different paths onto one slug.
    #[test]
    fn the_separator_cannot_be_forged_from_a_component() {
        assert_ne!(
            pipeline_slug("a_b/c.airway.yml"),
            pipeline_slug("a/b_c.airway.yml")
        );
    }

    #[test]
    fn a_ref_with_no_usable_segment_is_refused_rather_than_sanitized() {
        assert_eq!(pipeline_slug("pipe lines/ue.airway.yml"), None);
    }

    /// Traversal is refused, because DROPPING it collides two different files
    /// onto one merge identity — the opposite of what the separator is for.
    #[test]
    fn traversal_is_refused_not_dropped() {
        assert_eq!(pipeline_slug("../ue.airway.yml"), None);
        assert_eq!(pipeline_slug("a/../b/ue.airway.yml"), None);
        assert_eq!(pipeline_slug("/abs/ue.airway.yml"), None);
        // `.` still collapses: `./a` and `a` are the same file, not two.
        assert_eq!(
            pipeline_slug("./pipelines/ue.airway.yml"),
            pipeline_slug("pipelines/ue.airway.yml")
        );
    }

    #[test]
    fn only_declared_kinds_are_uploadable() {
        assert!(is_uploadable("ubereats"));
        assert!(!is_uploadable("toast"));
        assert!(!is_uploadable(""));
    }
}
