use assert_cmd::assert::OutputAssertExt;
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

#[test]
fn ok_on_valid_config() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mut cmd = Command::new(get_oxy_binary());
    cmd.current_dir(workspace_dir.join("examples"))
        .arg("validate");
    cmd.assert().success();
}

#[test]
fn failed_on_invalid_config() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mut cmd = Command::new(get_oxy_binary());
    cmd.current_dir(workspace_dir.join("crates/core/tests/fixtures/invalid_config"))
        .arg("validate");
    cmd.assert().failure();
}
