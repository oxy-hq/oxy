//! Boundary test: the only code that names the retired `apps.draft_build_id` /
//! `apps.published_build_id` columns is `custom_apps_environments::record_move`,
//! which still writes them for one release (the previous release reads them).
//! Nothing reads them, and nothing else writes them: every reader asks
//! `app_environments` (`custom_apps_env_resolve`).
//!
//! File-level string matching over production sources, comments excluded, so a
//! raw `SELECT`/`UPDATE` or an entity field named like the column is caught
//! wherever it creeps back. It cannot see a column built by string concatenation;
//! it catches the shape every reader and writer has used.
//!
//! The follow-up that drops the columns (#3686) deletes this test with the mirror.

use std::fs;
use std::path::{Path, PathBuf};

const COLUMNS: &[&str] = &["draft_build_id", "published_build_id"];

/// The one file allowed to name them.
const WRITER: &str = "crates/custom-apps/src/environments.rs";

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Production sources only. Test directories and `tests.rs` files seed and
/// assert on the columns on purpose; `migration` defines them.
fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read crates dir") {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if matches!(name, "tests" | "target" | "migration" | "node_modules") {
                continue;
            }
            source_files(&path, out);
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            out.push(path);
        }
    }
}

/// Files outside [`WRITER`] with a non-comment line naming a column, as
/// `path:line`.
fn offenders() -> Vec<String> {
    let mut files = Vec::new();
    source_files(&crates_dir(), &mut files);
    let mut out = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(crates_dir())
            .expect("under crates")
            .to_string_lossy()
            .replace('\\', "/");
        let relative = format!("crates/{relative}");
        if relative == WRITER {
            continue;
        }
        let Ok(src) = fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in src.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            // Also catches `DraftBuildId` / `PublishedBuildId` (the entity column).
            let squashed = line.replace('_', "").to_lowercase();
            if COLUMNS
                .iter()
                .any(|c| squashed.contains(&c.replace('_', "")))
            {
                out.push(format!("{relative}:{}", n + 1));
            }
        }
    }
    out
}

#[test]
fn only_record_move_names_the_retired_pointer_columns() {
    let offenders = offenders();
    assert!(
        offenders.is_empty(),
        "these lines name `apps.draft_build_id` / `apps.published_build_id`; read the \
         `app_environments` rows instead (`custom_apps_environments::pointer` / \
         `fixed_pointers`), and move pointers only through `record_move`:\n{}",
        offenders.join("\n")
    );
}

/// Guards the guard: the writer itself must still name both columns, or a
/// refactor that renamed them would leave the test above passing vacuously.
#[test]
fn the_writer_still_mirrors_both_columns() {
    let src = fs::read_to_string(crates_dir().join("custom-apps/src/environments.rs"))
        .expect("read the writer");
    for column in COLUMNS {
        assert!(src.contains(column), "{WRITER} no longer writes {column}");
    }
}
