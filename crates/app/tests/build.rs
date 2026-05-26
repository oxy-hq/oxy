use assert_cmd::assert::OutputAssertExt;
use serial_test::serial;
use std::process::Command;

fn setup_command() -> Command {
    let mut cmd = Command::new(oxy_test_utils::get_oxy_binary());
    cmd.current_dir(oxy_test_utils::oxy_example_fixture_dir())
        .arg("build");
    cmd
}

#[test]
#[serial]
#[ignore = "requires a running PostgreSQL database"]
fn test_build_succeeds() {
    let mut cmd = setup_command();
    cmd.assert().success();
}

#[test]
#[serial]
#[ignore = "requires a running PostgreSQL database"]
fn test_build_with_drop_all_tables_flag() {
    let mut cmd = setup_command();
    cmd.arg("--drop-all-tables").assert().success();
}
