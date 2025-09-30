use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug, JsonSchema, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    Line,
    Bar,
    Pie,
}

#[derive(Deserialize, Debug, JsonSchema, Serialize, Clone)]
pub struct AxisConfig {
    #[schemars(
        description = "Type of axis scale: 'category' for categorical data, 'value' for numerical data, 'time' for time series, 'log' for logarithmic scale"
    )]
    #[serde(rename = "type")]
    pub axis_type: String,

    #[schemars(description = "Display name for the axis, shown as axis label")]
    pub name: Option<String>,

    #[schemars(
        description = "Category data array for category axis type (e.g., ['Mon', 'Tue', 'Wed']). Not needed for value/time/log axis types"
    )]
    pub data: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize, Debug, JsonSchema, Serialize, Clone)]
pub struct SeriesConfig {
    #[schemars(description = "Display name for the series, shown in legend and tooltips")]
    pub name: Option<String>,

    #[schemars(
        description = "Chart type: 'line' for line charts, 'bar' for bar charts, 'pie' for pie charts"
    )]
    #[serde(rename = "type")]
    pub series_type: ChartType,

    #[schemars(
        description = "Data array for the series. Format depends on chart type: simple array for line/bar charts, or array of objects with name/value for pie charts"
    )]
    pub data: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize, Debug, JsonSchema, Serialize, Clone)]
pub struct VisualizeParams {
    #[schemars(
        description = "X-axis configuration for controlling scale type, labels, and category data. Not applicable for pie charts"
    )]
    #[serde(rename = "xAxis")]
    pub x_axis: Option<AxisConfig>,

    #[schemars(
        description = "Y-axis configuration for controlling scale type, labels, and formatting. Not applicable for pie charts"
    )]
    #[serde(rename = "yAxis")]
    pub y_axis: Option<AxisConfig>,

    #[schemars(
        description = "Array of data series defining chart content and chart type. Each series represents a dataset to be visualized"
    )]
    pub series: Vec<SeriesConfig>,

    #[schemars(description = "Chart title")]
    pub title: Option<String>,
}
