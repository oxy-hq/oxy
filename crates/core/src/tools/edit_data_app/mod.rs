use std::{fs, fs::File, path::PathBuf};

use crate::{
    config::model::AppConfig,
    execute::{
        Executable, ExecutionContext,
        types::{Output, event::DataApp},
    },
    observability::events,
};
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EditDataAppInput {
    pub param: EditDataAppParams,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EditDataAppParams {
    /// Optional when a data app is already associated with the current thread.
    /// Falls back to `ExecutionContext.data_app_file_path`.
    pub file_path: Option<String>,
    pub app_config: AppConfig,
}

#[derive(Debug, Clone)]
pub struct EditDataAppExecutable;

#[async_trait::async_trait]
impl Executable<EditDataAppInput> for EditDataAppExecutable {
    type Response = Output;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::EDIT_DATA_APP_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: EditDataAppInput,
    ) -> Result<Self::Response, OxyError> {
        events::tool::tool_call_input(&input);
        log::debug!("Editing data app with input: {:?}", &input);
        let EditDataAppInput { param } = input;
        let project_path = execution_context.project.config_manager.project_path();

        // Resolve file_path: use param if provided, otherwise fall back to context
        let relative_path = param
            .file_path
            .or_else(|| execution_context.data_app_file_path.clone())
            .ok_or_else(|| {
                OxyError::ArgumentError(
                    "file_path is required: either provide it in the tool parameters or ensure a data app is associated with the current thread".to_string(),
                )
            })?;

        // Validate that the file path is relative and does not contain any parent directory traversals.
        let relative_path_obj = std::path::Path::new(&relative_path);
        if relative_path_obj.is_absolute()
            || relative_path_obj
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(OxyError::ArgumentError(
                "Invalid file_path: path traversal attempt detected. Paths must be relative and must not contain '..'".to_string(),
            ));
        }
        let file_path = project_path.join(&relative_path);

        // Read and validate existing app config
        let existing_content = fs::read_to_string(&file_path).map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to read data app file '{}': {}",
                relative_path, e
            ))
        })?;
        let _existing_config: AppConfig = serde_yaml::from_str(&existing_content).map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to parse existing data app '{}': {}",
                relative_path, e
            ))
        })?;

        log::info!("Editing data app at: {}", file_path.display());
        let mut file = File::create(&file_path)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create file: {}", e)))?;
        let config = param.app_config;

        serde_yaml::to_writer(&mut file, &config)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write YAML: {}", e)))?;

        log::info!("Data app updated at: {}", file_path.display());

        execution_context
            .write_data_app(DataApp {
                file_path: PathBuf::from(relative_path.clone()),
            })
            .await?;

        let output = Output::Text(format!("Data app updated at: {}", relative_path));

        events::tool::tool_call_output(&output);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::types::Event;
    use crate::{
        adapters::project::builder::ProjectBuilder,
        execute::{ExecutionContextBuilder, types::event::Source},
    };
    use minijinja::Value;
    use oxy_test_utils::fixtures::TestFixture;
    use tokio::sync::mpsc;

    const VALID_APP_YAML: &str = r#"tasks:
  - name: my_task
    type: execute_sql
    sql_query: SELECT 1
    database: local
display:
  - type: markdown
    content: Hello World
"#;

    const UPDATED_APP_YAML: &str = r#"tasks:
  - name: updated_task
    type: execute_sql
    sql_query: SELECT 2
    database: local
display:
  - type: markdown
    content: Updated Content
