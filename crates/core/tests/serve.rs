#[cfg(test)]
pub mod serve {
    use std::time::Duration;

    use assert_cmd::Command;

    #[test]
    pub fn start_server_ok() {
        let mut cmd: Command = Command::cargo_bin("oxy").unwrap();
        cmd.current_dir("examples")
            .arg("serve")
            .timeout(Duration::from_secs(5))
            .assert()
            .stdout(predicates::str::contains("Web app running at"));
    }
}
