//! Reading back every file of a stored build under one directory.
//!
//! A publish has its bundle in hand; a promote, a rollback or the console's
//! *Make Live* has only the build row. They re-read the build's declared
//! `*.sql` migrations from the stored bundle (environments design §5.5a,
//! ruling 4), so the store needs a list as well as [`super::get_object`].

use std::path::{Path, PathBuf};

use super::{
    BuildStoreError, bucket, build_prefix, fs_build_dir, get_object, is_containable_build_id,
    is_safe_rel, s3_client,
};
use uuid::Uuid;

/// A bound on what one listing may return. A migrations directory holds tens
/// of files; this only stops a pathological bundle from being read whole.
const MAX_FILES: usize = 2_000;

/// Every file of build `build_id` under `dir` whose name ends with `suffix`,
/// as `(path relative to the build, bytes)`, sorted by path. Nested
/// directories are included. An absent directory is an empty list.
pub async fn files_under(
    app_id: Uuid,
    build_id: &str,
    dir: &str,
    suffix: &str,
) -> Result<Vec<(String, Vec<u8>)>, BuildStoreError> {
    let dir = dir.trim().trim_matches('/');
    if !is_containable_build_id(build_id) || dir.is_empty() || !is_safe_rel(dir) {
        return Err(BuildStoreError::Io(format!(
            "unsafe build listing: build {build_id:?}, directory {dir:?}"
        )));
    }
    let mut paths = match bucket() {
        Some(bucket) => s3_paths(&bucket, app_id, build_id, dir).await?,
        None => fs_paths(&fs_build_dir(app_id, build_id), dir).await?,
    };
    paths.retain(|path| path.ends_with(suffix));
    paths.sort();
    if paths.len() > MAX_FILES {
        return Err(BuildStoreError::Io(format!(
            "{dir} in build {build_id:?} holds more than {MAX_FILES} files"
        )));
    }
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        // Listed a moment ago: a miss now is a store that lost the object.
        let bytes = get_object(app_id, build_id, &path).await?.ok_or_else(|| {
            BuildStoreError::Io(format!(
                "{path} was listed in build {build_id:?} but is gone"
            ))
        })?;
        files.push((path, bytes.to_vec()));
    }
    Ok(files)
}

/// The keys under `dir` in the bucket, relative to the build's prefix.
async fn s3_paths(
    bucket: &str,
    app_id: Uuid,
    build_id: &str,
    dir: &str,
) -> Result<Vec<String>, BuildStoreError> {
    let client = s3_client().await;
    let root = build_prefix(app_id, build_id);
    let prefix = format!("{root}{dir}/");
    let mut paths = Vec::new();
    let mut continuation: Option<String> = None;
    loop {
        let mut req = client.list_objects_v2().bucket(bucket).prefix(&prefix);
        if let Some(token) = &continuation {
            req = req.continuation_token(token.clone());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| BuildStoreError::S3(format!("list_objects_v2 {prefix}: {e}")))?;
        paths.extend(
            resp.contents()
                .iter()
                .filter_map(|o| o.key()?.strip_prefix(&root).map(str::to_string)),
        );
        if paths.len() > MAX_FILES {
            break;
        }
        continuation = match resp.is_truncated() {
            Some(true) => resp.next_continuation_token().map(str::to_string),
            _ => None,
        };
        if continuation.is_none() {
            break;
        }
    }
    Ok(paths)
}

/// The files under `root/dir` on the filesystem backend, relative to `root`
/// with `/` separators — the keys the bucket would hold.
async fn fs_paths(root: &Path, dir: &str) -> Result<Vec<String>, BuildStoreError> {
    let mut paths = Vec::new();
    let mut pending: Vec<PathBuf> = vec![root.join(dir)];
    while let Some(current) = pending.pop() {
        let mut entries = match tokio::fs::read_dir(&current).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(BuildStoreError::Io(format!(
                    "read_dir {}: {e}",
                    current.display()
                )));
            }
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| BuildStoreError::Io(format!("read_dir {}: {e}", current.display())))?
        {
            let path = entry.path();
            let kind = entry
                .file_type()
                .await
                .map_err(|e| BuildStoreError::Io(format!("stat {}: {e}", path.display())))?;
            if kind.is_dir() {
                pending.push(path);
            } else if let Some(rel) = relative_key(root, &path) {
                paths.push(rel);
            }
            if paths.len() > MAX_FILES {
                return Ok(paths);
            }
        }
    }
    Ok(paths)
}

/// `path` relative to `root`, joined with `/`. `None` for a path outside it.
fn relative_key(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_relative_to_the_build_and_slash_joined() {
        let root = Path::new("/state/customer-apps/x/builds/b1");
        assert_eq!(
            relative_key(root, &root.join("migrations").join("0001.sql")),
            Some("migrations/0001.sql".to_string())
        );
        assert_eq!(relative_key(root, Path::new("/elsewhere/0001.sql")), None);
    }

    /// The filesystem backend lists nested files under the directory and
    /// nothing beside it; an absent directory is an empty list.
    #[tokio::test]
    async fn the_filesystem_backend_lists_one_directory_recursively() {
        let root = std::env::temp_dir().join(format!("oxy-build-list-{}", Uuid::new_v4()));
        for (rel, body) in [
            ("migrations/0001.sql", "a"),
            ("migrations/nested/0002.sql", "b"),
            ("other/0003.sql", "c"),
        ] {
            let path = root.join(rel);
            tokio::fs::create_dir_all(path.parent().expect("a parent"))
                .await
                .expect("mkdir");
            tokio::fs::write(&path, body).await.expect("write");
        }
        let mut listed = fs_paths(&root, "migrations").await.expect("list");
        listed.sort();
        assert_eq!(
            listed,
            vec!["migrations/0001.sql", "migrations/nested/0002.sql"]
        );
        assert!(fs_paths(&root, "absent").await.expect("list").is_empty());
        let _ = tokio::fs::remove_dir_all(&root).await;
    }
}