"#;

    async fn build_test_context(
        fixture: &TestFixture,
        data_app_file_path: Option<String>,
    ) -> (ExecutionContext, mpsc::Receiver<Event>) {
        let project = ProjectBuilder::new(uuid::Uuid::new_v4())
            .with_project_path_and_fallback_config(fixture.path())
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        let (tx, rx) = mpsc::channel(10);
        let ctx = ExecutionContextBuilder::new()
            .with_source(Source {
                id: "test".to_string(),
                kind: "test".to_string(),
                parent_id: None,
            })
            .with_global_context(Value::UNDEFINED)
            .with_project_manager(project)
            .with_writer(tx)
            .with_data_app_file_path(data_app_file_path)
            .build()
            .unwrap();
        (ctx, rx)
    }

    fn parse_app_config(yaml: &str) -> AppConfig {
        serde_yaml::from_str(yaml).expect("valid AppConfig YAML")
    }

    #[tokio::test]
    async fn test_edits_file_via_param_file_path() {
        let fixture = TestFixture::new().unwrap();
        fixture.create_file("my.app.yml", VALID_APP_YAML).unwrap();
        let (ctx, _rx) = build_test_context(&fixture, None).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: Some("my.app.yml".to_string()),
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(result.is_ok());
        // Verify file was actually updated on disk
        let written = std::fs::read_to_string(fixture.path().join("my.app.yml")).unwrap();
        assert!(written.contains("updated_task"));
    }

    #[tokio::test]
    async fn test_output_message_contains_relative_path() {
        let fixture = TestFixture::new().unwrap();
        fixture.create_file("my.app.yml", VALID_APP_YAML).unwrap();
        let (ctx, _rx) = build_test_context(&fixture, None).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: Some("my.app.yml".to_string()),
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await.unwrap();
        if let Output::Text(msg) = result {
            assert!(msg.contains("my.app.yml"));
        } else {
            panic!("Expected Output::Text");
        }
    }

    #[tokio::test]
    async fn test_edits_file_via_context_fallback() {
        let fixture = TestFixture::new().unwrap();
        fixture.create_file("ctx.app.yml", VALID_APP_YAML).unwrap();
        let (ctx, _rx) = build_test_context(&fixture, Some("ctx.app.yml".to_string())).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: None,
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(result.is_ok());
        let written = std::fs::read_to_string(fixture.path().join("ctx.app.yml")).unwrap();
        assert!(written.contains("updated_task"));
    }

    #[tokio::test]
    async fn test_param_file_path_takes_precedence_over_context() {
        let fixture = TestFixture::new().unwrap();
        fixture
            .create_file("param.app.yml", VALID_APP_YAML)
            .unwrap();
        // context points to a non-existent file — param should win
        let (ctx, _rx) =
            build_test_context(&fixture, Some("nonexistent.app.yml".to_string())).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: Some("param.app.yml".to_string()),
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_returns_error_when_no_file_path_provided() {
        let fixture = TestFixture::new().unwrap();
        let (ctx, _rx) = build_test_context(&fixture, None).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: None,
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::ArgumentError(_))));
    }

    #[tokio::test]
    async fn test_rejects_absolute_path() {
        let fixture = TestFixture::new().unwrap();
        let (ctx, _rx) = build_test_context(&fixture, None).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: Some("/etc/passwd".to_string()),
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::ArgumentError(_))));
    }

    #[tokio::test]
    async fn test_rejects_path_traversal() {
        let fixture = TestFixture::new().unwrap();
        let (ctx, _rx) = build_test_context(&fixture, None).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: Some("../../etc/passwd".to_string()),
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::ArgumentError(_))));
    }

    #[tokio::test]
    async fn test_returns_error_when_file_not_found() {
        let fixture = TestFixture::new().unwrap();
        let (ctx, _rx) = build_test_context(&fixture, None).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: Some("nonexistent.app.yml".to_string()),
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::RuntimeError(_))));
    }

    #[tokio::test]
    async fn test_returns_error_for_invalid_existing_yaml() {
        let fixture = TestFixture::new().unwrap();
        fixture
            .create_file("bad.app.yml", "tasks: [}\ndisplay: }")
            .unwrap();
        let (ctx, _rx) = build_test_context(&fixture, None).await;
        let input = EditDataAppInput {
            param: EditDataAppParams {
                file_path: Some("bad.app.yml".to_string()),
                app_config: parse_app_config(UPDATED_APP_YAML),
            },
        };
        let result = EditDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::RuntimeError(_))));
    }
}
