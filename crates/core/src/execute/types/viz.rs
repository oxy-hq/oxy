use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::model::{BarChartDisplay, LineChartDisplay, PieChartDisplay};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
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
            VizParamsType::Line(l) => write!(f, "line(x={} y={} data=\"{}\")", l.x, l.y, l.data),
            VizParamsType::Bar(b) => write!(f, "bar(x={} y={} data=\"{}\")", b.x, b.y, b.data),
            VizParamsType::Pie(p) => write!(f, "pie(value={} data={})", p.value, p.data),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
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
