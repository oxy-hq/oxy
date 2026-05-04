use super::*;
use std::fs;
use std::path::Path;

fn write_temp(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
}

// ── llm.ref parsing ───────────────────────────────────────────────────────

#[test]
fn llm_ref_parses_correctly() {
    let yaml = "llm:\n  ref: openai-4o-mini\n";
    let config = AgentConfig::from_yaml(yaml).unwrap();
    assert_eq!(config.llm.model_ref.as_deref(), Some("openai-4o-mini"));
    assert!(config.llm.model.is_none());
}

#[test]
fn llm_ref_with_model_override() {
    let yaml = "llm:\n  ref: openai-4o-mini\n  model: gpt-5.4\n";
    let config = AgentConfig::from_yaml(yaml).unwrap();
    assert_eq!(config.llm.model_ref.as_deref(), Some("openai-4o-mini"));
    assert_eq!(config.llm.model.as_deref(), Some("gpt-5.4"));
}

#[test]
fn llm_ref_absent_by_default() {
    let yaml = "llm:\n  model: gpt-4\n";
    let config = AgentConfig::from_yaml(yaml).unwrap();
    assert!(config.llm.model_ref.is_none());
}

#[test]
fn llm_ref_empty_config_defaults() {
    let config = AgentConfig::from_yaml("{}").unwrap();
    assert!(config.llm.model_ref.is_none());
    assert!(config.llm.model.is_none());
}

// ── extract_procedure_databases ───────────────────────────────────────────

#[test]
fn procedure_databases_flat_tasks() {
    let yaml = r#"
name: my_proc
tasks:
  - name: q1
    type: execute_sql
    database: warehouse
    sql_query: SELECT 1
  - name: q2
    type: execute_sql
    database: staging
    sql_query: SELECT 2
"#;
    let mut dbs = extract_procedure_databases(yaml);
    dbs.sort();
    assert_eq!(dbs, vec!["staging", "warehouse"]);
}

#[test]
fn procedure_databases_nested_loop_sequential() {
    let yaml = r#"
name: my_proc
tasks:
  - name: loop_step
    type: loop_sequential
    values: [1, 2, 3]
    tasks:
      - name: inner_query
        type: execute_sql
        database: local
        sql_query: SELECT 1
"#;
    let dbs = extract_procedure_databases(yaml);
    assert_eq!(dbs, vec!["local"]);
}

#[test]
fn procedure_databases_deduplication() {
    let yaml = r#"
name: my_proc
tasks:
  - name: q1
    type: execute_sql
    database: local
    sql_query: SELECT 1
  - name: loop_step
    type: loop_sequential
    values: [1, 2]
    tasks:
      - name: q2
        type: execute_sql
        database: local
        sql_query: SELECT 2
"#;
    let dbs = extract_procedure_databases(yaml);
    assert_eq!(dbs, vec!["local"]);
}

#[test]
fn procedure_databases_no_execute_sql() {
    let yaml = r#"
name: my_proc
tasks:
  - name: fmt
    type: formatter
    template: "hello"
"#;
    let dbs = extract_procedure_databases(yaml);
    assert!(dbs.is_empty());
}

#[test]
fn procedure_databases_multiple_nested_levels() {
    // database appears at top-level task and inside a nested loop
    let yaml = r#"
name: p
tasks:
  - name: top
    type: execute_sql
    database: alpha
    sql_query: SELECT 1
  - name: outer_loop
    type: loop_sequential
    values: [1]
    tasks:
      - name: inner_loop
        type: loop_sequential
        values: [1]
        tasks:
          - name: deep
            type: execute_sql
            database: beta
            sql_query: SELECT 2
"#;
    let mut dbs = extract_procedure_databases(yaml);
    dbs.sort();
    assert_eq!(dbs, vec!["alpha", "beta"]);
}

// ── parse_oxy_comment_block ───────────────────────────────────────────────

#[test]
fn sql_oxy_database_present() {
    let sql = "/*\n  oxy:\n    database: local\n    embed:\n      - How many stores\n*/\nSELECT COUNT(*) FROM stores;";
    let block = parse_oxy_comment_block(sql).unwrap();
    assert_eq!(block.database, Some("local".to_string()));
}

#[test]
fn sql_oxy_no_comment() {
    assert!(parse_oxy_comment_block("SELECT 1;").is_none());
}

