use oxy_semantic::variables::{RuntimeVariableResolver, VariableEncoder, VariableResolver};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_complete_variable_workflow_encoding_to_resolution() {
    // Phase 1: Encode variables in semantic expressions
    let mut encoder = VariableEncoder::new();
    let original_expr = "SELECT * FROM {{variables.schema}}.{{variables.table}}";
    let encoded_expr = encoder.encode_expression(original_expr);

    // Verify encoding produces valid SQL identifiers
    assert!(encoded_expr.starts_with("SELECT * FROM __VAR_"));
    assert!(!encoded_expr.contains("{{"));

    // Phase 2: Decode back to template format
    let decoded_expr = encoder.decode_expression(&encoded_expr).unwrap();
    assert_eq!(decoded_expr, original_expr);

    // Phase 3: Resolve variables with runtime context
    let context = json!({
        "variables": {
            "schema": "public",
            "table": "users"
        }
    });

    let resolver = RuntimeVariableResolver::new(context).unwrap();
    let resolved = resolver.resolve_expression(&decoded_expr).unwrap();
    assert_eq!(resolved, "SELECT * FROM public.users");
}

#[test]
fn test_multiple_variable_sources_with_precedence() {
    // Setup different variable sources
    let env_vars = HashMap::from([("table".to_string(), json!("env_table"))]);

    let globals = HashMap::from([
        ("table".to_string(), json!("global_table")),
        ("schema".to_string(), json!("global_schema")),
    ]);

    let agent_vars = HashMap::from([
        ("table".to_string(), json!("agent_table")),
        ("column".to_string(), json!("agent_column")),
    ]);

    let workflow_vars = HashMap::from([
        ("table".to_string(), json!("workflow_table")),
        ("limit".to_string(), json!(100)),
    ]);

    // Create resolver with all sources
    let resolver = RuntimeVariableResolver::from_sources(
        Some(workflow_vars),
        Some(agent_vars),
        Some(globals),
        Some(env_vars),
    )
    .unwrap();

    // Verify precedence: workflow > agent > global > env
    assert_eq!(
        resolver.resolve_variable("table").unwrap(),
        json!("workflow_table")
    );
    assert_eq!(
        resolver.resolve_variable("column").unwrap(),
        json!("agent_column")
    );
    assert_eq!(
        resolver.resolve_variable("schema").unwrap(),
        json!("global_schema")
    );
    assert_eq!(resolver.resolve_variable("limit").unwrap(), json!(100));
}

#[test]
fn test_complex_expression_with_nested_variables() {
    let mut encoder = VariableEncoder::new();

    let expr = "SELECT {{variables.id_col}}, {{variables.name_col}} FROM {{variables.db.schema}}.{{variables.db.table}} WHERE {{variables.filter_col}} = 'active'";
    let encoded = encoder.encode_expression(expr);

    // Verify all variables are encoded
    assert!(!encoded.contains("{{"));
    assert!(encoded.contains("__VAR_"));

    let decoded = encoder.decode_expression(&encoded).unwrap();

    let context = json!({
        "variables": {
            "id_col": "user_id",
            "name_col": "full_name",
            "filter_col": "status",
            "db": {
                "schema": "analytics",
                "table": "users"
            }
        }
    });

    let resolver = RuntimeVariableResolver::new(context).unwrap();
    let resolved = resolver.resolve_expression(&decoded).unwrap();

    assert_eq!(
        resolved,
        "SELECT user_id, full_name FROM analytics.users WHERE status = 'active'"
    );
}

#[test]
fn test_variable_resolution_with_sql_encoding() {
    let context = json!({
        "variables": {
            "table_name": "orders",
            "date_column": "created_at"
        }
    });

    let resolver = RuntimeVariableResolver::new(context).unwrap();

    // Create encoder and encode variables, then decode for resolver
    let mut encoder = VariableEncoder::new();
    let original_sql = "SELECT {{variables.date_column}} FROM {{variables.table_name}}";
    let encoded_sql = encoder.encode_expression(original_sql);

    // Verify encoding happened
    assert!(encoded_sql.contains("__VAR_"));
    assert!(!encoded_sql.contains("{{"));

    let resolved = resolver.resolve_sql_variables(encoded_sql).unwrap();
    assert_eq!(resolved, "SELECT created_at FROM orders");
}

#[test]
fn test_error_handling_for_missing_variables() {
    let context = json!({
        "variables": {
            "existing": "value"
        }
    });

    let resolver = RuntimeVariableResolver::new(context).unwrap();

    // Test missing variable
    let result = resolver.resolve_variable("missing");
    assert!(result.is_err());
}

