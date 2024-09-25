use minijinja::{context, Environment, Value};

use crate::{connector::{Connector, WarehouseInfo}, yaml_parsers::{agent_parser::AgentConfig, config_parser::Warehouse}};

#[derive(Clone)]
pub struct PromptBuilder {
    config: AgentConfig,
    warehouse_info: Option<WarehouseInfo>,
}

impl PromptBuilder {
    pub fn new(config: &AgentConfig) -> Self {
        PromptBuilder { config: config.clone(), warehouse_info: None }
    }

    pub async fn setup(&mut self, warehouse: &Warehouse) {
        let connector = Connector::new(warehouse.clone());
        self.warehouse_info = Some(connector.load_warehouse_info().await);
    }

    fn render(&self, source: String, ctx: Value) -> String {
        let env = Environment::new();
        let template = env.template_from_str(&source).unwrap();
        template.render(&ctx).unwrap()
    }

    fn context(&self) -> Value {
        context! {
            warehouse => self.warehouse_info.clone(),
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
