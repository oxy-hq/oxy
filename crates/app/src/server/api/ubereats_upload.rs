//! Upload endpoint for UberEats "Payment details" reports.
//!
//! Writes a report into the S3 landing zone that airway's `ubereats` source
//! reads. The two halves are a **contract**, not two independent settings:
//!
//! - the zone this writes to (`OXY_UBEREATS_LANDING_ZONE`) and the pipeline's
//!   `base_path` must name the same place, or reports land where nothing reads
//!   them;
//! - the key layout below is what the source derives the report **period** and
//!   the row identity from, so it cannot be changed freely — see [`object_key`].
//!
//! # Why the bytes come through oxy
//!
//! The obvious alternative is a presigned PUT, the way the custom-app asset
//! store works. It is rejected here on purpose: presigning means the server
//! never sees the content, so a report whose header variant renamed a
//! JE-critical column is accepted, sits in the zone, and fails at **load**
//! time — hours later, in a pipeline run, as a "missing JE-critical column"
//! error nobody is watching for. Monthly payment-details reports are small, so
//! the cost of streaming them is a rounding error against catching that at the
//! moment someone can still fix it.
//!
//! # Why validation runs the real source
//!
//! Everything needed to validate — the 49-column map, the JE-critical set,
//! header detection, period derivation — is `pub(crate)` inside airway. Copying
//! any of it here would put two copies of a contract in two repos on two
//! release cadences, which is the drift this codebase has been bitten by
//! repeatedly. `agentic_airway::report_validation` instead runs the real source
//! over the bytes: validation *by execution* of the shipped code path, so it
//! cannot disagree with what the loader will do.
//!
//! It lives in `agentic-airway` rather than here because that crate owns oxy's
//! dependency on the engine — this handler is transport, and linking `airway`
//! into the app crate would widen that coupling for no gain.

use agentic_airway::report_validation::{
    ReportValidationError, check_period, validate_ubereats_report,
};
use axum::extract::{Multipart, Path};
use axum::http::StatusCode;
use serde::Serialize;
use uuid::Uuid;

use crate::server::api::middlewares::role_guards::WorkspaceEditor;

/// The landing zone, as a URL — `s3://bucket/prefix`.
///
/// Deliberately the same shape as the pipeline's `base_path` so the two can be
/// set to the identical string and the contract between them is visible rather
/// than reconstructed from a bucket plus a prefix.
const ZONE_VAR: &str = "OXY_UBEREATS_LANDING_ZONE";

/// Body ceiling for an upload.
///
/// Enforced by a `DefaultBodyLimit` layer on the route's `Router`, NOT by the
/// check in the handler: axum applies its own 2 MiB default otherwise, so a
/// 3 MiB report fails inside `field.bytes()` as a `400 length limit exceeded`
/// and the handler's check is never reached. The in-handler check remains as a
/// backstop for a caller that streams past the layer, and it runs *after* the
/// part is buffered — it bounds what is stored, not what is read.
///
/// A monthly payment-details report for one store is well under a megabyte;
/// 64 MiB is far above any real one and still bounded, which matters because
/// the body is buffered to validate it.
pub const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct UploadedReport {
    /// Where it landed, as a URL — the same spelling the pipeline's `base_path`
    /// uses, so an operator can see at a glance whether they match.
    pub location: String,
    /// The period the loader will stamp on every row, derived here rather than
    /// reported back from the loader, so a mismatch surfaces now.
    pub report_year: i64,
    pub report_month: u32,
    /// Rows the report yielded under validation. Zero is a successful upload of
    /// an empty report, which is worth seeing.
    pub rows: usize,
}

/// The zone URL split into `(bucket, prefix)`.
fn zone() -> Result<(String, String), (StatusCode, String)> {
    let raw = std::env::var(ZONE_VAR)
        .ok()
        .filter(|v| !v.trim().is_empty());
    let Some(raw) = raw else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "report uploads require {ZONE_VAR} (e.g. `s3://my-bucket/ubereats`), and it \
                 must name the same place as the airway pipeline's `base_path`"
            ),
        ));
    };
    let rest = raw.trim().trim_end_matches('/');
    let Some(rest) = rest.strip_prefix("s3://") else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!("{ZONE_VAR} must be an `s3://bucket/prefix` URL, got `{rest}`"),
        ));
    };
    let (bucket, prefix) = match rest.split_once('/') {
        Some((b, p)) => (b, p),
        None => (rest, ""),
    };
    if bucket.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!("{ZONE_VAR} names no bucket"),
        ));
    }
    Ok((bucket.to_string(), prefix.to_string()))
}

