use assert_cmd::assert::OutputAssertExt;
use std::path::PathBuf;
use std::process::Command;

fn setup_command() -> Command {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mut cmd = Command::new(oxy_test_utils::get_oxy_binary());
    cmd.current_dir(workspace_dir.join("examples")).arg("test");
    cmd
}

#[test]
fn test_min_accuracy_rejects_invalid_value() {
    let mut cmd = setup_command();
    let result = cmd.arg("--min-accuracy").arg("2.0").assert().failure();
    let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
    assert!(
        output.contains("min-accuracy must be between 0.0 and 1.0"),
        "Expected min-accuracy validation error, got:\n{output}"
    );
}

#[test]
fn test_agent_target_resolves() {
    // Verify that an .agent.test.yml file correctly resolves its .agent.yml target.
    // Without an API key this will fail at the LLM call stage, but should NOT fail
    // at config parsing or target resolution.
    if std::env::var("OPENAI_API_KEY").is_ok() {
        println!("Skipping test: OPENAI_API_KEY is set, would trigger real agent run");
        return;
    }

    let mut cmd = setup_command();
    let result = cmd.arg("testing/sales.agent.test.yml").assert().failure();
    let stderr = String::from_utf8(result.get_output().stderr.clone()).unwrap();
    let stdout = String::from_utf8(result.get_output().stdout.clone()).unwrap();
    let output = format!("{stderr}{stdout}");
    assert!(
        !output.contains("Could not determine target"),
        "Agent target resolution failed:\n{output}"
    );
    assert!(
        !output.contains("Unsupported target file type"),
        "Agent target type not recognized:\n{output}"
    );
    assert!(
        !output.contains("Failed to deserialize test config"),
        "Test config parsing failed:\n{output}"
    );
}

#[test]
fn test_aw_target_resolves() {
    // Verify that an .aw.test.yml file correctly resolves its .aw.yml target.
    if std::env::var("OPENAI_API_KEY").is_ok() {
        println!("Skipping test: OPENAI_API_KEY is set, would trigger real agent run");
        return;
    }

    let mut cmd = setup_command();
    let result = cmd
        .arg("agentic_workflows/build.aw.test.yml")
        .assert()
        .failure();
    let stderr = String::from_utf8(result.get_output().stderr.clone()).unwrap();
    let stdout = String::from_utf8(result.get_output().stdout.clone()).unwrap();
    let output = format!("{stderr}{stdout}");
    assert!(
        !output.contains("Could not determine target"),
        "AW target resolution failed:\n{output}"
    );
    assert!(
        !output.contains("Unsupported target file type"),
        "AW target type not recognized:\n{output}"
    );
    assert!(
        !output.contains("Failed to deserialize test config"),
        "Test config parsing failed:\n{output}"
    );
}
