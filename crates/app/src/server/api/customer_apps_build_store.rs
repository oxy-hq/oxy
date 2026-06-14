//! Storage for versioned customer-app bundles.
//!
//! Each publish writes its files under
//! `customer-apps/<app_id>/builds/<build_id>/`. Two backends, selected at
//! call time by whether `OXY_CUSTOMER_APPS_S3_BUCKET` is set:
//!
//! - **S3** (cloud / configured): the single source of truth — oxy holds no
//!   local copy. The serve path reads objects back through an in-memory
//!   cache ([`super::customer_apps_bundle_cache`]).
//! - **Filesystem** (local dev, no bucket): files land under
//!   `<OXY_STATE_DIR>/customer-apps/<app_id>/builds/<build_id>/` so an
//!   engineer can `oxy serve` + `oxy publish` locally without MinIO/S3.
//!
//! Either way the logical key is [`build_prefix`]; it's recorded verbatim in
//! `app_builds.s3_prefix` and interpreted as an S3 key or an FS subpath
//! depending on the active backend. The serve path goes through this module,
//! so local serve "just works" off the filesystem.

use std::path::PathBuf;

use aws_config::BehaviorVersion;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum BuildStoreError {
    #[error("s3 error: {0}")]
    S3(String),
    #[error("filesystem build-store error: {0}")]
    Io(String),
}

/// Logical prefix (no leading slash, trailing slash) that holds a given
/// build's files. The single canonical layout for the publish pipeline,
/// shared by both backends.
pub fn build_prefix(app_id: Uuid, build_id: &str) -> String {
    format!("customer-apps/{app_id}/builds/{build_id}/")
}

/// Configured S3 bucket, or `None` to select the filesystem backend.
fn bucket() -> Option<String> {
    std::env::var("OXY_CUSTOMER_APPS_S3_BUCKET")
        .ok()
        .filter(|b| !b.trim().is_empty())
}

/// Root of the local-mode build store: `OXY_STATE_DIR`, else the platform
/// data dir under `oxy`. Both `put`/`get`/`delete` resolve through here so
/// writes and reads always agree.
fn state_root() -> PathBuf {
    if let Ok(dir) = std::env::var("OXY_STATE_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .map(|d| d.join("oxy"))
        .unwrap_or_else(|| PathBuf::from(".oxy-state"))
}

/// Absolute on-disk directory for a build under the filesystem backend.
fn fs_build_dir(app_id: Uuid, build_id: &str) -> PathBuf {
    state_root().join(build_prefix(app_id, build_id))
}

/// Reject relative paths that could escape the build dir on the filesystem
/// backend (`..` components or absolute). The tar extractor already guards
/// uploaded paths; this is defense-in-depth for both put and the serve-time
/// get, whose `rel_path` is derived from the request URL.
fn is_safe_rel(rel: &str) -> bool {
    let p = std::path::Path::new(rel);
    !p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Build an S3 client, honoring `AWS_ENDPOINT_URL` (localstack/MinIO) by
/// forcing path-style addressing.
async fn s3_client() -> S3Client {
    let shared = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let mut builder = aws_sdk_s3::config::Builder::from(&shared);
    if std::env::var("AWS_ENDPOINT_URL")
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        builder = builder.force_path_style(true);
    }
    S3Client::from_conf(builder.build())
}

/// Upload every `(relative_path, bytes)` pair under the build prefix.
/// Returns the prefix that was written.
pub async fn put_build(
    app_id: Uuid,
    build_id: &str,
    files: Vec<(String, Vec<u8>)>,
) -> Result<String, BuildStoreError> {
    let prefix = build_prefix(app_id, build_id);
    if let Some(bad) = files.iter().find(|(rel, _)| !is_safe_rel(rel)) {
        return Err(BuildStoreError::Io(format!("unsafe build path: {}", bad.0)));
    }
    // Multi-instance guard: the filesystem backend writes the bundle to THIS
    // node's local disk, but on a multi-replica deployment (OXY_ROLE !=
    // `all`) the serve path round-robins — a later asset request lands on a
    // replica whose local disk has no such build → blank page / 404. Refuse
    // the publish with a clear error instead of silently degrading to
    // per-node-local storage. Only affects deployments that actually use
    // customer apps; `all`-mode (local dev / single process) is unaffected.
    if bucket().is_none()
        && crate::server::role_manifest::current_process_role()
            != crate::server::role_manifest::Role::All
    {
        return Err(BuildStoreError::Io(
            "customer-app publish requires a shared object store on a multi-instance \
             deployment: set OXY_CUSTOMER_APPS_S3_BUCKET (the filesystem backend is \
             single-node only)"
                .to_string(),
        ));
    }
    match bucket() {
        Some(bucket) => {
            let client = s3_client().await;
            for (rel, bytes) in files {
                let key = format!("{prefix}{}", rel.trim_start_matches('/'));
                client
                    .put_object()
                    .bucket(&bucket)
                    .key(&key)
                    .body(ByteStream::from(bytes))
                    .send()
                    .await
                    .map_err(|e| BuildStoreError::S3(format!("put_object {key}: {e}")))?;
            }
        }
        None => {
            let root = fs_build_dir(app_id, build_id);
            for (rel, bytes) in files {
                let dest = root.join(rel.trim_start_matches('/'));
                if let Some(parent) = dest.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        BuildStoreError::Io(format!("mkdir {}: {e}", parent.display()))
                    })?;
                }
                tokio::fs::write(&dest, bytes)
                    .await
                    .map_err(|e| BuildStoreError::Io(format!("write {}: {e}", dest.display())))?;
            }
        }
    }
    Ok(prefix)
}

