use assert_cmd::assert::OutputAssertExt;
use serial_test::serial;
use std::path::PathBuf;
use std::process::Command;

fn setup_command() -> Command {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mut cmd = Command::new(oxy_test_utils::get_oxy_binary());
    cmd.current_dir(workspace_dir.join("examples")).arg("build");
    cmd
}

#[test]
#[serial]
fn test_build_succeeds() {
    let mut cmd = setup_command();
    cmd.assert().success();
}

#[test]
#[serial]
fn test_build_with_drop_all_tables_flag() {
    let mut cmd = setup_command();
    cmd.arg("--drop-all-tables").assert().success();
}
