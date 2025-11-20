use crate::{
    agent::builders::fsm::viz::CollectViz,
    config::model::Display,
    execute::types::{VizParams, VizParamsType},
};

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