/// Where a report lands inside the zone.
///
/// **This layout is read by the loader, not just written by us.**
///
/// - `YYYY.MM` is a path component because that is what the source scans for
///   when deriving the period. It scans components right-to-left for a
///   `YYYY[.-_]MM` token, so the period must appear as its own segment — a
///   Hive-style `report_year=2026` would NOT be recognized.
/// - The path **within the zone** is what the source folds into `_row_uid`. So
///   two uploads that should merge must land on the same key, and two that must
///   coexist (two chunks of one period) must land on different ones. Re-posting
///   the same `workflow_id` for the same period is therefore idempotent by
///   construction: same key, same rows, same ids, and the loader's `Merge`
///   collapses them.
/// - The extension stays `.csv` because the source reads `.csv` only and
///   refuses a base naming anything else.
/// - **The workspace id is a segment**, because the zone is one process-wide
///   setting shared by every workspace on a deployment. Without it, two
///   workspaces posting the same `workflow_id` for the same period write the
///   same object — and since the key IS the merge identity, the second
///   silently overwrites the first and its rows merge into the other tenant's
///   table at identical `_row_uid`s. It sits BEFORE the period so the source's
///   right-to-left scan still finds `YYYY.MM` as its own segment.
///
/// A pipeline therefore points `base_path` at `<zone>/<workspace_id>`, not at
/// the zone root.
fn object_key(
    prefix: &str,
    workspace_id: Uuid,
    year: i64,
    month: u32,
    workflow_id: &str,
) -> String {
    let period = format!("{year:04}.{month:02}");
    let tail = format!("{workspace_id}/{period}/{workflow_id}.csv");
    if prefix.is_empty() {
        tail
    } else {
        format!("{prefix}/{tail}")
    }
}

/// A `workflow_id` has to be safe to put in an object key.
///
/// Rejected rather than sanitized: a silently-rewritten id would produce a key
/// the caller does not expect, and since the key IS the merge identity, a
/// caller who thinks they re-posted the same report would instead land a second
/// copy under a different name.
fn valid_workflow_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// Read a numeric multipart field, or refuse naming the field and the value.
async fn parse_field<T: std::str::FromStr>(
    field: axum::extract::multipart::Field<'_>,
    name: &str,
) -> Result<T, (StatusCode, String)> {
    let raw = field
        .text()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("reading `{name}`: {e}")))?;
    raw.trim().parse::<T>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            format!("`{name}` is not a number: `{}`", raw.trim()),
        )
    })
}