#[test]
fn test_variable_resolution_with_filters() {
    let context = json!({
        "variables": {
            "env": "prod",
            "region": "US"
        }
    });

    let resolver = RuntimeVariableResolver::new(context).unwrap();

    // Test simple interpolation
    let expr = "WHERE region = '{{variables.region}}'";
    let resolved = resolver.resolve_expression(expr).unwrap();
    assert_eq!(resolved, "WHERE region = 'US'");

    // Test another simple variable
    let expr = "environment: {{variables.env}}";
    let resolved = resolver.resolve_expression(expr).unwrap();
    assert_eq!(resolved, "environment: prod");
}

#[test]
fn test_variable_extraction_from_expressions() {
    let encoder = VariableEncoder::new();

    let expr = "SELECT {{variables.col1}}, {{variables.col2}} FROM {{variables.table}}";
    let vars = encoder.extract_variables(expr);

    assert_eq!(vars.len(), 3);
    assert!(vars.contains(&"col1".to_string()));
    assert!(vars.contains(&"col2".to_string()));
    assert!(vars.contains(&"table".to_string()));
}

#[test]
fn test_encoding_preserves_non_variable_content() {
    let mut encoder = VariableEncoder::new();

    let expr = "SELECT id, name FROM {{variables.table}} WHERE status = 'active' AND created_at > '2024-01-01'";
    let encoded = encoder.encode_expression(expr);
    let decoded = encoder.decode_expression(&encoded).unwrap();

    // Non-variable content should be preserved exactly
    assert!(decoded.contains("SELECT id, name FROM"));
    assert!(decoded.contains("WHERE status = 'active'"));
    assert!(decoded.contains("AND created_at > '2024-01-01'"));
}

#[test]
fn test_empty_context_with_no_variables() {
    let resolver = RuntimeVariableResolver::new(json!({})).unwrap();

    // Expression without variables should pass through
    let expr = "SELECT * FROM users";
    let resolved = resolver.resolve_expression(expr).unwrap();
    assert_eq!(resolved, expr);
}

#[test]
fn test_variable_encoding_with_special_characters() {
    let mut encoder = VariableEncoder::new();

    // Test underscores in variable names
    let expr = "{{variables.user_table}}";
    let encoded = encoder.encode_expression(expr);
    let decoded = encoder.decode_expression(&encoded).unwrap();
    assert_eq!(decoded, expr);

    // Test dots in nested paths
    let expr = "{{variables.db.schema.table}}";
    let encoded = encoder.encode_expression(expr);
    let decoded = encoder.decode_expression(&encoded).unwrap();
    assert_eq!(decoded, expr);
}

#[test]
fn test_decode_all_variables_in_sql() {
    let mut encoder = VariableEncoder::new();

    let original = "SELECT {{variables.col1}}, {{variables.col2}} FROM {{variables.table}}";
    let encoded = encoder.encode_expression(original);

    // Test decoding all at once
    let decoded = encoder.decode_all_variables(&encoded);
    assert_eq!(decoded, original);
}

#[test]
fn test_variable_resolver_with_numeric_and_boolean_values() {
    let context = json!({
        "variables": {
            "limit": 100,
            "offset": 50,
            "active": true,
            "ratio": 0.95
        }
    });

    let resolver = RuntimeVariableResolver::new(context).unwrap();

    assert_eq!(resolver.resolve_variable("limit").unwrap(), json!(100));
    assert_eq!(resolver.resolve_variable("offset").unwrap(), json!(50));
    assert_eq!(resolver.resolve_variable("active").unwrap(), json!(true));
    assert_eq!(resolver.resolve_variable("ratio").unwrap(), json!(0.95));
}

#[test]
fn test_has_variables_detection() {
    let encoder = VariableEncoder::new();

    assert!(encoder.has_variables("{{variables.test}}"));
    assert!(encoder.has_variables("SELECT * FROM {{variables.table}}"));
    assert!(encoder.has_variables("{{variables.a}} and {{variables.b}}"));
    assert!(!encoder.has_variables("SELECT * FROM users"));
    assert!(!encoder.has_variables(""));
}

#[test]
fn test_encoder_idempotency() {
    let mut encoder = VariableEncoder::new();

    let original = "{{variables.test}}";
    let encoded1 = encoder.encode_expression(original);
    let encoded2 = encoder.encode_expression(original);

    // Same input should produce same output
    assert_eq!(encoded1, encoded2);
}

#[test]
fn test_multiple_occurrences_of_same_variable() {
    let mut encoder = VariableEncoder::new();

    let expr = "SELECT {{variables.col}}, {{variables.col}} FROM {{variables.table}}";
    let encoded = encoder.encode_expression(expr);
    let decoded = encoder.decode_expression(&encoded).unwrap();

    assert_eq!(decoded, expr);

    let context = json!({
        "variables": {
            "col": "id",
            "table": "users"
        }
    });

    let resolver = RuntimeVariableResolver::new(context).unwrap();
    let resolved = resolver.resolve_expression(&decoded).unwrap();
    assert_eq!(resolved, "SELECT id, id FROM users");
}
