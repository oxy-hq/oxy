use log::debug;
use minijinja::{context, Environment, Value};
use std::fs;
use std::path::PathBuf;

use crate::{
    connector::{Connector, WarehouseInfo},
    yaml_parsers::{agent_parser::AgentConfig, config_parser::Warehouse},
};

#[derive(Clone)]
pub struct PromptBuilder {
    config: AgentConfig,
    warehouse_info: Option<WarehouseInfo>,
    queries: Vec<String>,
    project_path: PathBuf,
}

impl PromptBuilder {
    pub fn new(config: &AgentConfig, project_path: &PathBuf) -> Self {
        PromptBuilder {
            config: config.clone(),
            project_path: project_path.clone(),
            warehouse_info: None,
            queries: Vec::new(),
        }
    }

    pub async fn setup(&mut self, warehouse: &Warehouse) {
        let connector = Connector::new(warehouse.clone());
        self.warehouse_info = Some(connector.load_warehouse_info().await);

        if self.config.retrieval_type == "all-shot" {
            self.load_queries();
        }
    }

    fn load_queries(&mut self) {
        let query_path = &self.project_path.join("data").join(&self.config.scope);

        debug!(
            "Query path: {}; scope: {}",
            query_path.display(),
            self.config.scope
        );

        if let Ok(entries) = fs::read_dir(query_path) {
            for entry in entries.flatten() {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    self.queries.push(content);
                }
            }
        }
    }

    fn render(&self, source: String, ctx: Value) -> String {
        let env = Environment::new();
        let template = env.template_from_str(&source).unwrap();
        template.render(&ctx).unwrap()
    }

    fn context(&self) -> Value {
        context! {
            warehouse => self.warehouse_info.clone(),
            queries => self.queries.clone(),
        }
    }

    pub fn system(&self) -> String {
        let ctx = self.context();
        let source = self.config.system_instructions.clone();
        self.render(source, ctx)
    }

    pub fn sql_tool(&self) -> String {
        let ctx = self.context();
        let source = self.config.sql_tool_instructions.clone();
        self.render(source, ctx)
    }
}
