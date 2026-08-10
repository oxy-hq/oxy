#[test]
fn test_omni_query_task_yaml_parsing() {
    // Test parsing a YAML task definition with omni_query type
    let yaml_content = r#"
name: test_omni_query
type: omni_query
integration: test_integration
topic: test_topic
fields:
  - "test_view.field1"
  - "test_view.field2"
limit: 100
"#;

    let task: oxy::config::model::Task =
        serde_yaml::from_str(yaml_content).expect("Failed to parse YAML");

    // Verify the task kind is correct
    assert_eq!(task.kind(), "omni_query");
    assert_eq!(task.name, "test_omni_query");

    // Verify the task type is OmniQuery
    if let oxy::config::model::TaskType::OmniQuery(omni_task) = &task.task_type {
        assert_eq!(omni_task.integration, "test_integration");
        assert_eq!(omni_task.topic, "test_topic");
        assert_eq!(omni_task.query.fields.len(), 2);
        assert_eq!(omni_task.query.fields[0], "test_view.field1");
        assert_eq!(omni_task.query.fields[1], "test_view.field2");
        assert_eq!(omni_task.query.limit, Some(100));
    } else {
        panic!("Task type should be OmniQuery, got: {:?}", task.task_type);
    }
}
