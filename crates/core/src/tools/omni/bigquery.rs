use crate::config::model::{OmniMeasure, OmniSemanticModel, OmniTopic};
use std::collections::{HashMap, HashSet};

use anyhow::Ok;
use minijinja::Value;

use crate::{
    config::model::{AggregateType, OmniField, OmniFilter, OmniFilterValue},
    tools::types::{ExecuteOmniParams, FilterOperator},
};

use super::{
    types::{CompiledField, Filter, SqlParts},
    utils::{DELIMITER, generate_alias, get_referenced_variables, omni_template_to_jinja2},
};

use super::engine::SqlGenerationEngine;

impl SqlParts {
    pub fn new(base_table: &str) -> Self {
        Self {
            base_table: base_table.to_owned(),
            select_clauses: Vec::new(),
            where_clauses: Vec::new(),
            order_clauses: Vec::new(),
            group_clauses: Vec::new(),
            having_clauses: Vec::new(),
            join_clauses: Vec::new(),
            limit: None,
        }
    }

    pub fn to_query(&self) -> String {
        let mut sql_query = format!("SELECT {}", self.select_clauses.join(", "));
        sql_query.push_str(format!(" FROM {}", self.base_table).as_str());
        if !self.join_clauses.is_empty() {
            sql_query.push_str(format!(" {}", self.join_clauses.join(" ")).as_str());
        }
        if !self.where_clauses.is_empty() {
            sql_query.push_str(format!(" WHERE {}", self.where_clauses.join(" AND ")).as_str());
        }
        if !self.group_clauses.is_empty() {
            sql_query.push_str(format!(" GROUP BY {}", self.group_clauses.join(", ")).as_str());
        }
        if !self.having_clauses.is_empty() {
            sql_query.push_str(format!(" HAVING {}", self.having_clauses.join(" AND ")).as_str());
        }
        if !self.order_clauses.is_empty() {
            sql_query.push_str(format!(" ORDER BY {}", self.order_clauses.join(", ")).as_str());
        }
        if self.limit.is_some() {
            sql_query.push_str(format!(" LIMIT {}", self.limit.unwrap()).as_str());
        }
        sql_query
    }
}

pub struct BigquerySqlGenerationEngine {
    pub semantic_model: OmniSemanticModel,
    pub compiled_fields: HashMap<String, CompiledField>,
}

impl BigquerySqlGenerationEngine {
    pub fn new(semantic_model: OmniSemanticModel) -> Self {
        Self {
            semantic_model,
            compiled_fields: HashMap::new(),
        }
    }

    pub fn get_field_name(&self, field: &str, view_name: Option<&str>) -> String {
        if field.contains(".") {
            field.to_owned()
        } else {
            match view_name {
                Some(view_name) => format!("{}.{}", view_name, field),
                None => {
                    tracing::error!(
                        "Field {} does not have view name, please check your semantic model",
                        field
                    );
                    field.to_owned()
                }
            }
        }
    }

    pub fn render_sql(&self, sql: &str, ctx: HashMap<String, String>) -> anyhow::Result<String> {
        let jinja_sql = omni_template_to_jinja2(sql);
        let ctx = ctx
            .iter()
            .map(|(k, v)| (k.replace(".", DELIMITER), Value::from(v)))
            .collect::<HashMap<String, Value>>();
        let jinja_env = minijinja::Environment::new();
        let ctx_serialized = serde_json::to_string(&ctx)
            .map_err(|e| anyhow::anyhow!("Error serializing context: {}", e))?;
        jinja_env.render_str(&jinja_sql, ctx).map_err(|e| {
            anyhow::anyhow!(
                "Error rendering SQL: {} {} {} {}",
                sql,
                &jinja_sql,
                ctx_serialized,
                e
            )
        })
    }

