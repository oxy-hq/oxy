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
//!
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
    zone_from_raw(std::env::var(ZONE_VAR).ok())
}

/// The pure half of [`zone_from_env`], and the ONE place the line between
/// "unset" and "set but useless" is drawn.
///
/// Whitespace-only counts as UNSET, deliberately. `OXY_SOURCE_UPLOAD_ZONE: ""`
/// in a manifest is how most deploy systems spell "not filled in yet", so the
/// operator's next step really is to go set it — which is what
/// [`ZoneError::NotConfigured`] tells them. A value like `"/"` is different:
/// somebody typed it meaning something, so it is malformed rather than absent
/// and [`parse_zone`] refuses it as such.
///
/// Split out so BOTH halves are testable without touching process env —
/// `setenv` racing another thread's read is a data race, and after `parse_zone`
/// stopped producing `NotConfigured` this filter became its only producer,
/// leaving the variant unreachable from any test.
fn zone_from_raw(raw: Option<String>) -> Result<(String, String), ZoneError> {
    let raw = raw
        .filter(|v| !v.trim().is_empty())
        .ok_or(ZoneError::NotConfigured)?;
    parse_zone(&raw)
}

/// A zone URL split into `(bucket, root_prefix)`.
///
/// Takes the string rather than reading env, so it is testable **without
/// mutating process environment** — `setenv` racing another thread's read is a
/// data race, and this crate reads env on several paths.
///
/// Everything reaching here is a value somebody wrote: [`zone_from_raw`] has
/// already turned unset and whitespace-only into [`ZoneError::NotConfigured`].
///
/// Private is what makes that a guarantee rather than a convention, and
/// private rather than `pub(crate)` because the claim is about this MODULE: no
/// caller outside it exists. As `pub` an external one could hand it `""`
/// directly and get `NotS3Url("")` — "got ``", which tells an operator
/// nothing, out of the one door built to keep those two diagnoses apart.
fn parse_zone(raw: &str) -> Result<(String, String), ZoneError> {
    // Through the shared helper, not a fourth hand-rolled copy: this is the
    // same arithmetic, and it carried the same missing re-trim, so
    // `OXY_SOURCE_UPLOAD_ZONE="s3://bkt /"` yielded a bucket with a trailing
    // space in every derived `base_path` on every role. Lower risk than the
    // declared-value case — one env var, so both halves compute the same wrong
    // bucket and it fails at S3 rather than landing rows nowhere — but it is
    // the same expression and the header's argument applies unchanged.
    // `NotS3Url`, NOT `NotConfigured`: a zone of `"/"` or `"///"` IS set, and
    // "requires OXY_SOURCE_UPLOAD_ZONE" sends an operator to grep a manifest
    // where they will find it present and be stuck. The header sells the unset
    // case as the failure that says what to do; handing that message to a
    // different fault spends it. Blank and whitespace never reach here —
    // `zone_from_env` filters them — so every input that does is a value
    // someone actually wrote.
    let rest =
        normalize_base_path(raw).ok_or_else(|| ZoneError::NotS3Url(raw.trim().to_string()))?;
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

/// A declared `base_path`, normalized — or `None` when it names nowhere.
///
/// The one spelling of this arithmetic. It existed in two crates verbatim
/// (`.trim().trim_end_matches('/')`), which is exactly what the header of this
/// module argues against for the derivation: two copies are free to drift, and
/// the drift is silent because both halves look correct alone. Re-typing the
/// second copy fixed the instance and left the next edit free to re-open it.
///
/// One predicate over slashes and whitespace, so every interleaving collapses
/// in a single pass and the function is idempotent. The comment on the
/// implementation carries what that replaced and why it is trailing-only.
///
/// `None` means "names nowhere" — empty, whitespace, or nothing but slashes.
///
/// The two kinds of caller read `None` differently, deliberately. A DECLARED
/// `base_path` reads it as ABSENT and derives: a pipeline whose zone is blank
/// has not disagreed with anything, and refusing it produced a message claiming
/// it "reads from ``" when at run time it reads the derived zone.
/// `parse_zone` instead REFUSES, because its input is a server-set env var,
/// where a value naming nowhere is a misconfiguration rather than a default.
pub fn normalize_base_path(declared: &str) -> Option<&str> {
    // One predicate, so every interleaving collapses in a single pass and the
    // function is IDEMPOTENT. `.trim().trim_end_matches('/').trim()` was not:
    // `"s3://z/p/ /"` came back as `"s3://z/p/"`, still carrying the slash this
    // exists to remove, so `normalize(normalize(x)) != normalize(x)`.
    //
    // Trailing only. A leading slash is content — `/tmp/ue` is a legal zone
    // (`base_path` is not required to be `s3://`), and `trim_matches` would
    // quietly turn it into the relative `tmp/ue`.
    let normalized = declared
        .trim()
        .trim_end_matches(|c: char| c == '/' || c.is_whitespace());
    (!normalized.is_empty()).then_some(normalized)
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

    /// `normalize(normalize(x)) == normalize(x)` for every interleaving of
    /// slashes and whitespace — the property the two-step form lacked.
    #[test]
    fn normalization_is_idempotent() {
        for raw in [
            "s3://z/p",
            "  s3://z/p  ",
            "s3://z/p/ /",
            "s3://z/p/ / / ",
            " /tmp/ue/ ",
            "s3://z/p///",
        ] {
            let once = normalize_base_path(raw).expect("names somewhere");
            assert_eq!(
                normalize_base_path(once),
                Some(once),
                "`{raw}` -> `{once}` is not a fixed point"
            );
        }
    }

    /// A LEADING slash is content, not padding: `base_path` may be a local
    /// path, and stripping it would turn an absolute zone into a relative one.
    #[test]
    fn a_leading_slash_survives_normalization() {
        assert_eq!(normalize_base_path("/tmp/ue/"), Some("/tmp/ue"));
        assert_eq!(normalize_base_path("  /tmp/ue  "), Some("/tmp/ue"));
    }

    /// The zone parser shares the helper, so a stray space in the env var
    /// cannot put one inside the bucket name.
    #[test]
    fn a_padded_zone_var_does_not_pad_the_bucket() {
        let (bucket, root) = parse_zone("s3://oxy-dev-ue /").expect("parses");
        assert_eq!(bucket, "oxy-dev-ue", "a trailing space would reach S3");
        assert_eq!(root, "");
    }

    /// Unset, and the spellings of unset a manifest actually produces.
    ///
    /// Reachable only since `zone_from_raw` was split out: `parse_zone` stopped
    /// producing this variant, so nothing could pin the one case the change was
    /// made to preserve.
    #[test]
    fn an_unset_or_placeholder_zone_is_not_configured() {
        for unset in [None, Some(String::new()), Some("   ".to_string())] {
            assert_eq!(
                zone_from_raw(unset.clone()),
                Err(ZoneError::NotConfigured),
                "`{unset:?}` is how a manifest spells 'not filled in yet'"
            );
        }
        // A real value still parses through the same door.
        assert_eq!(
            zone_from_raw(Some("s3://bkt/pre".into())),
            Ok(("bkt".to_string(), "pre".to_string()))
        );
    }

    /// A zone that IS set but names nowhere must not claim it is unset.
    ///
    /// That distinction is the whole value of the unset message: "requires
    /// OXY_SOURCE_UPLOAD_ZONE" tells an operator to go set it, and for `"/"`
    /// they would find it already set and have nowhere to go.
    #[test]
    fn a_set_zone_that_names_nowhere_is_malformed_not_absent() {
        // Through the REAL door first. Both halves of this invariant were
        // pinned and the seam between them was not: `zone_from_raw` spells
        // "names nowhere" as `!v.trim().is_empty()`, and `"/"` reaches
        // `parse_zone` only because that spelling lets it through. Tightening
        // the filter to `normalize_base_path` — the unification this module
        // has been doing everywhere else, so the obvious next edit — sends
        // `"/"` back to `NotConfigured` and the operator back to the dead end,
        // with every other assertion here still passing.
        assert_eq!(
            zone_from_raw(Some(" / ".to_string())),
            Err(ZoneError::NotS3Url("/".to_string())),
            "a set-but-useless env var must reach parse_zone and be named"
        );

        for set_but_useless in ["/", "///", " / "] {
            match parse_zone(set_but_useless) {
                // The payload, not just the variant: a refactor handing this
                // `NotS3Url("")` renders "got ``", which tells an operator
                // nothing, and a variant-only assertion would still pass.
                Err(ZoneError::NotS3Url(got)) => {
                    assert!(!got.is_empty(), "`{set_but_useless}` must name what it saw")
                }
                other => panic!("`{set_but_useless}` is set, so it is malformed: {other:?}"),
            }
        }
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

    /// The trailing space is the reason this helper exists: it reaches the
    /// connector inside the object key. One predicate over both character
    /// classes closes every interleaving, which a staged trim-then-strip did
    /// not — see [`normalization_is_idempotent`] for the property itself.
    #[test]
    fn normalization_collapses_slashes_and_whitespace_in_one_pass() {
        assert_eq!(normalize_base_path("s3://z/p"), Some("s3://z/p"));
        assert_eq!(normalize_base_path("  s3://z/p  "), Some("s3://z/p"));
        assert_eq!(normalize_base_path("s3://z/p/"), Some("s3://z/p"));
        assert_eq!(normalize_base_path("s3://z/p///"), Some("s3://z/p"));
        // The case a single leading trim gets wrong.
        assert_eq!(normalize_base_path(" s3://z/p /"), Some("s3://z/p"));
        // A local path is a legal zone too, not only `s3://`.
        assert_eq!(normalize_base_path("/tmp/ue/"), Some("/tmp/ue"));
    }

    /// Blank means "names nowhere", which callers read as ABSENT and derive —
    /// not as a disagreement to refuse.
    #[test]
    fn a_zone_that_names_nowhere_is_none() {
        for blank in ["", "   ", "/", "///", "  //  "] {
            assert_eq!(normalize_base_path(blank), None, "`{blank}` names nowhere");
        }
    }

    #[test]
    fn only_declared_kinds_are_uploadable() {
        assert!(is_uploadable("ubereats"));
        assert!(!is_uploadable("toast"));
        assert!(!is_uploadable(""));
    }
}
