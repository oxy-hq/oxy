use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    agent::builders::fsm::viz::CollectViz,
    config::model::{BarChartDisplay, Display, LineChartDisplay, PieChartDisplay},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum VizParamsType {
    Line(LineChartDisplay),
    Bar(BarChartDisplay),
    Pie(PieChartDisplay),
}

impl std::fmt::Display for VizParamsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VizParamsType::Line(_) => write!(f, "line"),
            VizParamsType::Bar(_) => write!(f, "bar"),
            VizParamsType::Pie(_) => write!(f, "pie"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VizParams {
    pub name: String,
    pub title: String,
    pub config: VizParamsType,
}

impl std::fmt::Display for VizParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Visualization: {}\nTitle: {}\nType: {}",
            self.name, self.title, self.config
        )
    }
}

impl From<VizParams> for Display {
    fn from(val: VizParams) -> Self {
        match val.config {
            VizParamsType::Line(config) => Display::LineChart(config),
            VizParamsType::Bar(config) => Display::BarChart(config),
            VizParamsType::Pie(config) => Display::PieChart(config),
        }
    }
}

pub struct VizState {
    pub visualizations: Vec<VizParams>,
}

impl VizState {
    pub fn new() -> Self {
        Self {
            visualizations: vec![],
        }
    }
}

impl CollectViz for VizState {
    fn list_viz(&self) -> &[VizParams] {
        &self.visualizations
    }

    fn collect_viz(&mut self, viz: VizParams) {
        self.visualizations.push(viz);
    }
}
