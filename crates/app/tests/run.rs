use assert_cmd::assert::OutputAssertExt;
use std::process::Command;

fn setup_command() -> Command {
    let mut cmd = Command::new(oxy_test_utils::get_oxy_binary());
    cmd.current_dir(oxy_test_utils::oxy_example_fixture_dir())
        .arg("run");
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
        .arg("procedures/table_values.automation.yml")
        .assert()
        .success();
    let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();
    // The inline CLI runner prints `✓ {filename}` then per-task results;
    // the legacy "Workflow executed successfully" banner is gone.
    assert!(output.contains("✓ table_values.automation.yml"));
    assert!(output.contains("weekly"));
}

// Tests that exercised inline `type: agent` steps inside procedures
// (`run_workflow_with_anonymization_ok`, `run_workflow_with_loop_ok`) were
// retired alongside the `.agent.yml` fixtures and the classic
// `InlineAgentRunner`. The fixtures that drove them
// (`anonymize.procedure.yml`, `survey_responses.procedure.yml`, etc.)
// were removed in the same cleanup. `loop_sequential` coverage now
// lives in `run_example_workflow_ok` via `table_values.automation.yml`.
// CLI agent-execution coverage lives under `crates/agentic/pipeline/tests/`;
// `tests/fixtures/oxy_example/` carries no `.agentic.yml` fixture today.
