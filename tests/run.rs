#[cfg(test)]
pub mod run {
    use assert_cmd::Command;

    fn setup_command() -> Command {
        let mut cmd: Command = Command::cargo_bin("onyx").unwrap();
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
            .arg("--warehouse")
            .arg("primary_warehouse")
            .assert()
            .success();
    }

    #[test]
    fn run_sql_file_failed_if_warehouse_not_provided() {
        let mut cmd = setup_command();
        let result = cmd.arg("data/example_intervals.sql").assert().failure();
        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        assert!(output.contains("Warehouse is required for running SQL file. Please provide the warehouse using --warehouse or set a default warehouse in config.yml"));
    }

    #[test]
    fn run_sql_file_failed_if_warehouse_not_exist() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("data/example_intervals.sql")
            .arg("--warehouse")
            .arg("test")
            .assert()
            .failure();
        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        assert!(output.contains("Warehouse not found"));
    }

    #[test]
    fn run_sql_file_with_variables_ok() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("data/example_weekly_rejected.sql")
            .arg("--warehouse")
            .arg("primary_warehouse")
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