    pub fn compile_value(
        &self,
        view_name: Option<&str>,
        value: &str,
    ) -> anyhow::Result<CompiledField> {
        let referenced_fields = get_referenced_variables(value);
        if referenced_fields.is_empty() {
            return Ok(CompiledField {
                sql: value.to_owned(),
                required_views: HashSet::new(),
                filters: vec![],
            });
        }

        let mut rendered_ref_fields = HashMap::new();
        let mut ctx_map = HashMap::new();
        for referenced_field in referenced_fields {
            let field_name = self.get_field_name(&referenced_field, view_name);
            if self.compiled_fields.contains_key(&field_name) {
                let compiled_field = self.compiled_fields.get(&field_name).unwrap();
                rendered_ref_fields.insert(field_name.clone(), compiled_field.to_owned());
                ctx_map.insert(
                    referenced_field,
                    format!("({})", compiled_field.sql.clone()),
                );
            } else {
                let compiled_field = self.compile_field(&field_name)?;
                tracing::debug!("compiled field {} {:?}", field_name, compiled_field);
                rendered_ref_fields.insert(field_name.clone(), compiled_field.clone());

                ctx_map.insert(
                    referenced_field,
                    format!("({})", compiled_field.sql.clone()),
                );
            }
        }
        let sql = self.render_sql(value, ctx_map)?;
        let mut required_views = HashSet::new();
        if let Some(view_name) = view_name {
            required_views.insert(view_name.to_owned());
        }
        let mut filters = vec![];
        for (_, compiled_field) in rendered_ref_fields.iter() {
            required_views.extend(compiled_field.required_views.clone());
            filters.extend(compiled_field.filters.clone());
        }

        Ok(CompiledField {
            sql,
            required_views,
            filters,
        })
    }

    fn compile_filter_value(
        &self,
        filter_value: Option<OmniFilterValue>,
    ) -> anyhow::Result<String> {
        match filter_value {
            Some(OmniFilterValue::String(value)) => Ok(format!("'{}'", value)),
            Some(OmniFilterValue::Int(value)) => Ok(format!("{}", value)),
            Some(OmniFilterValue::Array(value)) => {
                let mut values = vec![];
                for v in value {
                    let compiled_value = self.compile_filter_value(Some(v))?;
                    values.push(compiled_value);
                }
                Ok(format!("({})", values.join(",")))
            }
            Some(OmniFilterValue::Bool(value)) => {
                Ok((if value { "true" } else { "false" }).to_string())
            }
            None => Ok("NULL".to_string()),
        }
    }

    fn get_filter_operator(&self, filter: &Filter) -> anyhow::Result<String> {
        match filter.filter {
            OmniFilter::Is(ref f) => match f.is.clone() {
                Some(v) => match v {
                    OmniFilterValue::String(_) => Ok("=".to_string()),
                    OmniFilterValue::Int(_) => Ok("=".to_string()),
                    OmniFilterValue::Array(_) => Ok("IN".to_string()),
                    OmniFilterValue::Bool(_) => Ok("IS".to_string()),
                },
                None => Ok("IS".to_string()),
            },
            OmniFilter::Not(ref f) => match f.not.clone() {
                Some(v) => match v {
                    OmniFilterValue::String(_) => Ok("!=".to_string()),
                    OmniFilterValue::Int(_) => Ok("!=".to_string()),
                    OmniFilterValue::Array(_) => Ok("NOT IN".to_string()),
                    OmniFilterValue::Bool(_) => Ok("IS NOT".to_string()),
                },
                None => Ok("IS NOT".to_string()),
            },
        }
    }

    fn compile_filter(
        &self,
        view_name: Option<&str>,
        filter: &Filter,
    ) -> anyhow::Result<CompiledField> {
        let full_filed_name = self.get_field_name(&filter.field, view_name);
        let compiled_field = self.compile_field(&full_filed_name)?;
        let filter_operator = self.get_filter_operator(filter)?;
        match filter.filter {
            OmniFilter::Is(ref f) => {
                let value = self.compile_filter_value(f.is.clone())?;
                Ok(CompiledField {
                    sql: format!("{} {} {}", compiled_field.sql, filter_operator, value),
                    required_views: compiled_field.required_views,
                    filters: vec![],
                })
            }
            OmniFilter::Not(ref value) => {
                let value = self.compile_filter_value(value.not.clone())?;
                Ok(CompiledField {
                    sql: format!("{} {} {}", compiled_field.sql, filter_operator, value),
                    required_views: compiled_field.required_views,
                    filters: vec![],
                })
            }
        }
    }

