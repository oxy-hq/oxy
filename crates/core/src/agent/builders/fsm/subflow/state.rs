use crate::{
    agent::builders::fsm::{
        control::TransitionContext,
        states::{data_app::DataAppState, qa::QAState, query::QueryState},
        subflow::trigger::CollectArtifact,
    },
    config::model::AppConfig,
    execute::types::Table,
};

#[derive(Debug, Clone)]
pub enum AgenticArtifactType {
    Query {
        content: String,
        tables: Vec<Table>,
    },
    QA {
        content: String,
    },
    App {
        content: String,
        app_config: AppConfig,
    },
}

#[derive(Debug, Clone)]
pub struct AgenticArtifact {
    name: String,
    artifact_type: AgenticArtifactType,
}

pub struct ArtifactsState {
    artifacts: Vec<AgenticArtifact>,
}

impl ArtifactsState {
    pub fn new() -> Self {
        Self { artifacts: vec![] }
    }

    pub fn get_artifacts(&self) -> &[AgenticArtifact] {
        &self.artifacts
    }
}

impl CollectArtifact for ArtifactsState {
    fn get_artifacts(&self) -> &[AgenticArtifact] {
        &self.artifacts
    }
    fn collect(&mut self, artifact: AgenticArtifact) {
        self.artifacts.push(artifact);
    }
}

impl From<QueryState> for AgenticArtifact {
    fn from(state: QueryState) -> Self {
        AgenticArtifact {
            name: String::new(),
            artifact_type: AgenticArtifactType::Query {
                content: state.get_content().to_string(),
                tables: state.get_tables().to_vec(),
            },
        }
    }
}

impl From<DataAppState> for AgenticArtifact {
    fn from(state: DataAppState) -> Self {
        AgenticArtifact {
            name: String::new(),
            artifact_type: AgenticArtifactType::App {
                content: state.get_content().to_string(),
                app_config: state.get_app().cloned().unwrap_or_default(),
            },
        }
    }
}

impl From<QAState> for AgenticArtifact {
    fn from(state: QAState) -> Self {
        AgenticArtifact {
            name: String::new(),
            artifact_type: AgenticArtifactType::QA {
                content: state.get_content().to_string(),
            },
        }
    }
}
