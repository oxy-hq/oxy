use assert_cmd::assert::OutputAssertExt;
use std::path::PathBuf;
use std::process::Command;

fn get_oxy_binary() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    // Try llvm-cov target first, fall back to regular debug target
    let mut bin_path = workspace_dir.join("target/llvm-cov-target/debug/oxy");
    if !bin_path.exists() {
        bin_path = workspace_dir.join("target/debug/oxy");
    }
    bin_path
}

fn setup_command() -> Command {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mut cmd = Command::new(get_oxy_binary());
    cmd.current_dir(workspace_dir.join("examples")).arg("run");
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
    // Skip test if OPENAI_API_KEY is not set
    if std::env::var("OPENAI_API_KEY").is_err() {
        println!("Skipping test: OPENAI_API_KEY not set");
        return;
    }

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
    // Skip test if OPENAI_API_KEY is not set
    if std::env::var("OPENAI_API_KEY").is_err() {
        println!("Skipping test: OPENAI_API_KEY not set");
        return;
    }

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
    // Skip test if OPENAI_API_KEY is not set
    if std::env::var("OPENAI_API_KEY").is_err() {
        println!("Skipping test: OPENAI_API_KEY not set");
        return;
    }

    let mut cmd = setup_command();
    let result = cmd
        .arg("agents/default.agent.yml")
        .arg("how many people are there")
        .assert()
        .success();
    let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();

    // The agent queries dim_users table in BigQuery which has ~2873 users
    // Accept the number with or without comma formatting since LLM responses vary
    // Examples: "2873", "2,873", "2 873"
    let has_count = output.contains("2873") || output.contains("2,873") || output.contains("2 873");

    assert!(
        has_count,
        "Expected output to contain user count '2873' (with or without formatting), but got:\n{}",
        output
    );
}
