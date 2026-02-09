use assert_cmd::assert::OutputAssertExt;
use serial_test::serial;
use std::path::PathBuf;
use std::process::Command;

fn get_oxy_binary() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    // Try llvm-cov target first, fall back to regular debug target
    let mut bin_path = workspace_dir.join("target/llvm-cov-target/debug/oxy");
    if !bin_path.exists() {
        bin_path = workspace_dir.join("target/debug/oxy");
    }
    bin_path
}

fn setup_command() -> Command {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mut cmd = Command::new(get_oxy_binary());
    cmd.current_dir(workspace_dir.join("examples")).arg("build");
    cmd
}

#[test]
#[serial]
fn test_build_creates_semantics_output() {
    let mut cmd = setup_command();
    cmd.assert().success();

    // Verify the .semantics directory exists after build
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let examples_dir = workspace_dir.join("examples");
    let semantics_dir = examples_dir.join(".semantics");

    assert!(
        semantics_dir.exists(),
        "Semantics output directory was not created"
    );
}

#[test]
#[serial]
fn test_build_with_drop_all_tables_flag() {
    let mut cmd = setup_command();
    cmd.arg("--drop-all-tables").assert().success();
}