/// Fetch a single object from a build. `Ok(None)` when the key/file is
/// absent (so the serve path can fall back to the SPA index or 404).
pub async fn get_object(
    app_id: Uuid,
    build_id: &str,
    rel_path: &str,
) -> Result<Option<Vec<u8>>, BuildStoreError> {
    let rel = rel_path.trim_start_matches('/');
    if !is_safe_rel(rel) {
        return Ok(None);
    }
    match bucket() {
        Some(bucket) => {
            let client = s3_client().await;
            let key = format!("{}{}", build_prefix(app_id, build_id), rel);
            match client.get_object().bucket(&bucket).key(&key).send().await {
                Ok(resp) => {
                    let data = resp
                        .body
                        .collect()
                        .await
                        .map_err(|e| BuildStoreError::S3(format!("collect {key}: {e}")))?;
                    Ok(Some(data.into_bytes().to_vec()))
                }
                Err(err) => {
                    let is_missing = err
                        .as_service_error()
                        .map(|e| e.is_no_such_key())
                        .unwrap_or(false);
                    if is_missing {
                        Ok(None)
                    } else {
                        Err(BuildStoreError::S3(format!("get_object {key}: {err}")))
                    }
                }
            }
        }
        None => {
            let dest = fs_build_dir(app_id, build_id).join(rel);
            match tokio::fs::read(&dest).await {
                Ok(bytes) => Ok(Some(bytes)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(BuildStoreError::Io(format!("read {}: {e}", dest.display()))),
            }
        }
    }
}

/// Delete every object/file under a build prefix (keep-last-N GC).
pub async fn delete_build(app_id: Uuid, build_id: &str) -> Result<(), BuildStoreError> {
    match bucket() {
        Some(bucket) => {
            let client = s3_client().await;
            let prefix = build_prefix(app_id, build_id);
            let mut continuation: Option<String> = None;
            loop {
                let mut req = client.list_objects_v2().bucket(&bucket).prefix(&prefix);
                if let Some(token) = &continuation {
                    req = req.continuation_token(token.clone());
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| BuildStoreError::S3(format!("list_objects_v2 {prefix}: {e}")))?;
                for obj in resp.contents() {
                    if let Some(key) = obj.key() {
                        client
                            .delete_object()
                            .bucket(&bucket)
                            .key(key)
                            .send()
                            .await
                            .map_err(|e| {
                                BuildStoreError::S3(format!("delete_object {key}: {e}"))
                            })?;
                    }
                }
                if resp.is_truncated().unwrap_or(false) {
                    continuation = resp.next_continuation_token().map(str::to_string);
                    if continuation.is_none() {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        None => {
            let dir = fs_build_dir(app_id, build_id);
            match tokio::fs::remove_dir_all(&dir).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(BuildStoreError::Io(format!("rmdir {}: {e}", dir.display()))),
            }
        }
    }
    Ok(())
}

/// Delete every object/file under `customer-apps/<app_id>/` — used by
/// the admin delete-app endpoint to reclaim S3 storage so a deleted
/// row doesn't leak its bundle bytes forever. Best-effort: returns the
/// underlying error so the caller can decide whether to fail the
/// HTTP request or just log and proceed (the admin handler logs and
/// proceeds — see `delete_app` in `admin/apps/handlers.rs`).
///
/// On the local filesystem backend this is a `remove_dir_all` of
/// `<state_root>/customer-apps/<app_id>/`. On S3 it walks the prefix
/// in pages and batches each page through `DeleteObjects` (≤1000 keys
/// per call — the S3 limit, which matches `ListObjectsV2`'s default
/// page size, so each page becomes exactly one delete request).
pub async fn delete_app(app_id: Uuid) -> Result<(), BuildStoreError> {
    match bucket() {
        Some(bucket) => {
            let client = s3_client().await;
            let prefix = format!("customer-apps/{app_id}/");
            let mut continuation: Option<String> = None;
            loop {
                let mut req = client.list_objects_v2().bucket(&bucket).prefix(&prefix);
                if let Some(token) = &continuation {
                    req = req.continuation_token(token.clone());
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| BuildStoreError::S3(format!("list_objects_v2 {prefix}: {e}")))?;

                // Batch this page into a single DeleteObjects call (≤1000
                // keys per S3 limit; ListObjectsV2 returns ≤1000 by default
                // so one delete-objects call covers each page exactly).
                let keys: Vec<aws_sdk_s3::types::ObjectIdentifier> = resp
                    .contents()
                    .iter()
                    .filter_map(|o| o.key())
                    .filter_map(|k| {
                        aws_sdk_s3::types::ObjectIdentifier::builder()
                            .key(k)
                            .build()
                            .ok()
                    })
                    .collect();
                if !keys.is_empty() {
                    let del = aws_sdk_s3::types::Delete::builder()
                        .set_objects(Some(keys))
                        .quiet(true)
                        .build()
                        .map_err(|e| BuildStoreError::S3(format!("build Delete payload: {e}")))?;
                    client
                        .delete_objects()
                        .bucket(&bucket)
                        .delete(del)
                        .send()
                        .await
                        .map_err(|e| {
                            BuildStoreError::S3(format!("delete_objects {prefix}: {e}"))
                        })?;
                }

                if resp.is_truncated().unwrap_or(false) {
                    continuation = resp.next_continuation_token().map(str::to_string);
                    if continuation.is_none() {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        None => {
            // FS backend mirrors the S3 prefix layout: builds live at
            // `<state_root>/customer-apps/<app_id>/builds/<build_id>/…`,
            // so the per-app directory we want to nuke is
            // `<state_root>/customer-apps/<app_id>/`.
            let app_dir = state_root().join(format!("customer-apps/{app_id}"));
            match tokio::fs::remove_dir_all(&app_dir).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(BuildStoreError::Io(format!(
                        "rmdir {}: {e}",
                        app_dir.display()
                    )));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prefix_has_no_leading_slash_and_trailing_slash() {
        let p = build_prefix(Uuid::nil(), "abc123");
        assert!(!p.starts_with('/'), "no leading slash: {p}");
        assert!(p.ends_with('/'), "trailing slash: {p}");
        assert_eq!(
            p,
            "customer-apps/00000000-0000-0000-0000-000000000000/builds/abc123/"
        );
    }

    #[tokio::test]
    async fn delete_app_removes_every_build_on_fs_and_is_idempotent() {
        // Force the filesystem backend into a temp state dir.
        let tmp = std::env::temp_dir().join(format!("oxy-bs-test-{}", Uuid::new_v4()));
        // SAFETY: single-threaded test; we set then clear the vars.
        unsafe {
            std::env::remove_var("OXY_CUSTOMER_APPS_S3_BUCKET");
            std::env::set_var("OXY_STATE_DIR", &tmp);
        }
        let app = Uuid::new_v4();

        // Two builds + a nested asset under each — must all disappear.
        put_build(
            app,
            "b1",
            vec![
                ("index.html".into(), b"<html>1".to_vec()),
                ("assets/main.js".into(), b"console.log(1)".to_vec()),
            ],
        )
        .await
        .expect("put b1");
        put_build(app, "b2", vec![("index.html".into(), b"<html>2".to_vec())])
            .await
            .expect("put b2");
        let app_dir = state_root().join(format!("customer-apps/{app}"));
        assert!(app_dir.exists(), "app dir should exist before delete");

        delete_app(app).await.expect("delete_app");
        assert!(!app_dir.exists(), "app dir gone after delete_app");
        assert_eq!(
            get_object(app, "b1", "index.html").await.expect("get b1"),
            None,
            "b1 index.html gone"
        );
        assert_eq!(
            get_object(app, "b2", "index.html").await.expect("get b2"),
            None,
            "b2 index.html gone"
        );

        // Idempotent: calling again on a non-existent app dir is a no-op
        // (NotFound is swallowed), so an admin double-click can't 500.
        delete_app(app).await.expect("delete_app idempotent");

        unsafe {
            std::env::remove_var("OXY_STATE_DIR");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn fs_backend_put_get_delete_roundtrips() {
        // Force the filesystem backend into a temp state dir.
        let tmp = std::env::temp_dir().join(format!("oxy-bs-test-{}", Uuid::new_v4()));
        // SAFETY: single-threaded test; we set then clear the vars.
        unsafe {
            std::env::remove_var("OXY_CUSTOMER_APPS_S3_BUCKET");
            std::env::set_var("OXY_STATE_DIR", &tmp);
        }
        let app = Uuid::new_v4();
        put_build(app, "b1", vec![("index.html".into(), b"<html>hi".to_vec())])
            .await
            .expect("put");
        let got = get_object(app, "b1", "index.html").await.expect("get");
        assert_eq!(got.as_deref(), Some(&b"<html>hi"[..]));
        let missing = get_object(app, "b1", "nope.js").await.expect("get missing");
        assert_eq!(missing, None);
        delete_build(app, "b1").await.expect("delete");
        let after = get_object(app, "b1", "index.html")
            .await
            .expect("get after delete");
        assert_eq!(after, None);
        unsafe {
            std::env::remove_var("OXY_STATE_DIR");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
