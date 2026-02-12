use std::path::PathBuf;
use std::time::Duration;

use assert_cmd::Command;
use assert_cmd::assert::OutputAssertExt;

#[test]
pub fn start_server_ok() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mut cmd = Command::new(oxy_test_utils::get_oxy_binary());
    cmd.current_dir(workspace_dir.join("examples"))
        .arg("serve")
        .timeout(Duration::from_secs(5))
        .assert()
        .stdout(predicates::str::contains("Web app running at"));
}
