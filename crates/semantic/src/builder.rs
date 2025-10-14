use crate::{models::*, validation::SemanticValidator};
use std::collections::HashMap;

/// Builder for creating Entity instances
#[derive(Debug, Clone)]
pub struct EntityBuilder {
    name: Option<String>,
    entity_type: Option<EntityType>,
    description: Option<String>,
    key: Option<String>,
    keys: Option<Vec<String>>,
    label: Option<String>,
}

impl EntityBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            entity_type: None,
            description: None,
            key: None,
            keys: None,
            label: None,
        }
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn primary(mut self) -> Self {
        self.entity_type = Some(EntityType::Primary);
        self
    }

    pub fn foreign(mut self) -> Self {
        self.entity_type = Some(EntityType::Foreign);
        self
    }

    pub fn entity_type(mut self, entity_type: EntityType) -> Self {
        self.entity_type = Some(entity_type);
        self
    }

    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keys = Some(keys.into_iter().map(|k| k.into()).collect());
        self
    }

    pub fn label<S: Into<String>>(mut self, label: S) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn build(self) -> Result<Entity, String> {
        // Validate that at least one of key or keys is provided
        if self.key.is_none() && self.keys.is_none() {
            return Err("Entity must have either 'key' or 'keys' specified".to_string());
        }

        let entity = Entity {
            name: self.name.ok_or("Entity name is required")?,
            entity_type: self.entity_type.ok_or("Entity type is required")?,
            description: self.description.ok_or("Entity description is required")?,
            key: self.key,
            keys: self.keys,
        };

        let validation = entity.validate();
        if !validation.is_valid {
            return Err(format!(
                "Entity validation failed: {}",
                validation.errors.join(", ")
            ));
        }

        Ok(entity)
    }
}

impl Default for EntityBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating Dimension instances
#[derive(Debug, Clone)]
pub struct DimensionBuilder {
    name: Option<String>,
    dimension_type: Option<DimensionType>,
    description: Option<String>,
    expr: Option<String>,
    label: Option<String>,
    samples: Option<Vec<String>>,
    synonyms: Option<Vec<String>>,
}

