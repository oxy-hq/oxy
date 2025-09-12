use std::{collections::HashMap, fs, path::PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{errors::OxyError, utils::list_by_sub_extension};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AggregateType {
    #[serde(rename = "count")]
    Count,
    #[serde(rename = "sum")]
    Sum,
    #[serde(rename = "average")]
    Avg,
    #[serde(rename = "min")]
    Min,
    #[serde(rename = "max")]
    Max,
    #[serde(rename = "average_distinct_on")]
    AverageDistinctOn,
    #[serde(rename = "median_distinct_on")]
    MedianDistinctOn,
    #[serde(rename = "count_distinct")]
    CountDistinct,
    #[serde(rename = "sum_distinct_on")]
    SumDistinctOn,
    #[serde(rename = "median")]
    Median,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(untagged)]
pub enum OmniFilter {
    Is(OmniIsFilter),
    Not(OmniNotFilter),
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OmniIsFilter {
    pub is: Option<OmniFilterValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OmniNotFilter {
    pub not: Option<OmniFilterValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(untagged)]
pub enum OmniFilterValue {
    String(String),
    Array(Vec<OmniFilterValue>),
    Int(i64),
    Bool(bool),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniMeasure {
    pub sql: Option<String>,
    pub description: Option<String>,
    pub aggregate_type: Option<AggregateType>,
    pub filters: Option<HashMap<String, OmniFilter>>,
    pub custom_primary_key_sql: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniDimension {
    pub sql: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniView {
    pub schema: String,
    #[serde(flatten)]
    pub view_type: OmniViewType,
    pub dimensions: HashMap<String, OmniDimension>,
    #[serde(default)]
    pub measures: HashMap<String, OmniMeasure>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum OmniViewType {
    Table(OmniTableView),
    Query(OmniQueryView),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniTableView {
    pub table_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniQueryView {
    pub sql: String,
}

impl OmniView {
    pub fn get_field(&self, field_name: &str) -> Option<OmniField> {
        if let Some(dimension) = self.dimensions.get(field_name) {
            return Some(OmniField::Dimension(dimension.clone()));
        }
        if let Some(measure) = self.measures.get(field_name) {
            return Some(OmniField::Measure(measure.clone()));
        }
        None
    }

    pub fn get_full_field_name(&self, field_name: &str, view_name: &str) -> String {
        match self.view_type.clone() {
            OmniViewType::Table(v) => {
                format!("{}.{}", v.table_name, field_name)
            }
            OmniViewType::Query(_) => {
                format!("{view_name}.{field_name}")
            }
        }
    }

    pub fn get_all_fields(&self) -> HashMap<String, OmniField> {
        let mut fields = HashMap::new();
        for (name, dimension) in &self.dimensions {
            fields.insert(name.to_string(), OmniField::Dimension(dimension.clone()));
        }
        for (name, measure) in &self.measures {
            fields.insert(name.to_string(), OmniField::Measure(measure.clone()));
        }
        fields
    }

    pub fn get_table_name(&self, view_name: &str) -> String {
        match &self.view_type {
            OmniViewType::Table(view) => {
                format!("{}.{}", self.schema, view.table_name)
            }
            OmniViewType::Query(v) => {
                format!("({}) as {}", v.sql, view_name)
            }
        }
    }
}
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct OmniTopicJoinItem {}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct OmniTopic {
    pub base_view: String,
    pub label: Option<String>,
    pub fields: Vec<String>,
    pub joins: HashMap<String, OmniTopicJoinItem>,
    pub default_filters: Option<HashMap<String, OmniFilter>>,
}

impl OmniTopic {
    pub fn get_pattern_priority(&self, pattern: &str) -> u8 {
        if "all_views.*" == pattern {
            return 1;
        }
        if pattern.starts_with("tag:") {
            return 3;
        }
        let parts = pattern.split('.').collect::<Vec<_>>();
        if parts.len() == 2 && parts[1] == "*" {
            return 2;
        }

        4
    }
    pub fn get_sorted_field_patterns(&self) -> Vec<String> {
        let mut sorted_fields = self.fields.clone();

        sorted_fields.sort_by(|a, b| {
            let a_priority = self.get_pattern_priority(a);
            let b_priority = self.get_pattern_priority(b);
            a_priority.cmp(&b_priority)
        });
        sorted_fields
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniSemanticModel {
    pub views: HashMap<String, OmniView>,
    pub topics: HashMap<String, OmniTopic>,
    pub relationships: Vec<OmniRelationShip>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct ExecuteOmniTool {
    pub name: String,
    #[serde(default = "default_omni_tool_description")]
    pub description: String,
    pub model_path: PathBuf,
    pub database: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OmniField {
    Dimension(OmniDimension),
    Measure(OmniMeasure),
}

impl OmniSemanticModel {
    pub fn get_description(&self) -> String {
        "Execute query on the database through the semantic layer.".to_string()
    }

    pub fn get_fields_by_pattern(
        &self,
        pattern: &str,
    ) -> anyhow::Result<HashMap<String, OmniField>> {
        if pattern == "all_views.*" {
            return Ok(self.get_all_fields());
        }

        if pattern.starts_with("tag:") {
            // TODO: implement tag based field retrieval
            return Ok(HashMap::new());
        }

        let (view_name, field_name) = pattern
            .split_once('.')
            .ok_or(anyhow::anyhow!("Invalid pattern: {}", pattern))?;

        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let mut fields = HashMap::new();
        if field_name == "*" {
            fields.extend(self.get_all_fields_from_view(view_name)?);
        } else {
            let field = view.get_field(field_name).ok_or(anyhow::anyhow!(
                "Field {} not found in view {}",
                field_name,
                view_name
            ))?;
            fields.insert(pattern.to_owned(), field);
        }

        Ok(fields)
    }

    pub fn get_all_fields(&self) -> HashMap<String, OmniField> {
        let mut fields = HashMap::new();

        for (view_name, view) in &self.views {
            for (name, dimension) in &view.dimensions {
                let field_name = format!("{view_name}.{name}");
                fields.insert(field_name, OmniField::Dimension(dimension.clone()));
            }
            for (name, measure) in &view.measures {
                let field_name = format!("{view_name}.{name}");
                fields.insert(field_name, OmniField::Measure(measure.clone()));
            }
        }

        fields
    }

    pub fn get_all_view_fields(
        &self,
        view_name: &str,
    ) -> anyhow::Result<HashMap<String, OmniField>> {
        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let mut fields = HashMap::new();

        for (name, dimension) in &view.dimensions {
            let field_name = format!("{view_name}.{name}");
            fields.insert(field_name, OmniField::Dimension(dimension.clone()));
        }
        for (name, measure) in &view.measures {
            let field_name = format!("{view_name}.{name}");
            fields.insert(field_name, OmniField::Measure(measure.clone()));
        }

        Ok(fields)
    }

    pub fn get_all_fields_from_view(
        &self,
        view_name: &str,
    ) -> anyhow::Result<HashMap<String, OmniField>> {
        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let mut fields = HashMap::new();

        for (name, dimension) in &view.dimensions {
            let field_name = format!("{view_name}.{name}");
            fields.insert(field_name, OmniField::Dimension(dimension.clone()));
        }
        for (name, measure) in &view.measures {
            let field_name = format!("{view_name}.{name}");
            fields.insert(field_name, OmniField::Measure(measure.clone()));
        }

        Ok(fields)
    }

    pub fn get_field(&self, view_name: &str, field_name: &str) -> anyhow::Result<OmniField> {
        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let field = view.get_field(field_name).ok_or(anyhow::anyhow!(
            "Field {} not found in view {}",
            field_name,
            view_name
        ))?;
        Ok(field)
    }

    pub fn get_topic_fields(&self, topic_name: &str) -> anyhow::Result<HashMap<String, OmniField>> {
        let topic = self.topics.get(topic_name).ok_or(anyhow::anyhow!(
            "Topic {} not found in semantic model",
            topic_name
        ))?;
        let mut fields = HashMap::new();

        for field_pattern in &topic.get_sorted_field_patterns() {
            let mut exclution = false;
            let mut field_pattern_cleaned = field_pattern.to_owned();
            if field_pattern.starts_with("-") {
                exclution = true;
                field_pattern_cleaned = field_pattern[1..].to_string();
            }
            let pattern_fields = self.get_fields_by_pattern(&field_pattern_cleaned)?;
            if exclution {
                for (name, _) in pattern_fields {
                    fields.remove(&name);
                }
            } else {
                for (name, field) in pattern_fields {
                    fields.insert(name, field);
                }
            }
        }

        Ok(fields)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OmniJoinType {
    #[serde(rename = "always_left")]
    AlwaysLeft,
    #[serde(rename = "full_outer")]
    FullOuter,
    #[serde(rename = "inner")]
    Inner,
}

impl OmniJoinType {
    pub fn to_sql(&self) -> String {
        match self {
            OmniJoinType::AlwaysLeft => "LEFT JOIN".to_string(),
            OmniJoinType::FullOuter => "FULL OUTER JOIN".to_string(),
            OmniJoinType::Inner => "INNER JOIN".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OmniRelationShipType {
    #[serde(rename = "one_to_one")]
    OneToOne,
    #[serde(rename = "one_to_many")]
    OneToMany,
    #[serde(rename = "many_to_one")]
    ManyToOne,
    #[serde(rename = "many_to_many")]
    ManyToMany,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniRelationShip {
    pub join_from_view: String,
    pub join_to_view: String,
    pub join_type: OmniJoinType,
    pub on_sql: String,
    pub relationship_type: OmniRelationShipType,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OmniTopicInfoTool {
    pub name: String,
    pub model_path: PathBuf,
    #[serde(default = "default_omni_info_description")]
    pub description: String,
}

impl OmniTopicInfoTool {
    pub fn get_description(&self) -> String {
        let mut description = "Get topic information. List of available topic:\n".to_string();
        let semantic_model = self
            .load_semantic_model()
            .expect("Failed to load semantic model");
        for (topic_name, _) in semantic_model.topics {
            description.push_str(&format!("- {topic_name}\n",));
        }
        description
    }

    pub fn load_semantic_model(&self) -> Result<OmniSemanticModel, OxyError> {
        // check if model path exists
        if !self.model_path.exists() {
            return Err(OxyError::AgentError(format!(
                "Model path {} does not exist",
                self.model_path.display()
            )));
        }
        let mut views = HashMap::new();
        let mut topics = HashMap::new();

        let view_paths = list_by_sub_extension(&self.model_path, ".view.yaml");
        let topic_paths = list_by_sub_extension(&self.model_path, ".topic.yaml");

        for view_path in view_paths {
            let entry = view_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {e}")))?;
            let view: OmniView = serde_yaml::from_slice(&file_bytes).map_err(|e| {
                OxyError::AgentError(format!(
                    "Failed to parse view: {} {}",
                    entry.to_string_lossy(),
                    e
                ))
            })?;
            match view.view_type.clone() {
                OmniViewType::Table(_) => {
                    let view_name = entry
                        .strip_prefix(self.model_path.clone())
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        // .replace(".query.view.yaml", "")
                        .replace(".view.yaml", "")
                        .replace("/", "__");
                    views.insert(view_name, view);
                }
                OmniViewType::Query(_) => {
                    let view_name = entry
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        .replace(".query.view.yaml", "");
                    views.insert(view_name, view);
                }
            }
        }

        for topic_path in topic_paths {
            let entry = topic_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {e}")))?;
            let topic_name = entry
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
                .replace(".topic.yaml", "");
            let topic = serde_yaml::from_slice(&file_bytes);

            match topic {
                Ok(topic) => {
                    topics.insert(topic_name, topic);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse topic: {} {}", &topic_name, e);
                }
            }
        }

        let relationships_file_path = self.model_path.join("relationships.yaml");

        let relationships: Vec<OmniRelationShip> = if relationships_file_path.exists() {
            let file_bytes = fs::read(relationships_file_path)
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {e}")))?;
            serde_yaml::from_slice(&file_bytes)
                .map_err(|e| OxyError::AgentError(format!("Failed to parse relationships: {e}")))?
        } else {
            vec![]
        };

        Ok(OmniSemanticModel {
            views,
            relationships,
            topics,
        })
    }
}

impl OmniView {
    pub fn get_model_description(&self) -> String {
        let mut description = format!("Schema: {}\n", self.schema);
        for (name, dimension) in &self.dimensions {
            let mut dimension_str = name.to_owned();
            if let Some(ref description) = dimension.description {
                dimension_str.push_str(&format!(" -  {description}"));
            }
            description.push_str(&format!("Dimension: {dimension_str}\n"));
        }
        for (name, measure) in &self.measures {
            let mut measure_str = name.to_owned();
            if let Some(ref description) = measure.description {
                measure_str.push_str(&format!(" -  {description})"));
            }
            description.push_str(&format!("Measure: {measure_str}\n"));
        }
        description
    }
}

impl ExecuteOmniTool {
    pub fn load_semantic_model(&self) -> Result<OmniSemanticModel, OxyError> {
        // check if model path exists
        if !self.model_path.exists() {
            return Err(OxyError::AgentError(format!(
                "Model path {} does not exist",
                self.model_path.display()
            )));
        }
        let mut views = HashMap::new();
        let mut topics = HashMap::new();

        let view_paths = list_by_sub_extension(&self.model_path, ".view.yaml");
        let topic_paths = list_by_sub_extension(&self.model_path, ".topic.yaml");

        for view_path in view_paths {
            let entry = view_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {e}")))?;
            let view: OmniView = serde_yaml::from_slice(&file_bytes).map_err(|e| {
                OxyError::AgentError(format!(
                    "Failed to parse view: {} {}",
                    entry.to_string_lossy(),
                    e
                ))
            })?;
            match view.view_type.clone() {
                OmniViewType::Table(_) => {
                    let view_name = entry
                        .strip_prefix(self.model_path.clone())
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        // .replace(".query.view.yaml", "")
                        .replace(".view.yaml", "")
                        .replace("/", "__");
                    views.insert(view_name, view);
                }
                OmniViewType::Query(_) => {
                    let view_name = entry
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        .replace(".query.view.yaml", "");
                    views.insert(view_name, view);
                }
            }
        }

        for topic_path in topic_paths {
            let entry = topic_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {e}")))?;
            let topic_name = entry
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
                .replace(".topic.yaml", "");
            let topic = serde_yaml::from_slice(&file_bytes);

            match topic {
                Ok(topic) => {
                    topics.insert(topic_name, topic);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse topic: {} {}", &topic_name, e);
                }
            }
        }

        let relationships_file_path = self.model_path.join("relationships.yaml");

        let relationships: Vec<OmniRelationShip> = if relationships_file_path.exists() {
            let file_bytes = fs::read(relationships_file_path)
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {e}")))?;
            serde_yaml::from_slice(&file_bytes)
                .map_err(|e| OxyError::AgentError(format!("Failed to parse relationships: {e}")))?
        } else {
            vec![]
        };

        Ok(OmniSemanticModel {
            views,
            relationships,
            topics,
        })
    }
}

fn default_omni_info_description() -> String {
    "Get details a about a omni topic. Including available fields".to_string()
}

fn default_omni_tool_description() -> String {
    "Execute query on the database. Construct from Omni semantic model.".to_string()
}
