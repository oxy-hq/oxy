//! Fails the build when a test starts using the shared database without being
//! serialized for it.
//!
//! `oxy-app`'s six integration binaries are mixed. Most of what's in them is
//! in-process — source scanning, router shape, pure authz decisions — and runs at
//! full parallelism. Twelve modules are not. They reach the raw shared
//! `OXY_DATABASE_URL` with no per-test database, so they touch the same `public`
//! schema that the `serial-db` members (`oxy`, `oxy-cameras`, `agentic-*`) run
//! `Migrator::up` against, and one of them *writes* to `agentic_runs`. Those
//! twelve are pinned into `serial-db` by an explicit list in
//! `.config/nextest.toml`.
//!
//! Two routes in, which is the reason this gate looks for more than one string:
//! ten call `oxy::database::client::establish_connection()` directly, and
//! `projects_query`/`local_mode_router` reach it through `api_router(..)`.
//!
//! An explicit list is only as good as its freshness, and this one fails in the
//! quietest possible way: the tests skip when `OXY_DATABASE_URL` is unset, so a
//! laptop run stays green and only CI — which does set it — would ever surface
//! the race, as a flake, somewhere unrelated. That is worth a build gate.
//!
//! Checked in both directions on purpose. A module that starts using the shared
//! connection has to be added (or ported to `common::fresh_db`); a module that
//! stops using it has to be removed, so the list can't quietly accrete names and
//! serialize tests that no longer need it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Repo root — `CARGO_MANIFEST_DIR` is `crates/app`.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

/// **Every** `oxy-app` *grouped* integration binary. Not a curated subset.
///
/// The scan is `read_dir` over these directories, so a top-level `tests/*.rs` is
/// outside it by construction — invisible here, and one more link on every full
/// run. `tests/artifact_naming_agrees.rs` is the last one; folding it into
/// `platform` closes both gaps at once and is the reason this reads "grouped"
/// rather than dropping the word "every", which is the part that matters.
///
/// This list was twice too short, both times for the same reason: a binary was
/// left out because its tests were "already grouped", and grouping was mistaken
/// for serialization. It isn't — nextest test-groups are independent semaphores,
/// so a `max-threads` cap in one group constrains nothing against another, and a
/// `db-per-test` member can run concurrently with the `serial-db` tests whose
/// rows it rewrites. `platform` was the live example; `custom_apps` and
/// `airhouse` were the same hole still open.
///
/// The cost of scanning a binary that turns out to be clean is zero. The cost of
/// omitting one is a race nobody sees until CI flakes in another crate. So: all
/// of them, and any new one goes here on the day it's created.
const MIXED_BINARIES: &[&str] = &[
    "authz",
    "slack",
    "platform",
    "custom_apps",
    "airhouse",
    "routing",
];

/// Ways a test reaches a database.
///
/// `api_router` is not incidental: it builds `new_agentic_state`, which calls
/// `establish_connection()` and then `cleanup_stale_runs()` — an unscoped UPDATE
/// across `agentic_runs`. A gate that only looked for the literal
/// `establish_connection` certified a claim it wasn't checking.
///
/// The trailing `(` on `api_router` is load-bearing: it matches call sites and
/// NOT a bare `use …::api_router;` import, which carries no paren. So a file that
/// imports the symbol without calling it is not flagged. `establish_connection`
/// has no paren because it is reached both as `establish_connection()` and as a
/// re-exported path.
const DB_ENTRY_POINTS: &[&str] = &["establish_connection", "api_router("];

/// Modules that reach the *shared* database, discovered from source.
///
/// Skips lines that are entirely a comment: `org_invitations.rs` documents
/// `establish_connection`'s memoization in a doc comment, and a gate that counts
/// prose is a gate that passes for the wrong reason.
fn modules_using_shared_db() -> BTreeSet<String> {
    let tests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut found = BTreeSet::new();

    for binary in MIXED_BINARIES {
        let dir = tests_dir.join(binary);
        for entry in std::fs::read_dir(&dir).expect("read test group dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("file stem")
                .to_string();
            // This file names the function it looks for, in the scanning code
            // and in its own failure messages, so it matches itself. It opens no
            // database. (Found the honest way: the gate failed on first run.)
            if stem == "main" || stem == "shared_db_registry" {
                continue;
            }
            if uses_shared_db(&path) {
                found.insert(stem);
            }
        }
    }
    found
}

