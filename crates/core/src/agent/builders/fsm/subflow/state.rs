use crate::{
    agent::builders::fsm::{
        control::TransitionContext,
        query::PrepareData,
        states::{data_app::DataAppState, qa::QAState, query::QueryState},
        subflow::trigger::CollectArtifact,
    },
    config::model::AppConfig,
    execute::types::{Table, VizParams},
};

#[derive(Debug, Clone)]
pub enum AgenticArtifactType {
    Viz {
        description: String,
        params: VizParams,
    },
    Dataset {
        description: String,
        tables: Vec<Table>,
    },
    App {
        description: String,
        app_config: AppConfig,
    },
    QA {
        question: String,
        response: String,
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
            artifact_type: AgenticArtifactType::Dataset {
                description: state.get_intent().to_string(),
                tables: state.get_tables().into_iter().cloned().collect(),
            },
        }
    }
}

impl From<DataAppState> for AgenticArtifact {
    fn from(state: DataAppState) -> Self {
        AgenticArtifact {
            name: String::new(),
            artifact_type: AgenticArtifactType::App {
                description: state.get_intent().to_string(),
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
                question: state.get_intent().to_string(),
                response: state.get_content().to_string(),
            },
        }
    }
}