    // First groups by <custom_primary_key_sql> (the distinct entity)
    // For each <custom_primary_key_sql>, find the minimum of <sql>
    // Finally, calculates the average across these distinct <custom_primary_key_sql>
    fn compile_average_distinct_on(
        &self,
        view_name: &str,
        compiled_sql: &str,
        measure: &crate::config::model::OmniMeasure,
        view: &crate::config::model::OmniView,
        field_name: &str,
    ) -> anyhow::Result<CompiledField> {
        let compiled_primary_k = self.compile_value(
            Some(view_name),
            measure.custom_primary_key_sql.as_ref().unwrap(),
        )?;
        let alias = generate_alias(field_name);
        let sql = format!(
            "(SELECT AVG(t) as {} FROM ( SELECT {}, MIN({}) AS t FROM {} GROUP BY {}))",
            alias,
            &compiled_primary_k.sql,
            compiled_sql,
            view.get_table_name(view_name),
            &compiled_primary_k.sql
        );

        Ok(CompiledField {
            sql,
            required_views: compiled_primary_k.required_views,
            filters: compiled_primary_k.filters,
        })
    }
    fn compile_mean_distinct_on(
        &self,
        view_name: &str,
        sql: &str,
        measure: &crate::config::model::OmniMeasure,
        view: &crate::config::model::OmniView,
        field_name: &str,
    ) -> anyhow::Result<CompiledField> {
        if measure.sql.is_none() {
            return Err(anyhow::anyhow!(
                "AverageDistinctOn measure {} does not have sql",
                field_name
            ));
        }
        let compiled_primary_k = self.compile_value(
            Some(view_name),
            measure.custom_primary_key_sql.as_ref().unwrap(),
        )?;
        let alias = generate_alias(field_name);
        let sql = format!(
            "(SELECT MEAN(t) as {} FROM ( SELECT {}, MIN({}) AS t FROM {} GROUP BY {}))",
            alias,
            &compiled_primary_k.sql,
            sql,
            view.get_table_name(view_name),
            &compiled_primary_k.sql
        );

        Ok(CompiledField {
            sql,
            required_views: compiled_primary_k.required_views,
            filters: compiled_primary_k.filters,
        })
    }

    fn compile_sum_distinct_on(
        &self,
        view_name: &str,
        sql: &str,
        measure: &crate::config::model::OmniMeasure,
        view: &crate::config::model::OmniView,
        field_name: &str,
    ) -> anyhow::Result<CompiledField> {
        if measure.sql.is_none() {
            return Err(anyhow::anyhow!(
                "AverageDistinctOn measure {} does not have sql",
                field_name
            ));
        }
        let compiled_primary_k = self.compile_value(
            Some(view_name),
            measure.custom_primary_key_sql.as_ref().unwrap(),
        )?;
        let alias = generate_alias(field_name);
        let sql = format!(
            "(SELECT SUM(t) as {} FROM ( SELECT {}, MIN({}) AS t FROM {} GROUP BY {}))",
            alias,
            &compiled_primary_k.sql,
            sql,
            view.get_table_name(view_name),
            &compiled_primary_k.sql
        );

        Ok(CompiledField {
            sql,
            required_views: compiled_primary_k.required_views,
            filters: compiled_primary_k.filters,
        })
    }