#[test]
fn sql_oxy_comment_without_database() {
    let sql = "/*\n  oxy:\n    embed:\n      - How many stores\n*/\nSELECT 1;";
    assert!(
        parse_oxy_comment_block(sql)
            .and_then(|b| b.database)
            .is_none()
    );
}

#[test]
fn sql_oxy_description_and_database() {
    let sql =
        "/*\n  oxy:\n    description: \"Daily revenue\"\n    database: warehouse\n*/\nSELECT 1;";
    let block = parse_oxy_comment_block(sql).unwrap();
    assert_eq!(block.description, Some("Daily revenue".to_string()));
    assert_eq!(block.database, Some("warehouse".to_string()));
}

// ── resolve_context — database inference ──────────────────────────────────

#[test]
fn resolve_context_infers_db_from_sql_oxy_comment() {
    let tmp = std::env::temp_dir().join("oxy_cfg_test_sql");
    fs::create_dir_all(&tmp).unwrap();
    write_temp(
        &tmp,
        "q.sql",
        "/*\n  oxy:\n    database: analytics\n*/\nSELECT 1;",
    );

    let config = AgentConfig::from_yaml("context:\n  - '*.sql'\n").unwrap();
    let ctx = config.resolve_context(&tmp).unwrap();

    assert!(
        ctx.referenced_databases.contains(&"analytics".to_string()),
        "expected 'analytics' in referenced_databases, got: {:?}",
        ctx.referenced_databases
    );

    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn resolve_context_infers_db_from_procedure_file() {
    let tmp = std::env::temp_dir().join("oxy_cfg_test_proc");
    fs::create_dir_all(&tmp).unwrap();
    write_temp(
        &tmp,
        "my.procedure.yml",
        "name: p\ntasks:\n  - name: q\n    type: execute_sql\n    database: warehouse\n    sql_query: SELECT 1\n",
    );

    let config = AgentConfig::from_yaml("context:\n  - '*.procedure.yml'\n").unwrap();
    let ctx = config.resolve_context(&tmp).unwrap();

    assert!(
        ctx.referenced_databases.contains(&"warehouse".to_string()),
        "expected 'warehouse' in referenced_databases, got: {:?}",
        ctx.referenced_databases
    );
    assert_eq!(ctx.procedure_files.len(), 1);

    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn resolve_context_infers_db_from_nested_procedure_loop() {
    let tmp = std::env::temp_dir().join("oxy_cfg_test_nested");
    fs::create_dir_all(&tmp).unwrap();
    write_temp(
        &tmp,
        "deep.procedure.yml",
        "name: p\ntasks:\n  - name: outer\n    type: loop_sequential\n    values: [1]\n    tasks:\n      - name: q\n        type: execute_sql\n        database: remote\n        sql_query: SELECT 1\n",
    );

    let config = AgentConfig::from_yaml("context:\n  - '*.procedure.yml'\n").unwrap();
    let ctx = config.resolve_context(&tmp).unwrap();

    assert!(
        ctx.referenced_databases.contains(&"remote".to_string()),
        "expected 'remote', got: {:?}",
        ctx.referenced_databases
    );

    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn resolve_context_deduplicates_databases_across_files() {
    let tmp = std::env::temp_dir().join("oxy_cfg_test_dedup");
    fs::create_dir_all(&tmp).unwrap();
    write_temp(
        &tmp,
        "q1.sql",
        "/*\n  oxy:\n    database: local\n*/\nSELECT 1;",
    );
    write_temp(
        &tmp,
        "q2.sql",
        "/*\n  oxy:\n    database: local\n*/\nSELECT 2;",
    );
    write_temp(
        &tmp,
        "proc.procedure.yml",
        "name: p\ntasks:\n  - name: q\n    type: execute_sql\n    database: local\n    sql_query: SELECT 3\n",
    );

    let config = AgentConfig::from_yaml("context:\n  - '*.sql'\n  - '*.procedure.yml'\n").unwrap();
    let ctx = config.resolve_context(&tmp).unwrap();

    let count = ctx
        .referenced_databases
        .iter()
        .filter(|d| *d == "local")
        .count();
    assert_eq!(
        count, 1,
        "database names should be deduplicated; got: {:?}",
        ctx.referenced_databases
    );

    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn resolve_context_merges_databases_from_sql_and_procedure() {
    let tmp = std::env::temp_dir().join("oxy_cfg_test_merge");
    fs::create_dir_all(&tmp).unwrap();
    write_temp(
        &tmp,
        "q.sql",
        "/*\n  oxy:\n    database: alpha\n*/\nSELECT 1;",
    );
    write_temp(
        &tmp,
        "proc.procedure.yml",
        "name: p\ntasks:\n  - name: q\n    type: execute_sql\n    database: beta\n    sql_query: SELECT 1\n",
    );

    let config = AgentConfig::from_yaml("context:\n  - '*.sql'\n  - '*.procedure.yml'\n").unwrap();
    let ctx = config.resolve_context(&tmp).unwrap();

    let mut dbs = ctx.referenced_databases.clone();
    dbs.sort();
    assert_eq!(dbs, vec!["alpha", "beta"]);

    fs::remove_dir_all(&tmp).ok();
}

// ── extended_thinking config parsing ────────────────────────────────────

#[test]
fn extended_thinking_config_parses_anthropic() {
    let yaml = r#"
llm:
  ref: claude-sonnet-4-6
  thinking: adaptive
  extended_thinking:
    model: claude-opus-4-6
    thinking: adaptive
"#;
    let config = AgentConfig::from_yaml(yaml).unwrap();
    let et = config.llm.extended_thinking.unwrap();
    assert_eq!(et.model.as_deref(), Some("claude-opus-4-6"));
    assert!(matches!(
        et.thinking.unwrap().to_thinking_config(),
        ThinkingConfig::Adaptive
    ));
}

#[test]
fn extended_thinking_config_parses_openai_effort() {
    let yaml = r#"
llm:
  ref: openai
  model: gpt-5.4
  thinking: effort::low
  extended_thinking:
    model: gpt-5.4
    thinking: effort::medium
"#;
    let config = AgentConfig::from_yaml(yaml).unwrap();
    let et = config.llm.extended_thinking.unwrap();
    assert!(matches!(
        et.thinking.unwrap().to_thinking_config(),
        ThinkingConfig::Effort(ReasoningEffort::Medium)
    ));
}

#[test]
fn thinking_in_llm_takes_precedence_over_top_level() {
    let yaml = r#"
thinking: disabled
llm:
  thinking: adaptive
"#;
    let config = AgentConfig::from_yaml(yaml).unwrap();
    // llm.thinking should take precedence over top-level thinking
    let effective = config.llm.thinking.as_ref().or(config.thinking.as_ref());
    assert!(matches!(
        effective.unwrap().to_thinking_config(),
        ThinkingConfig::Adaptive
    ));
}

#[test]
fn top_level_thinking_used_when_llm_thinking_absent() {
    let yaml = "thinking: adaptive\nllm:\n  model: test\n";
    let config = AgentConfig::from_yaml(yaml).unwrap();
    assert!(config.llm.thinking.is_none());
    let effective = config.llm.thinking.as_ref().or(config.thinking.as_ref());
    assert!(matches!(
        effective.unwrap().to_thinking_config(),
        ThinkingConfig::Adaptive
    ));
}

#[test]
fn extended_thinking_absent_when_not_configured() {
    let config = AgentConfig::from_yaml("{}").unwrap();
    assert!(config.llm.extended_thinking.is_none());
}

#[test]
fn extended_thinking_with_only_model_override() {
    let yaml = r#"
llm:
  extended_thinking:
    model: claude-opus-4-6
"#;
    let config = AgentConfig::from_yaml(yaml).unwrap();
    let et = config.llm.extended_thinking.unwrap();
    assert_eq!(et.model.as_deref(), Some("claude-opus-4-6"));
    assert!(et.thinking.is_none());
}

#[test]
fn extended_thinking_with_only_thinking_override() {
    let yaml = r#"
llm:
  extended_thinking:
    thinking: effort::high
"#;
    let config = AgentConfig::from_yaml(yaml).unwrap();
    let et = config.llm.extended_thinking.unwrap();
    assert!(et.model.is_none());
    assert!(matches!(
        et.thinking.unwrap().to_thinking_config(),
        ThinkingConfig::Effort(ReasoningEffort::High)
    ));
}
