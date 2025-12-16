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

impl VizParams {
    pub fn new(name: String, title: String, config: VizParamsType) -> Self {
        Self {
            name,
            title,
            config,
        }
    }

    pub fn with_data_path(self, data_path: &str) -> Self {
        let config = match self.config {
            VizParamsType::Line(mut l) => {
                l.data = data_path.to_string();
                VizParamsType::Line(l)
            }
            VizParamsType::Bar(mut b) => {
                b.data = data_path.to_string();
                VizParamsType::Bar(b)
            }
            VizParamsType::Pie(mut p) => {
                p.data = data_path.to_string();
                VizParamsType::Pie(p)
            }
        };
        Self {
            name: self.name,
            title: self.title,
            config,
        }
    }

    pub fn data_slug(&self) -> String {
        slugify::slugify(&self.data(), "", "-", None)
    }

    fn data(&self) -> &str {
        match &self.config {
            VizParamsType::Line(l) => &l.data,
            VizParamsType::Bar(b) => &b.data,
            VizParamsType::Pie(p) => &p.data,
        }
    }
}

impl std::fmt::Display for VizParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        tracing::info!("Formatting VizParams for display {:?}", self);
        write!(
            f,
            "Visualization: {}\n=============\nTitle: {}\nConfig: {}",
            self.name, self.title, self.config
        )
    }
}