    pub fn compiled_measure(
        &self,
        field_name: &str,
        view_name: &str,
        measure: &OmniMeasure,
    ) -> anyhow::Result<CompiledField> {
        let view = self
            .semantic_model
            .views
            .get(view_name)
            .ok_or(anyhow::anyhow!(
                "View {} not found in semantic model",
                view_name
            ))?;
        let mut filters = vec![];
        let mut measure_filters = vec![];
        if let Some(fs) = &measure.filters {
            for (field_name, filter) in fs {
                let full_field_name = self.get_field_name(field_name, Some(view_name));
                measure_filters.push(Filter {
                    field: full_field_name,
                    filter: filter.clone(),
                });
            }
        }
        let mut compiled_sql;
        let mut required_views = HashSet::new();
        required_views.insert(view_name.to_owned());
        if let Some(sql) = &measure.sql {
            let compiled_field = self.compile_value(Some(view_name), sql)?;
            tracing::debug!("compiled field {} {:?}", field_name, compiled_field);
            compiled_sql = compiled_field.sql;
            required_views.extend(compiled_field.required_views);
            filters.extend(compiled_field.filters);
        } else {
            compiled_sql = view.get_full_field_name(field_name, view_name);
        }

        let mut compiled_filters = vec![];
        for filter in &measure_filters {
            let compiled_filter = self.compile_filter(Some(view_name), filter)?;
            compiled_filters.push(compiled_filter);
        }

        if !compiled_filters.is_empty() {
            // use case when on top of the sql
            compiled_sql = format!(
                "CASE WHEN ({}) THEN ({}) ELSE NULL END",
                compiled_filters
                    .iter()
                    .map(|f| f.sql.clone())
                    .collect::<Vec<_>>()
                    .join(" AND "),
                compiled_sql
            );
        }

        match &measure.aggregate_type {
            Some(aggregate_type) => match aggregate_type {
                AggregateType::Count => {
                    let value;
                    if measure.sql.is_some() {
                        value = compiled_sql;
                    } else {
                        value = "1".to_string();
                    }
                    Ok(CompiledField {
                        sql: format!("COUNT({})", value),
                        required_views,
                        filters,
                    })
                }
                AggregateType::Sum => {
                    if measure.sql.is_some() {
                        Ok(CompiledField {
                            sql: format!("SUM({})", compiled_sql),
                            required_views,
                            filters,
                        })
                    } else {
                        Err(anyhow::anyhow!(
                            "Sum measure {} does not have sql",
                            field_name
                        ))
                    }
                }
                AggregateType::Avg => {
                    if measure.sql.is_some() {
                        Ok(CompiledField {
                            sql: format!("AVG({})", compiled_sql),
                            required_views,
                            filters,
                        })
                    } else {
                        Err(anyhow::anyhow!(
                            "Avg measure {} does not have sql",
                            field_name
                        ))
                    }
                }
                AggregateType::Max => {
                    if let Some(_sql) = &measure.sql {
                        Ok(CompiledField {
                            sql: format!("MAX({})", compiled_sql),
                            required_views,
                            filters,
                        })
                    } else {
                        Err(anyhow::anyhow!(
                            "Max measure {} does not have sql",
                            field_name
                        ))
                    }
                }
                AggregateType::Min => {
                    if let Some(_sql) = &measure.sql {
                        Ok(CompiledField {
                            sql: format!("MIN({})", compiled_sql),
                            required_views,
                            filters,
                        })
                    } else {
                        Err(anyhow::anyhow!(
                            "Min measure {} does not have sql",
                            field_name
                        ))
                    }
                }
                AggregateType::CountDistinct => {
                    if measure.sql.is_some() {
                        Ok(CompiledField {
                            sql: format!("COUNT(DISTINCT {})", compiled_sql),
                            required_views,
                            filters,
                        })
                    } else {
                        Err(anyhow::anyhow!(
                            "CountDistinct measure {} does not have sql",
                            field_name
                        ))
                    }
                }
                AggregateType::AverageDistinctOn => {
                    let compiled = self.compile_average_distinct_on(
                        view_name,
                        &compiled_sql,
                        measure,
                        view,
                        field_name,
                    )?;
                    required_views.extend(compiled.required_views);
                    filters.extend(compiled.filters);
                    Ok(CompiledField {
                        sql: compiled.sql,
                        required_views,
                        filters,
                    })
                }
                AggregateType::MedianDistinctOn => {
                    let compiled = self.compile_mean_distinct_on(
                        view_name,
                        &compiled_sql,
                        measure,
                        view,
                        field_name,
                    )?;
                    required_views.extend(compiled.required_views);
                    filters.extend(compiled.filters);
                    Ok(CompiledField {
                        sql: compiled.sql,
                        required_views,
                        filters,
                    })
                }
                AggregateType::SumDistinctOn => {
                    let compiled = self.compile_sum_distinct_on(
                        view_name,
                        &compiled_sql,
                        measure,
                        view,
                        field_name,
                    )?;
                    required_views.extend(compiled.required_views);
                    filters.extend(compiled.filters);
                    Ok(CompiledField {
                        sql: compiled.sql,
                        required_views,
                        filters,
                    })
                }
                AggregateType::Median => {
                    if measure.sql.is_some() {
                        Ok(CompiledField {
                            sql: format!("MEDIAN({})", compiled_sql),
                            required_views,
                            filters,
                        })
                    } else {
                        Err(anyhow::anyhow!(
                            "Median measure {} does not have sql",
                            field_name
                        ))
                    }
                }
            },
            None => Ok(CompiledField {
                sql: compiled_sql,
                required_views,
                filters,
            }),
        }
    }