/// `POST /api/{workspace_id}/ubereats/reports`
///
/// Multipart fields: `file` (required), `workflow_id` (required),
/// `report_year` + `report_month` (optional, both-or-neither).
pub async fn upload_report(
    // Writing the input to a journal-entry pipeline is a contributor action:
    // `workspace_middleware` proves the caller may *access* the workspace, not
    // that they may change it, so a Viewer would otherwise pass.
    _editor: WorkspaceEditor,
    Path(workspace_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<axum::Json<UploadedReport>, (StatusCode, String)> {
    let (bucket, prefix) = zone()?;

    let mut bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut workflow_id: Option<String> = None;
    let mut year: Option<i64> = None;
    let mut month: Option<u32> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart error: {e}")))?
    {
        match field.name().unwrap_or_default() {
            "file" => {
                filename = field.file_name().map(str::to_string);
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("file read: {e}")))?;
                if data.len() > MAX_REPORT_BYTES {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!(
                            "report is {} bytes, over the {MAX_REPORT_BYTES}-byte ceiling",
                            data.len()
                        ),
                    ));
                }
                bytes = Some(data.to_vec());
            }
            "workflow_id" => {
                // Not `.ok()`: a read failure here would leave `None` and tell
                // the caller the field is *missing* — the same misdirection
                // `parse_field` was added to remove one field over.
                workflow_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::BAD_REQUEST,
                                format!("reading `workflow_id`: {e}"),
                            )
                        })?
                        .trim()
                        .to_string(),
                );
            }
            // Parsed strictly. Silently dropping an unparseable value told a
            // caller who sent BOTH halves that they must "be given together",
            // which is the same misdirection this file refuses to commit by
            // sanitizing a `workflow_id`.
            "report_year" => year = Some(parse_field::<i64>(field, "report_year").await?),
            "report_month" => month = Some(parse_field::<u32>(field, "report_month").await?),
            _ => {}
        }
    }

    let bytes = bytes.ok_or((StatusCode::BAD_REQUEST, "missing `file` field".to_string()))?;
    let workflow_id = workflow_id.ok_or((
        StatusCode::BAD_REQUEST,
        "missing `workflow_id` field — it names the object, and re-posting the same \
         id for the same period is what makes an upload idempotent"
            .to_string(),
    ))?;
    if !valid_workflow_id(&workflow_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "`workflow_id` must be 1–128 characters of [A-Za-z0-9_-] — it becomes an \
             object key, and sanitizing it silently would land the report somewhere \
             the caller did not name"
                .to_string(),
        ));
    }
    // Both or neither, matching the pipeline config's rule for the same pair:
    // half a period is not one, and guessing the other half stamps a month
    // nobody named.
    let period = match (year, month) {
        (Some(y), Some(m)) => {
            // The same bounds the pipeline config enforces, from one
            // definition — here the period is also interpolated into the
            // object key, so `2026.13` would produce a key the source's period
            // scan cannot read and strand the report in the zone.
            check_period(y, m).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
            Some((y, m))
        }
        (None, None) => None,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "`report_year` and `report_month` must be given together — half a \
                 period is not one"
                    .to_string(),
            ));
        }
    };
    // A name is required, because it is what the period is derived from when
    // the caller did not supply one — and `.csv`, because the source reads
    // nothing else and would refuse this file at load time.
    let filename = filename.filter(|f| !f.trim().is_empty()).ok_or((
        StatusCode::BAD_REQUEST,
        "the `file` part needs a filename — the report period is read from it when \
         `report_year`/`report_month` are not given"
            .to_string(),
    ))?;
    if !filename.to_ascii_lowercase().ends_with(".csv") {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "`{filename}` is not a `.csv` — this source reads CSV only, and `.xlsx` \
                 is out of scope"
            ),
        ));
    }

    let validated = validate_ubereats_report(&bytes, &filename, period)
        .await
        .map_err(|e| match e {
            // Airway's message verbatim: it names the file and the missing
            // column, and rewording it would make the upload-time and
            // load-time diagnoses differ for one cause.
            ReportValidationError::Rejected(msg) => (StatusCode::BAD_REQUEST, msg),
            ReportValidationError::Unavailable(msg) => {
                tracing::error!(error = %msg, "UberEats report validation unavailable");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not validate the report".to_string(),
                )
            }
        })?;
    let (year, month, rows) = (
        validated.report_year,
        validated.report_month,
        validated.rows,
    );
    // Bounds again, because a period derived from the FILENAME never passed
    // through the check above and lands in the key just the same. Named for
    // where it came from: telling a caller who sent no form fields that
    // "`report_year` must be 2000–2100" points at something they never sent.
    //
    // Only the filename branch can fail here. A caller-supplied period was
    // already checked above, and `validate_ubereats_report` pins it with
    // `with_period`, so the value read back off a row equals the input — the
    // `Some(_)` arm below is defensive, not a second live path.
    check_period(year, month).map_err(|e| {
        let msg = match period {
            Some(_) => e,
            None => format!("the period read from `{filename}` is out of range: {e}"),
        };
        (StatusCode::BAD_REQUEST, msg)
    })?;
    let key = object_key(&prefix, workspace_id, year, month, &workflow_id);

    put_object(&bucket, &key, bytes).await?;

    Ok(axum::Json(UploadedReport {
        location: format!("s3://{bucket}/{key}"),
        report_year: year,
        report_month: month,
        rows,
    }))
}

