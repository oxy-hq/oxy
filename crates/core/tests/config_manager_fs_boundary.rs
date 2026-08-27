//! The `ConfigManager<S>` capability split, enforced mechanically.
//!
//! `ReadOnly` promises "this manager does not REQUIRE a working copy". That
//! promise is only worth anything if it is checked: the split started out wrong, with
//! nine methods that read the filesystem sitting on the capability-free impl while the
//! slot's own doc said the opposite. Nothing caught it, because the split was a
//! convention someone had to remember.
//!
//! Note what the promise is NOT. The slot carries `Option<WorkingCopy>`, so a node that
//! owns the files still serves the boundary-miss fallbacks in `impl<S: DiskSlot>`. An
//! earlier version asserted emptiness instead, and the middleware used it to downgrade
//! a manager that HELD the files — so the ide answered `NoSource` for an unpromoted
//! workspace, and for every feature-branch preview.
//!
//! So: every method reachable on `ConfigManager<ReadOnly>` must not delegate to
//! `self.storage`, which is the only door to the disk. A method that needs one
//! belongs on `impl ConfigManager<WorkingCopy>`.
//!
//! Same shape as `crates/app/tests/authz_boundaries.rs` — a source scan with an
//! allowlist, where the allowlist is a backlog rather than an exemption.

use std::path::PathBuf;

fn manager_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/config/manager.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Methods on the capability-free impl that are allowed to mention
/// `self.storage`. Empty on purpose: today none do, and a new one should have to
/// argue for itself in review rather than slip in.
const ALLOWED_STORAGE_USERS_ON_ANY_CAPABILITY: &[&str] = &[];

/// Split the file at the `impl ConfigManager<WorkingCopy>` header. Everything before it
/// that lives inside `impl<S> ConfigManager<S>` is reachable on the read-only slot.
fn capability_free_impl(src: &str) -> &str {
    let start = src
        .find("impl<S> ConfigManager<S> {")
        .expect("the capability-free impl block should exist");
    let end = src
        .find("impl ConfigManager<WorkingCopy> {")
        .expect("the WorkingCopy-only impl block should exist");
    assert!(
        start < end,
        "expected impl<S> to precede impl ConfigManager<WorkingCopy>; if the order changed, \
         this test's slicing needs updating rather than deleting"
    );
    &src[start..end]
}

