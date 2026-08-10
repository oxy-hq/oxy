use std::time::Duration;

use assert_cmd::Command;

#[test]
#[ignore = "requires a running PostgreSQL database (OXY_DATABASE_URL)"]
pub fn start_server_ok() {
    let mut cmd = Command::new(oxy_test_utils::get_oxy_binary());
    cmd.current_dir(oxy_test_utils::oxy_example_fixture_dir())
        .arg("serve")
        .timeout(Duration::from_secs(5))
        .assert()
        .stdout(predicates::str::contains("Web app running at"));
}
