use std::fs;

use crate::{
    config::model::AppConfig,
    execute::{Executable, ExecutionContext, types::Output},
    observability::events,
};
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadDataAppInput {
    pub param: ReadDataAppParams,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ReadDataAppParams {
    /// Optional when a data app is already associated with the current thread.
    /// Falls back to `ExecutionContext.data_app_file_path`.
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReadDataAppExecutable;

#[async_trait::async_trait]
impl Executable<ReadDataAppInput> for ReadDataAppExecutable {
    type Response = Output;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::READ_DATA_APP_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: ReadDataAppInput,
    ) -> Result<Self::Response, OxyError> {
        events::tool::tool_call_input(&input);
        log::debug!("Reading data app with input: {:?}", &input);
        let ReadDataAppInput { param } = input;
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

        let content = fs::read_to_string(&file_path).map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to read data app file '{}': {}",
                relative_path, e
            ))
        })?;

        // Validate it's a valid AppConfig
        let _config: AppConfig = serde_yaml::from_str(&content).map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to parse data app '{}': {}",
                relative_path, e
            ))
        })?;

        log::info!("Read data app from: {}", file_path.display());

        let output = Output::Text(content);
        events::tool::tool_call_output(&output);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    async fn build_test_context(
        fixture: &TestFixture,
        data_app_file_path: Option<String>,
    ) -> ExecutionContext {
        let project = ProjectBuilder::new(uuid::Uuid::new_v4())
            .with_project_path_and_fallback_config(fixture.path())
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        let (tx, _rx) = mpsc::channel(10);
        ExecutionContextBuilder::new()
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
            .unwrap()
    }

    #[tokio::test]
    async fn test_reads_file_from_param() {
        let fixture = TestFixture::new().unwrap();
        fixture.create_file("my.app.yml", VALID_APP_YAML).unwrap();
        let ctx = build_test_context(&fixture, None).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams {
                file_path: Some("my.app.yml".to_string()),
            },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(result.is_ok());
        if let Output::Text(content) = result.unwrap() {
            assert!(content.contains("execute_sql"));
        } else {
            panic!("Expected Output::Text");
        }
    }

    #[tokio::test]
    async fn test_reads_file_from_context_fallback() {
        let fixture = TestFixture::new().unwrap();
        fixture.create_file("my.app.yml", VALID_APP_YAML).unwrap();
        let ctx = build_test_context(&fixture, Some("my.app.yml".to_string())).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams { file_path: None },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_param_file_path_takes_precedence_over_context() {
        let fixture = TestFixture::new().unwrap();
        fixture
            .create_file("param.app.yml", VALID_APP_YAML)
            .unwrap();
        // context points to a non-existent file — param should win
        let ctx = build_test_context(&fixture, Some("nonexistent.app.yml".to_string())).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams {
                file_path: Some("param.app.yml".to_string()),
            },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_returns_error_when_no_file_path_provided() {
        let fixture = TestFixture::new().unwrap();
        let ctx = build_test_context(&fixture, None).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams { file_path: None },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::ArgumentError(_))));
    }

    #[tokio::test]
    async fn test_rejects_absolute_path() {
        let fixture = TestFixture::new().unwrap();
        let ctx = build_test_context(&fixture, None).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams {
                file_path: Some("/etc/passwd".to_string()),
            },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::ArgumentError(_))));
    }

    #[tokio::test]
    async fn test_rejects_path_traversal() {
        let fixture = TestFixture::new().unwrap();
        let ctx = build_test_context(&fixture, None).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams {
                file_path: Some("../../etc/passwd".to_string()),
            },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::ArgumentError(_))));
    }

    #[tokio::test]
    async fn test_returns_runtime_error_when_file_not_found() {
        let fixture = TestFixture::new().unwrap();
        let ctx = build_test_context(&fixture, None).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams {
                file_path: Some("nonexistent.app.yml".to_string()),
            },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::RuntimeError(_))));
    }

    #[tokio::test]
    async fn test_returns_runtime_error_for_invalid_yaml() {
        let fixture = TestFixture::new().unwrap();
        fixture
            .create_file("bad.app.yml", "tasks: [}\ndisplay: }")
            .unwrap();
        let ctx = build_test_context(&fixture, None).await;
        let input = ReadDataAppInput {
            param: ReadDataAppParams {
                file_path: Some("bad.app.yml".to_string()),
            },
        };
        let result = ReadDataAppExecutable.execute(&ctx, input).await;
        assert!(matches!(result, Err(OxyError::RuntimeError(_))));
    }
}
