use assert_cmd::Command;
use serial_test::serial;
use std::path::PathBuf;

fn setup_command() -> Command {
    let mut cmd: Command = Command::cargo_bin("oxy").unwrap();
    cmd.current_dir("examples").arg("build");
    cmd
}

#[test]
#[serial]
fn test_build_creates_semantics_output() {
    let mut cmd = setup_command();
    cmd.assert().success();

    // Verify the .semantics directory exists after build
    let examples_dir = PathBuf::from("examples");
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
