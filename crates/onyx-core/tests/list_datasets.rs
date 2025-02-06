#[cfg(test)]
pub mod list_datasets {
    use assert_cmd::Command;

    #[test]
    fn list_datasets_ok() {
        let mut binding = Command::cargo_bin("onyx").unwrap();
        let cmd = binding.current_dir("examples").arg("list-datasets");
        let result = cmd.assert().success();
        let output = result.get_output();
        let stdout = String::from_utf8(output.stdout.clone()).unwrap();
        assert!(stdout.contains("dbt_prod"));
        assert!(stdout.contains("segment_marketing_web_prod"));
    }
}
