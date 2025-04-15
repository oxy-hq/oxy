#[cfg(test)]
pub mod run {
    use assert_cmd::Command;

    fn setup_command() -> Command {
        let mut cmd: Command = Command::cargo_bin("oxy").unwrap();
        cmd.current_dir("examples").arg("run");
        cmd
    }

    #[test]
    fn run_failed_if_file_not_exist() {
        let mut cmd = setup_command();
        let result = cmd.arg("test.sql").assert().failure();
        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        assert!(output.contains("File not found"));
    }

    #[test]
    fn run_example_sql_file_ok() {
        let mut cmd = setup_command();
        cmd.arg("data/example_intervals.sql")
            .arg("--database")
            .arg("primary_database")
            .assert()
            .success();
    }

    #[test]
    fn run_sql_file_ok_if_database_not_provided_use_default_database() {
        let mut cmd = setup_command();
        cmd.arg("data/example_intervals.sql").assert().success();
    }

    #[test]
    fn run_sql_file_failed_if_database_not_exist() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("data/example_intervals.sql")
            .arg("--database")
            .arg("test")
            .assert()
            .failure();
        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        assert!(output.contains("Database 'test' not found in config"));
    }

    #[test]
    fn run_sql_file_with_variables_ok() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("data/example_weekly_rejected.sql")
            .arg("--database")
            .arg("primary_database")
            .arg("-v")
            .arg("variable_a=1")
            .arg("variable_b=testalias")
            .arg("variable_c=*")
            .assert()
            .success();
        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();
        assert!(output.contains("testalias"));
    }

    #[test]
    fn run_example_workflow_ok() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("workflows/table_values.workflow.yml")
            .assert()
            .success();
        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();
        assert!(output.contains("Workflow executed successfully"));
        assert!(output.contains("weekly"));
    }

    #[test]
    fn run_workflow_with_anonymization_ok() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("workflows/anonymize.workflow.yml")
            .assert()
            .success();
        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();
        assert!(output.contains("Workflow executed successfully"));
    }

    #[test]
    fn run_workflow_with_loop_ok() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("workflows/survey_responses.workflow.yml")
            .assert()
            .success();
        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();
        assert!(output.contains("Workflow executed successfully"));
    }

    #[test]
    fn run_agent_ok() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("agents/default.agent.yml")
            .arg("how many people are there")
            .assert()
            .success();
        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();
        assert!(output.contains("2873"));
    }

    #[test]
    fn run_agent_semantic_ok() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("agents/semantic_model.agent.yml")
            .arg("how many property_grouping")
            .assert()
            .success();
        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();
        assert!(output.contains("2"));
    }
}
