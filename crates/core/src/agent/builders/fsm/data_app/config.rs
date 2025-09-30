#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DataApp {
    #[serde(default = "default_data_app_name")]
    pub name: String,
    #[serde(default = "default_data_app_description")]
    pub description: String,
}

fn default_data_app_name() -> String {
    "data_app".to_string()
}

fn default_data_app_description() -> String {
    "Collect viz and tables from the context to create data app".to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Insight {
    #[serde(default = "default_insight_name")]
    pub name: String,
    #[serde(default = "default_insight_description")]
    pub description: String,
    #[serde(default = "default_instruction")]
    pub instruction: String,
    pub model: Option<String>,
}

fn default_insight_name() -> String {
    "insight".to_string()
}

fn default_insight_description() -> String {
    "Generates insights based on the provided data and context.".to_string()
}

fn default_instruction() -> String {
    "Analyze the data and provide meaningful insights that can help in decision-making. Focus on key trends, patterns, and anomalies.".to_string()
}