impl DimensionBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            dimension_type: None,
            description: None,
            expr: None,
            label: None,
            samples: None,
            synonyms: None,
        }
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn string_type(mut self) -> Self {
        self.dimension_type = Some(DimensionType::String);
        self
    }

    pub fn number_type(mut self) -> Self {
        self.dimension_type = Some(DimensionType::Number);
        self
    }

    pub fn date_type(mut self) -> Self {
        self.dimension_type = Some(DimensionType::Date);
        self
    }

    pub fn datetime_type(mut self) -> Self {
        self.dimension_type = Some(DimensionType::Datetime);
        self
    }

    pub fn boolean_type(mut self) -> Self {
        self.dimension_type = Some(DimensionType::Boolean);
        self
    }

    pub fn dimension_type(mut self, dimension_type: DimensionType) -> Self {
        self.dimension_type = Some(dimension_type);
        self
    }

    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn expr<S: Into<String>>(mut self, expr: S) -> Self {
        self.expr = Some(expr.into());
        self
    }

    pub fn label<S: Into<String>>(mut self, label: S) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn samples<I, S>(mut self, samples: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.samples = Some(samples.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn sample<S: Into<String>>(mut self, sample: S) -> Self {
        self.samples
            .get_or_insert_with(Vec::new)
            .push(sample.into());
        self
    }

    pub fn synonyms<I, S>(mut self, synonyms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.synonyms = Some(synonyms.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn synonym<S: Into<String>>(mut self, synonym: S) -> Self {
        self.synonyms
            .get_or_insert_with(Vec::new)
            .push(synonym.into());
        self
    }

    pub fn build(self) -> Result<Dimension, String> {
        let dimension = Dimension {
            name: self.name.ok_or("Dimension name is required")?,
            dimension_type: self.dimension_type.ok_or("Dimension type is required")?,
            description: self.description,
            expr: self.expr.ok_or("Dimension expr is required")?,
            samples: self.samples,
            synonyms: self.synonyms,
        };

        let validation = dimension.validate();
        if !validation.is_valid {
            return Err(format!(
                "Dimension validation failed: {}",
                validation.errors.join(", ")
            ));
        }

        Ok(dimension)
    }
}

impl Default for DimensionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating Measure instances
#[derive(Debug, Clone)]
pub struct MeasureBuilder {
    name: Option<String>,
    measure_type: Option<MeasureType>,
    description: Option<String>,
    expr: Option<String>,
    label: Option<String>,
    filters: Option<Vec<MeasureFilter>>,
    samples: Option<Vec<String>>,
    synonyms: Option<Vec<String>>,
}

impl MeasureBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            measure_type: None,
            description: None,
            expr: None,
            label: None,
            filters: None,
            samples: None,
            synonyms: None,
        }
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn count(mut self) -> Self {
        self.measure_type = Some(MeasureType::Count);
        self
    }

    pub fn sum(mut self) -> Self {
        self.measure_type = Some(MeasureType::Sum);
        self
    }

    pub fn average(mut self) -> Self {
        self.measure_type = Some(MeasureType::Average);
        self
    }

    pub fn min(mut self) -> Self {
        self.measure_type = Some(MeasureType::Min);
        self
    }

    pub fn max(mut self) -> Self {
        self.measure_type = Some(MeasureType::Max);
        self
    }

    pub fn count_distinct(mut self) -> Self {
        self.measure_type = Some(MeasureType::CountDistinct);
        self
    }

    pub fn median(mut self) -> Self {
        self.measure_type = Some(MeasureType::Median);
        self
    }

    pub fn custom(mut self) -> Self {
        self.measure_type = Some(MeasureType::Custom);
        self
    }

    pub fn measure_type(mut self, measure_type: MeasureType) -> Self {
        self.measure_type = Some(measure_type);
        self
    }

    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn expr<S: Into<String>>(mut self, expr: S) -> Self {
        self.expr = Some(expr.into());
        self
    }

    pub fn label<S: Into<String>>(mut self, label: S) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn samples<I, S>(mut self, samples: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.samples = Some(samples.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn sample<S: Into<String>>(mut self, sample: S) -> Self {
        self.samples
            .get_or_insert_with(Vec::new)
            .push(sample.into());
        self
    }

    pub fn synonyms<I, S>(mut self, synonyms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.synonyms = Some(synonyms.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn synonym<S: Into<String>>(mut self, synonym: S) -> Self {
        self.synonyms
            .get_or_insert_with(Vec::new)
            .push(synonym.into());
        self
    }

    pub fn filter(mut self, expr: String, description: Option<String>) -> Self {
        let filter = MeasureFilter { expr, description };
        self.filters.get_or_insert_with(Vec::new).push(filter);
        self
    }

    pub fn build(self) -> Result<Measure, String> {
        let measure = Measure {
            name: self.name.ok_or("Measure name is required")?,
            measure_type: self.measure_type.ok_or("Measure type is required")?,
            description: self.description,
            expr: self.expr,
            filters: self.filters,
            samples: self.samples,
            synonyms: self.synonyms,
        };

        let validation = measure.validate();
        if !validation.is_valid {
            return Err(format!(
                "Measure validation failed: {}",
                validation.errors.join(", ")
            ));
        }

        Ok(measure)
    }
}

impl Default for MeasureBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating View instances
#[derive(Debug, Clone)]
pub struct ViewBuilder {
    name: Option<String>,
    description: Option<String>,
    label: Option<String>,
    datasource: Option<String>,
    table: Option<String>,
    sql: Option<String>,
    entities: Vec<Entity>,
    dimensions: Vec<Dimension>,
    measures: Vec<Measure>,
}

impl ViewBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            description: None,
            label: None,
            datasource: None,
            table: None,
            sql: None,
            entities: Vec::new(),
            dimensions: Vec::new(),
            measures: Vec::new(),
        }
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn label<S: Into<String>>(mut self, label: S) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn datasource<S: Into<String>>(mut self, datasource: S) -> Self {
        self.datasource = Some(datasource.into());
        self
    }

    pub fn table<S: Into<String>>(mut self, table: S) -> Self {
        self.table = Some(table.into());
        self
    }

    pub fn sql<S: Into<String>>(mut self, sql: S) -> Self {
        self.sql = Some(sql.into());
        self
    }

    pub fn entity(mut self, entity: Entity) -> Self {
        self.entities.push(entity);
        self
    }

    pub fn entities<I>(mut self, entities: I) -> Self
    where
        I: IntoIterator<Item = Entity>,
    {
        self.entities.extend(entities);
        self
    }

    pub fn dimension(mut self, dimension: Dimension) -> Self {
        self.dimensions.push(dimension);
        self
    }

    pub fn dimensions<I>(mut self, dimensions: I) -> Self
    where
        I: IntoIterator<Item = Dimension>,
    {
        self.dimensions.extend(dimensions);
        self
    }

    pub fn measure(mut self, measure: Measure) -> Self {
        self.measures.push(measure);
        self
    }

    pub fn measures<I>(mut self, measures: I) -> Self
    where
        I: IntoIterator<Item = Measure>,
    {
        self.measures.extend(measures);
        self
    }

    pub fn build(self) -> Result<View, String> {
        let view = View {
            name: self.name.ok_or("View name is required")?,
            description: self.description.ok_or("View description is required")?,
            label: self.label,
            datasource: self.datasource,
            table: self.table,
            sql: self.sql,
            entities: self.entities,
            dimensions: self.dimensions,
            measures: if self.measures.is_empty() {
                None
            } else {
                Some(self.measures)
            },
        };

        let validation = view.validate();
        if !validation.is_valid {
            return Err(format!(
                "View validation failed: {}",
                validation.errors.join(", ")
            ));
        }

        Ok(view)
    }
}

impl Default for ViewBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating Topic instances
#[derive(Debug, Clone)]
pub struct TopicBuilder {
    name: Option<String>,
    description: Option<String>,
    views: Vec<String>,
    retrieval: Option<TopicRetrievalConfig>,
}

impl TopicBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            description: None,
            views: Vec::new(),
            retrieval: None,
        }
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn view<S: Into<String>>(mut self, view: S) -> Self {
        self.views.push(view.into());
        self
    }

    pub fn views<I, S>(mut self, views: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.views.extend(views.into_iter().map(|s| s.into()));
        self
    }

    pub fn build(self) -> Result<Topic, String> {
        let topic = Topic {
            name: self.name.ok_or("Topic name is required")?,
            description: self.description.ok_or("Topic description is required")?,
            views: self.views,
            retrieval: self.retrieval,
        };

        let validation = topic.validate();
        if !validation.is_valid {
            return Err(format!(
                "Topic validation failed: {}",
                validation.errors.join(", ")
            ));
        }

        Ok(topic)
    }
}

impl Default for TopicBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating SemanticLayer instances
#[derive(Debug, Clone)]
pub struct SemanticLayerBuilder {
    views: Vec<View>,
    topics: Vec<Topic>,
    metadata: Option<HashMap<String, serde_json::Value>>,
}

impl SemanticLayerBuilder {
    pub fn new() -> Self {
        Self {
            views: Vec::new(),
            topics: Vec::new(),
            metadata: None,
        }
    }

    pub fn view(mut self, view: View) -> Self {
        self.views.push(view);
        self
    }

    pub fn views<I>(mut self, views: I) -> Self
    where
        I: IntoIterator<Item = View>,
    {
        self.views.extend(views);
        self
    }

    pub fn topic(mut self, topic: Topic) -> Self {
        self.topics.push(topic);
        self
    }

    pub fn topics<I>(mut self, topics: I) -> Self
    where
        I: IntoIterator<Item = Topic>,
    {
        self.topics.extend(topics);
        self
    }

    pub fn metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn metadata_entry<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<serde_json::Value>,
    {
        self.metadata
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> Result<SemanticLayer, String> {
        let semantic_layer = SemanticLayer {
            views: self.views,
            topics: if self.topics.is_empty() {
                None
            } else {
                Some(self.topics)
            },
            metadata: self.metadata,
        };

        let validation = semantic_layer.validate();
        if !validation.is_valid {
            return Err(format!(
                "SemanticLayer validation failed: {}",
                validation.errors.join(", ")
            ));
        }

        Ok(semantic_layer)
    }
}

impl Default for SemanticLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