/// True when a module reaches a database from real code.
///
/// There is deliberately **no** "but it uses `common::fresh_db`, so it's fine"
/// escape here. A previous version had one, justified by `compiled_reader_semantic`
/// and `toast_webhook_compile_boundary` supposedly looking like shared-DB users.
/// They don't — their only mentions of `establish_connection` are `///` doc
/// comments, which the filter below already drops — so it bought nothing, while
/// adding a suppression that worked at *file* granularity: one `fresh_db` call
/// anywhere in a file hid every other test in it. That is the same shape as the
/// two bugs this gate exists because of, one level down.
///
/// The tradeoff is deliberate. A file that genuinely combines the harness with
/// `api_router` will fail this gate and need a human decision — pin it, or port
/// it. Over-reporting costs someone five minutes; under-reporting costs a flake
/// in another crate that nobody traces back here.
fn uses_shared_db(path: &Path) -> bool {
    let src = std::fs::read_to_string(path).expect("read test source");
    src.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .any(|line| DB_ENTRY_POINTS.iter().any(|entry| line.contains(entry)))
}

/// The module alternation of a *positive* `test(/^(…)::/)` in one override block.
///
/// `None` when the block has no alternation, or only a negated one. nextest
/// spells negation two ways — `!test(…)` and `& not test(…)` — and a gate that
/// only knew the first would read the `db-per-test` exclusion list as if it were
/// the serial-db list, silently checking the wrong set and passing.
fn positive_alternation(block: &str) -> Option<&str> {
    const NEEDLE: &str = "test(/^(";
    block.match_indices(NEEDLE).find_map(|(i, _)| {
        let before = block[..i].trim_end();
        if before.ends_with('!') || before.ends_with("not") {
            return None;
        }
        let rest = &block[i + NEEDLE.len()..];
        rest.find(")::/").map(|end| &rest[..end])
    })
}

/// Modules pinned into `serial-db` by the oxy-app override in nextest's config.
fn modules_pinned_serial() -> BTreeSet<String> {
    let config = repo_root().join(".config/nextest.toml");
    let src = std::fs::read_to_string(&config).expect("read .config/nextest.toml");

    // Anchored on the override BLOCK, not on a line and not on file order.
    //
    // Two earlier versions of this were wrong in opposite ways. Line-based
    // matching required `filter =`, `package(=oxy-app)` and `test(/^(` to share
    // one physical line, so a `taplo fmt` wrapping that ~300-char string tripped
    // a panic that blamed a dropped serialization. Replacing it with a
    // whole-file `find` then picked the *first* alternation — which is the
    // `db-per-test` override's NEGATED exclusion list, a different set entirely.
    // Splitting on the block header gets both properties without either bug.
    let block = src
        .split("[[profile.default.overrides]]")
        .find(|b| b.contains("package(=oxy-app)") && positive_alternation(b).is_some())
        .unwrap_or_else(|| {
            panic!(
                "no oxy-app module-list override found in {}. If the shared-DB \
                 tests were ported to `common::fresh_db`, delete this test with \
                 them; otherwise the serialization was dropped and the race is \
                 back.",
                config.display()
            )
        });

    let alternation = positive_alternation(block).expect("checked above");
    alternation
        .split('|')
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect()
}

#[test]
fn shared_db_tests_are_pinned_to_the_serial_group() {
    let using = modules_using_shared_db();
    let pinned = modules_pinned_serial();

    let unpinned: Vec<_> = using.difference(&pinned).cloned().collect();
    assert!(
        unpinned.is_empty(),
        "these modules reach the shared database (via establish_connection or \
         api_router) but are NOT in the serial-db override in \
         .config/nextest.toml: {unpinned:?}\n\
         They will run in parallel against the same `public` schema that the \
         serial-db packages migrate — the CREATE TABLE / pg_type_typname_nsp_index \
         race. Either add them to that override's module list, or (better) port \
         them onto `common::fresh_db()` so they get their own database."
    );

    let stale: Vec<_> = pinned.difference(&using).cloned().collect();
    assert!(
        stale.is_empty(),
        "these modules are pinned into serial-db in .config/nextest.toml but no \
         longer reach the shared database (no establish_connection, no \
         api_router): {stale:?}\n\
         Drop them from the override — serializing tests that don't need it is \
         the wall-clock cost this grouping exists to avoid."
    );
}

/// The gate above is only meaningful if the source scan actually sees the
/// binaries. An empty scan would make it pass vacuously forever.
#[test]
fn the_scan_is_not_vacuous() {
    for binary in MIXED_BINARIES {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(binary);
        let count = std::fs::read_dir(&dir)
            .expect("read test group dir")
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("rs"))
            .count();
        assert!(
            count > 1,
            "{} has {count} .rs files — the shared-DB scan would be vacuous",
            dir.display()
        );
    }
    assert!(
        !modules_using_shared_db().is_empty(),
        "found no modules reaching the shared database at all; the detection \
         probably broke rather than the problem disappearing"
    );
}