/// Crude but sufficient: a method starts at `pub fn` / `pub async fn` and ends
/// at the next one. Enough to attribute a `self.storage` mention to a name.
fn methods(block: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in block.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed
            .strip_prefix("pub async fn ")
            .or_else(|| trimmed.strip_prefix("pub fn "))
            .or_else(|| trimmed.strip_prefix("pub(super) fn "))
            .or_else(|| trimmed.strip_prefix("async fn "))
            .or_else(|| trimmed.strip_prefix("fn "))
        {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if let Some(done) = current.take() {
                out.push(done);
            }
            current = Some((name, String::new()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(done) = current {
        out.push(done);
    }
    out
}

#[test]
fn nofs_methods_never_reach_the_filesystem() {
    let src = manager_source();
    let block = capability_free_impl(&src);

    let offenders: Vec<String> = methods(block)
        .into_iter()
        .filter(|(name, body)| {
            body.contains("self.storage")
                && !ALLOWED_STORAGE_USERS_ON_ANY_CAPABILITY.contains(&name.as_str())
        })
        .map(|(name, _)| name)
        .collect();

    assert!(
        offenders.is_empty(),
        "these methods are reachable on ConfigManager<ReadOnly> but delegate to \
         `self.storage`, which reads the working copy:\n  {}\n\n\
         A stateless serve replica has no working copy, so calling one there \
         returns empty data rather than an error — the exact silent failure the \
         capability parameter exists to prevent. Move them to \
         `impl ConfigManager<WorkingCopy>`.",
        offenders.join("\n  ")
    );
}

#[test]
fn the_fs_impl_actually_holds_the_filesystem_methods() {
    // Counter-guard: if someone "fixes" the test above by deleting the split
    // rather than honouring it, this fails.
    let src = manager_source();
    let fs_block = &src[src
        .find("impl ConfigManager<WorkingCopy> {")
        .expect("the WorkingCopy-only impl block should exist")..];

    // The `list_*` methods are deliberately absent: `list_apps`,
    // `list_analytics_agents` and `list_pipelines` moved to the artifacts impl,
    // where they serve the compile boundary first and reach for a disk only as
    // a fallback. `artifact_reads_reach_the_disk_through_one_door` guards them
    // there. What is left here has no second source: a raw path, the state dir,
    // and the two single-entity reads that have not moved yet.
    for expected in [
        "fn workspace_path",
        "fn resolve_state_dir",
        "fn resolve_automation",
    ] {
        assert!(
            fs_block.contains(expected),
            "`{expected}` should live on `impl ConfigManager<WorkingCopy>` — it reads the \
             working copy"
        );
    }
}

/// The artifacts impl (`impl<S: DiskSlot>`) is reachable on the read-only slot, so the
/// old guarantee — "the method does not exist" — no longer applies to it. This is
/// the replacement: every read there must reach the disk through `disk()`, which
/// returns `NoSource` / `WorkspaceUnavailable` rather than an empty list.
///
/// A method that calls `.storage()` itself skips that classification, and a
/// replica gets `[]` where it should get a retryable error. That is the shape
/// behind both shipped outages.
#[test]
fn artifact_reads_reach_the_disk_through_one_door() {
    let src = manager_source();
    let start = src
        .find("impl<S: DiskSlot> ConfigManager<S> {")
        .expect("the artifacts impl should exist");
    let end = src
        .find("impl ConfigManager<WorkingCopy> {")
        .expect("the WorkingCopy-only impl should exist");
    assert!(
        start < end,
        "artifacts impl should precede the WorkingCopy impl"
    );
    let block = &src[start..end];

    let offenders: Vec<String> = methods(block)
        .into_iter()
        .filter(|(name, body)| name != "disk" && body.contains(".storage()"))
        .map(|(name, _)| name)
        .collect();

    assert!(
        offenders.is_empty(),
        "these artifact reads reach the working copy directly instead of going \
         through `disk()`:\n  {}\n\n\
         `disk()` is what turns an absent root into a retryable \
         `WorkspaceUnavailable` and a missing capability into `NoSource`. \
         Bypassing it means a stateless replica answers `[]` instead of saying \
         it cannot serve the request.",
        offenders.join("\n  ")
    );
}

/// A `compiled_*` method returns `Option`, and the `None` means "this manager
/// is not reading a compiled revision" — half a decision. Every caller outside
/// the crate that had to interpret it wrote the other half itself, and they
/// wrote it differently: one swallowed the error into an empty list
/// (`materialise_agent_context`, four times over), one into a 404
/// (`app_service::get_tasks`), one into a silent FS fall-through
/// (`display.rs`). The operator-facing symptom was being told to recompile a
/// workspace that was compiled fine.
///
/// They are `pub(super)` now, resolved in `scan.rs`, and a public one would put
/// that choice back in front of a caller who has no way to know it is half an
/// answer.
#[test]
fn no_half_decision_leaves_the_crate() {
    let src = manager_source();
    let leaked: Vec<String> = src
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("pub async fn compiled_"))
        .map(|rest| format!("compiled_{}", rest.split('(').next().unwrap_or(rest)))
        .collect();

    assert!(
        leaked.is_empty(),
        "these `compiled_*` readers are public:\n  {}\n\n\
         A `compiled_*` name means the method answers only the boundary arm and \
         returns `None` for the other one, which makes the caller decide. That \
         is the thing `ConfigManager` exists to stop.\n\
         \x20 - To READ the artifact, add a method that owns both arms — \
         `automation_definition` and `semantics_scan` are the two shapes.\n\
         \x20 - To ask specifically about the PROMOTED revision (the smoke test \
         does), return `Result` with `NoSource` on `Origin::Disk`, like \
         `promoted_semantic_entities` — no `Option` for anyone to reinterpret.",
        leaked.join("\n  ")
    );
}

#[test]
fn the_capability_is_named_not_inferred() {
    // The builder's plain terminals must yield a NAMED slot. If they ever become
    // generic again, `S` goes back to being decided by inference: call a
    // filesystem method downstream and it silently unifies to `WorkingCopy`, so new
    // disk-touching code compiles unchanged and shows a reviewer nothing.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/config/builder.rs");
    let src = std::fs::read_to_string(&path).expect("read builder.rs");

    // Matched on the function NAME, not a whole one-line signature: rustfmt
    // wraps a signature the moment it crosses 100 columns, and a literal that
    // includes `(self)` turns a rename into a PANIC in this test rather than a
    // failure. It has done exactly that once.
    for (terminal, expected) in [
        (
            "pub async fn build_without_working_copy(",
            "ConfigManager<ReadOnly>",
        ),
        (
            "pub fn build_with_provided_config_and_working_copy(",
            "ConfigManager<WorkingCopy>",
        ),
        (
            "pub async fn build_with_working_copy(",
            "ConfigManager<WorkingCopy>",
        ),
    ] {
        let at = src
            .find(terminal)
            .unwrap_or_else(|| panic!("terminal `{terminal}` not found in builder.rs"));
        // To the opening brace, so the window is the signature however it wrapped.
        let body_at = src[at..]
            .find(" {")
            .unwrap_or_else(|| panic!("`{terminal}` has no body"));
        let signature = &src[at..at + body_at];
        assert!(
            signature.contains(expected),
            "`{terminal}` should return `{expected}` — the capability must be \
             chosen by naming the terminal, never left to inference. Saw: {signature}"
        );
    }
}
