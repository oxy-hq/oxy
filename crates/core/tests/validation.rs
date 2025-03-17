#[cfg(test)]
mod validation {
    use assert_cmd::Command;

    #[test]
    fn ok_on_valid_config() {
        let mut binding = Command::cargo_bin("oxy").unwrap();
        let cmd = binding.current_dir("examples").arg("validate");
        cmd.assert().success();
    }

    #[test]
    fn failed_on_invalid_config() {
        let mut binding = Command::cargo_bin("oxy").unwrap();
        let cmd = binding
            .current_dir("tests/fixtures/invalid_config")
            .arg("validate");
        cmd.assert().failure();
    }
}