    pub fn compile_field(&self, field_name: &str) -> anyhow::Result<CompiledField> {
        let topic_fields = self.semantic_model.get_all_fields();
        let (_, _) = topic_fields
            .iter()
            .find(|f| f.0 == &field_name)
            .ok_or(anyhow::anyhow!("Field {} not found", field_name,))?;

        let (view_name, field_name) = field_name
            .split_once('.')
            .ok_or(anyhow::anyhow!("Invalid field name: {}", field_name))?;

        let view = self
            .semantic_model
            .views
            .get(view_name)
            .ok_or(anyhow::anyhow!(
                "View {} not found in semantic model",
                view_name
            ))?;

        let field = view.get_field(field_name).ok_or(anyhow::anyhow!(
            "Field {} not found in view {}",
            field_name,
            view_name
        ))?;

        match field {
            OmniField::Dimension(dimension) => match dimension.sql {
                Some(ref sql) => self.compile_value(Some(view_name), sql),
                None => {
                    let mut required_views = HashSet::new();
                    required_views.insert(view_name.to_owned());
                    Ok(CompiledField {
                        sql: view.get_full_field_name(field_name, view_name),
                        required_views,
                        filters: vec![],
                    })
                }
            },
            OmniField::Measure(measure) => self.compiled_measure(field_name, view_name, &measure),
        }
    }

    pub fn compile_join(&self, view_name: &str, topic: &OmniTopic) -> anyhow::Result<String> {
        if topic.joins.contains_key(view_name) {
            let mut relationship = self
                .semantic_model
                .relationships
                .iter()
                .find(|r| r.join_from_view == view_name && r.join_to_view == topic.base_view);

            if relationship.is_none() {
                relationship = self.semantic_model.relationships.iter().find(
                    |r: &&crate::config::model::OmniRelationShip| {
                        r.join_from_view == topic.base_view && r.join_to_view == view_name
                    },
                );
            }
            match relationship {
                Some(relationship) => {
                    let join_view =
                        self.semantic_model
                            .views
                            .get(view_name)
                            .ok_or(anyhow::anyhow!(
                                "Join view {} not found in semantic model",
                                view_name
                            ))?;
                    let on_clause = self.compile_value(None, &relationship.on_sql)?;
                    let join_clause = format!(
                        "{} {} ON {}",
                        relationship.join_type.to_sql(),
                        join_view.get_table_name(view_name),
                        on_clause.sql
                    );
                    Ok(join_clause)
                }
                None => Err(anyhow::anyhow!(
                    "Relationship not found between view {} and view {}",
                    view_name,
                    &topic.base_view
                )),
            }
        } else {
            Err(anyhow::anyhow!(
                "Join {} not allowed in topic {}",
                view_name,
                topic.label.clone().unwrap_or("no label".to_string())
            ))
        }
    }
}

