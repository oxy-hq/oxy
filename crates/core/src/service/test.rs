use crate::config::ConfigBuilder;
use crate::config::model::EvalKind;
use crate::execute::agent::AgentInput;
use crate::execute::core::run;
use crate::execute::eval::{
    EvalExecutor, EvalInput, EvalReceiver, Target, TargetAgent, TestStreamMessage,
};
use crate::execute::renderer::NoopRegister;
use crate::utils::find_project_path;
use async_stream::stream;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use minijinja::Value;

pub async fn run_test(pathb64: String, test_index: usize) -> impl IntoResponse {
    let decoded_path: Vec<u8> = match BASE64_STANDARD.decode(pathb64) {
        Ok(decoded_path) => decoded_path,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to decode path: {}", e)),
                    event: None,
                };
            });
        }
    };
    let path = match String::from_utf8(decoded_path) {
        Ok(path) => path,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to decode path: {}", e)),
                    event: None,
                };
            });
        }
    };

    let project_path = match find_project_path() {
        Ok(path) => path,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to find project path: {}", e)),
                    event: None,
                };
            });
        }
    };

    let config_builder = match ConfigBuilder::new().with_project_path(&project_path) {
        Ok(config_builder) => config_builder,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to build config: {}", e)),
                    event: None,
                };
            });
        }
    };

    let config = match config_builder.build().await {
        Ok(config) => config,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to build config: {}", e)),
                    event: None,
                };
            });
        }
    };

    let agent = match config.resolve_agent(&path).await {
        Ok(agent) => agent,
        Err(e) => {
            return StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to resolve agent: {}", e)),
                    event: None,
                };
            });
        }
    };

    let eval_inputs = agent
        .tests
        .clone()
        .into_iter()
        .nth(test_index)
        .map(|eval| EvalInput {
            target: Target::Agent(TargetAgent {
                agent_ref: path.clone().into(),
                input: match &eval {
                    EvalKind::Consistency(consistency) => AgentInput {
                        system_instructions: agent.system_instructions.clone(),
                        prompt: consistency.task_description.clone(),
                    },
                },
            }),
            eval,
        })
        .into_iter()
        .collect::<Vec<_>>();

    let executor = EvalExecutor;

    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let receiver = EvalReceiver::new(true, Some(tx));

    tokio::spawn(async move {
        run(
            &executor,
            eval_inputs,
            config,
            Value::UNDEFINED,
            Some(&NoopRegister),
            receiver,
        )
        .await
    });

    let stream = stream! {
        while let Some(msg) = rx.recv().await {
            yield msg;
        }
    };
    StreamBodyAs::json_nl(stream)
}