/// Write the report, with its own S3 client.
///
/// Honors `AWS_ENDPOINT_URL` with path-style addressing so LocalStack/MinIO
/// work, matching the custom-app stores — that flag being forgotten in a second
/// client is the failure this comment exists to prevent.
async fn put_object(bucket: &str, key: &str, bytes: Vec<u8>) -> Result<(), (StatusCode, String)> {
    let shared = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let mut builder = aws_sdk_s3::config::Builder::from(&shared);
    if std::env::var("AWS_ENDPOINT_URL")
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        builder = builder.force_path_style(true);
    }
    aws_sdk_s3::Client::from_conf(builder.build())
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type("text/csv")
        .body(aws_sdk_s3::primitives::ByteStream::from(bytes))
        .send()
        .await
        // Category, not the error: an S3 SDK error's Display can carry the
        // built request URL, which for a presigned or credentialed request is
        // the credential.
        .map_err(|e| {
            tracing::error!(error = ?e, bucket, key, "UberEats report upload failed");
            (
                StatusCode::BAD_GATEWAY,
                "writing the report to the landing zone failed".to_string(),
            )
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The route must keep an authorization extractor.
    ///
    /// `authz_boundaries.rs` cannot catch this: it bans hand-rolled authority
    /// *shapes* (`matches!(role, Owner | Admin)` and friends), so a handler
    /// with NO guard at all is invisible to it — which is exactly how this
    /// route shipped letting a Viewer write into the journal-entry landing
    /// zone. Scanning the signature is the cheap check that would have caught
    /// it, so it lives next to the thing it protects.
    #[test]
    fn the_upload_handler_takes_a_role_guard() {
        let src = include_str!("ubereats_upload.rs");
        let at = src
            .find("pub async fn upload_report(")
            .expect("the handler must still be named this");
        let end = src[at..]
            .find(") -> Result")
            .expect("the handler must still return a Result");
        // Comments stripped: the window spans the explanatory comment above
        // `_editor`, so a future reword that merely MENTIONS the type in prose
        // would keep this green with the extractor gone. The test would then
        // assert the wording of a comment, not the presence of a guard.
        let signature: String = src[at..at + end]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        // ANY of the six guards, in a binding position — not this one exactly.
        //
        // Round 5 named `WorkspaceAdmin` as the defensible alternative for
        // financial ingest. Pinning `WorkspaceEditor` would make that
        // *tightening* fail the test, reporting "lost its role guard" — the
        // opposite of what happened. The property worth protecting is that a
        // guard is present, not which one; a test that resists being
        // strengthened teaches people to delete it.
        //
        // Hand-copied, and therefore a duplicate of the real set: adding a
        // seventh guard to `role_guards.rs` and using it here fails this test
        // with "has no role guard". It fails CLOSED and names the list, so it
        // is cheap to diagnose.
        //
        // **This whole scan is the wrong artifact and should be deleted.** The
        // durable check is "every mutating route names a guard from
        // `role_guards`", in `crates/app/tests/authz/authz_boundaries.rs` — it
        // already walks every handler source with a `BANNED`/`ALLOWED` pair,
        // and it would retire this test, this copied list, and the three
        // rounds of syntax-chasing above it (exact type, qualified path,
        // trailing comment). This scan exists because that check does not yet,
        // and it is NOT currently tracked by an issue — saying "tracked
        // separately" without naming one is how a deferral becomes permanent.
        const GUARDS: [&str; 6] = [
            "OrgOwner",
            "OrgAdmin",
            "OrgAdminStrict",
            "OrgMemberStrict",
            "WorkspaceAdmin",
            "WorkspaceEditor",
        ];
        // Each parameter's type reduced to its LAST `::` segment, so a
        // fully-qualified binding — `_editor:
        // oxy_server_authz::role_guards::WorkspaceAdmin`, the form round 5's
        // own snippet used — still counts. Binding position is kept, so a bare
        // mention in prose cannot satisfy it.
        let guarded = signature.lines().any(|line| {
            // Truncated at `//`, which covers a TRAILING comment as well as a
            // whole-line one. Without it, `_editor: WorkspaceEditor, // …`
            // leaves the comment glued to the type, `trim_end_matches(',')`
            // no-ops because the line ends in prose, and the test reports "no
            // role guard" on a guarded route.
            // `split` with a non-empty pattern always yields one item.
            let line = line.split("//").next().expect("split yields one item");
            let Some((_, ty)) = line.split_once(':') else {
                return false;
            };
            let ty = ty.trim().trim_end_matches(',').trim();
            // `rsplit` always yields at least one item, so this is total.
            let last = ty.rsplit("::").next().expect("rsplit yields one item");
            GUARDS.contains(&last)
        });

        assert!(
            guarded,
            "the upload handler has no role guard in its signature — \
             `workspace_middleware` proves workspace ACCESS, not that the caller \
             may write. If you deliberately CHANGED the guard, it must still be \
             one of {GUARDS:?}. Signature was:\n{signature}"
        );
    }

    fn ws(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// The period must be its own `YYYY.MM` path segment, because that is what
    /// the source scans for. A Hive-style `report_year=2026` is NOT recognized
    /// by `period_from_filename`, so writing one would strand every report.
    #[test]
    fn the_key_puts_the_period_in_its_own_segment() {
        let a = ws(1);
        assert_eq!(
            object_key("ubereats", a, 2026, 8, "wf-1"),
            format!("ubereats/{a}/2026.08/wf-1.csv")
        );
        // Zero-padded, matching the token the source parses.
        assert_eq!(
            object_key("z", a, 2026, 1, "w"),
            format!("z/{a}/2026.01/w.csv")
        );
        // A zone at the bucket root has no leading slash to double up.
        assert_eq!(
            object_key("", a, 2026, 8, "wf-1"),
            format!("{a}/2026.08/wf-1.csv")
        );
    }

    /// The zone is one process-wide setting shared by every workspace, and the
    /// key IS the merge identity — so without a workspace segment one tenant
    /// overwrites another's object, and its rows merge into the other's table
    /// at identical `_row_uid`s.
    #[test]
    fn two_workspaces_cannot_collide_on_one_key() {
        assert_ne!(
            object_key("z", ws(1), 2026, 8, "wf-1"),
            object_key("z", ws(2), 2026, 8, "wf-1"),
            "same workflow and period in two workspaces must not share a key"
        );
    }

    /// The workspace segment must sit BEFORE the period, or the source's
    /// right-to-left scan stops finding `YYYY.MM` as its own component.
    #[test]
    fn the_period_is_still_the_last_directory_segment() {
        let key = object_key("z", ws(7), 2026, 8, "wf-1");
        let mut parts: Vec<&str> = key.split('/').collect();
        parts.pop(); // the file
        assert_eq!(
            parts.last().copied(),
            Some("2026.08"),
            "the period must be the directory the report sits in: {key}"
        );
    }

    /// The key IS the merge identity: same workflow + period must produce the
    /// same key so a re-post merges, and a different workflow must not.
    #[test]
    fn the_key_is_the_idempotency_boundary() {
        let w = ws(1);
        let a = object_key("z", w, 2026, 8, "wf-1");
        assert_eq!(
            a,
            object_key("z", w, 2026, 8, "wf-1"),
            "a re-post must collide"
        );
        assert_ne!(a, object_key("z", w, 2026, 8, "wf-2"), "chunks must not");
        assert_ne!(a, object_key("z", w, 2026, 9, "wf-1"), "periods must not");
    }

    /// Rejected, not sanitized — a rewritten id lands the report under a name
    /// the caller did not choose, and the key is the merge identity.
    #[test]
    fn a_workflow_id_that_would_reshape_the_key_is_refused() {
        assert!(valid_workflow_id("wf-1"));
        assert!(valid_workflow_id("chunk_2026_08_a"));

        for bad in [
            "",
            "../escape",
            "a/b",
            "wf 1",
            "wf.1",
            "wf%2F",
            &"x".repeat(129),
        ] {
            assert!(!valid_workflow_id(bad), "`{bad}` must be refused");
        }
    }

    fn with_zone<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
        let prev = std::env::var(ZONE_VAR).ok();
        match value {
            Some(v) => unsafe { std::env::set_var(ZONE_VAR, v) },
            None => unsafe { std::env::remove_var(ZONE_VAR) },
        }
        let out = f();
        match prev {
            Some(p) => unsafe { std::env::set_var(ZONE_VAR, p) },
            None => unsafe { std::env::remove_var(ZONE_VAR) },
        }
        out
    }

    #[test]
    fn the_zone_url_splits_into_bucket_and_prefix() {
        with_zone(Some("s3://my-bucket/ubereats"), || {
            assert_eq!(zone().unwrap(), ("my-bucket".into(), "ubereats".into()));
        });
        with_zone(Some("s3://my-bucket/a/b/"), || {
            assert_eq!(zone().unwrap(), ("my-bucket".into(), "a/b".into()));
        });
        with_zone(Some("s3://my-bucket"), || {
            assert_eq!(zone().unwrap(), ("my-bucket".into(), String::new()));
        });
    }

    /// Unset is a 503 naming the variable, not a 500 — the deployment is
    /// incomplete rather than the request being wrong, and the operator needs
    /// to know it must match the pipeline's `base_path`.
    #[test]
    fn an_unconfigured_zone_says_so() {
        with_zone(None, || {
            let (status, msg) = zone().unwrap_err();
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert!(msg.contains(ZONE_VAR), "names the variable: {msg}");
            assert!(msg.contains("base_path"), "names the contract: {msg}");
        });

        with_zone(Some("/local/path"), || {
            let (status, msg) = zone().unwrap_err();
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert!(msg.contains("s3://"), "says what it wants: {msg}");
        });
    }
}