impl SqlGenerationEngine for BigquerySqlGenerationEngine {
    fn generate_sql(&self, params: &ExecuteOmniParams) -> anyhow::Result<String> {
        let mut required_views = HashSet::new();
        let topic = self
            .semantic_model
            .topics
            .get(&params.topic)
            .ok_or(anyhow::anyhow!("Topic not found: {}", params.topic))?;

        let topic_fields = self.semantic_model.get_topic_fields(&params.topic)?;
        let mut selected_fields: HashMap<String, &OmniField> = HashMap::new();
        for field in &params.fields {
            if !topic_fields.contains_key(field) {
                return Err(anyhow::anyhow!(
                    "Field {} not found in topic {}",
                    field,
                    params.topic
                ));
            }
            selected_fields.insert(field.to_string(), topic_fields.get(field).unwrap());
        }

        let base_view = self
            .semantic_model
            .views
            .get(&topic.base_view)
            .ok_or(anyhow::anyhow!(
                "Base view {} not found in semantic model",
                topic.base_view
            ))?;
        let mut sql_parts = SqlParts::new(&base_view.get_table_name(&topic.base_view));
        if params.limit.is_some() {
            sql_parts.limit = params.limit;
        } else {
            // if all selected fields are measures, we need to set limit to 1
            if selected_fields
                .iter()
                .all(|(_, field)| matches!(field, OmniField::Measure(_)))
            {
                sql_parts.limit = Some(1);
            }
        }

        for (select_field_name, selected_field) in selected_fields {
            let compiled_field = self.compile_field(&select_field_name)?;
            tracing::debug!("compiled field {} {:?}", select_field_name, compiled_field);
            sql_parts.select_clauses.push(compiled_field.sql.clone());

            // add measure to group by
            match selected_field {
                OmniField::Dimension(_) => {
                    sql_parts.group_clauses.push(compiled_field.sql.clone());
                }
                OmniField::Measure(_) => {}
            }
            required_views.extend(compiled_field.required_views);
        }

        // filters
        for filter in &params.filters {
            let field = self
                .semantic_model
                .get_all_fields()
                .get(&filter.field)
                .ok_or(anyhow::anyhow!("Field not found: {}", filter.field))?
                .to_owned();

            let operator = match filter.operator {
                FilterOperator::Equal => "=",
                FilterOperator::NotEqual => "<>",
                FilterOperator::GreaterThan => ">",
                FilterOperator::LessThan => "<",
                FilterOperator::GreaterThanOrEqual => ">=",
                FilterOperator::LessThanOrEqual => "<=",
                FilterOperator::In => "IN",
                FilterOperator::NotIn => "NOT IN",
            };

            let compiled_field = self.compile_field(&filter.field)?;
            tracing::debug!("compiled field {} {:?}", filter.field, compiled_field);
            required_views.extend(compiled_field.required_views.clone());
            let expression: String =
                format!("({}) {} {}", &compiled_field.sql, operator, filter.values);
            match field {
                OmniField::Dimension(_) => {
                    sql_parts.where_clauses.push(expression);
                }
                OmniField::Measure(_) => {
                    sql_parts.having_clauses.push(expression);
                }
            }
        }

        // compile topic default filters if their view is included
        if let Some(default_filters) = &topic.default_filters {
            for (full_field_name, filter) in default_filters {
                let (view_name, field_name) = full_field_name
                    .split_once('.')
                    .ok_or(anyhow::anyhow!("Invalid field name: {}", full_field_name))?;

                if !required_views.contains(view_name) {
                    continue;
                }

                let compiled_field = self.compile_filter(
                    None,
                    &Filter {
                        field: full_field_name.to_owned(),
                        filter: filter.to_owned(),
                    },
                )?;

                let field = self.semantic_model.get_field(view_name, field_name)?;
                match field {
                    OmniField::Dimension(_) => {
                        sql_parts.where_clauses.push(compiled_field.sql);
                    }
                    OmniField::Measure(_) => {
                        sql_parts.having_clauses.push(compiled_field.sql);
                    }
                }
                required_views.extend(compiled_field.required_views.clone());
            }
        }

        // figure out joins
        for required_view in required_views.clone() {
            if required_view == topic.base_view {
                continue;
            }

            let join_clause = self.compile_join(&required_view, topic)?;
            sql_parts.join_clauses.push(join_clause);
        }

        Ok(sql_parts.to_query())
    }
}
