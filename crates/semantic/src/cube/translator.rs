use regex::Regex;
use std::{collections::HashMap, path::PathBuf};

use crate::{
    DimensionType, EntityType, MeasureFilter, MeasureType, SemanticLayer, SemanticLayerError, View,
    cube::{
        data_sources::generate_data_sources,
        entity_graph::EntityGraph,
        file_writer::save_cube_semantics,
        models::{
            CubeCube, CubeDimension, CubeMeasure, CubeMeasureFilter,
            CubeSemanticLayerWithDataSources, CubeView, DatabaseDetails,
        },
    },
    parse_semantic_layer_from_dir,
};

/// Main translator for converting Oxy semantic layers to CubeJS format
pub struct CubeJSTranslator {
    target_dir: PathBuf,
}

impl CubeJSTranslator {
    /// Create a new translator with default path
    pub fn new(target_dir: PathBuf) -> Result<Self, SemanticLayerError> {
        Ok(Self { target_dir })
    }

    /// Translate an Oxy semantic layer to CubeJS format
    pub async fn translate(
        &self,
        oxy_semantic_layer: &SemanticLayer,
        databases: HashMap<String, DatabaseDetails>,
    ) -> Result<CubeSemanticLayerWithDataSources, SemanticLayerError> {
        translate_oxy_to_cube(oxy_semantic_layer.clone(), databases).await
    }

    /// Save CubeJS data to files
    pub async fn save_to_files(
        &self,
        cube_data: &CubeSemanticLayerWithDataSources,
    ) -> Result<(), SemanticLayerError> {
        save_cube_semantics(cube_data.clone(), &self.target_dir.to_string_lossy()).await
    }

    /// Translate and save in one operation
    pub async fn translate_and_save(
        &self,
        oxy_semantic_layer: &SemanticLayer,
        databases: HashMap<String, DatabaseDetails>,
    ) -> Result<(), SemanticLayerError> {
        let cube_data = self.translate(oxy_semantic_layer, databases).await?;
        self.save_to_files(&cube_data).await
    }
}

/// Core translation function from Oxy to Cube format with data sources
async fn translate_oxy_to_cube(
    oxy: SemanticLayer,
    databases: HashMap<String, DatabaseDetails>,
) -> Result<CubeSemanticLayerWithDataSources, SemanticLayerError> {
    let mut cubes = Vec::new();
    let mut cube_views = Vec::new();

    println!(
        "Translating {} views from Oxy to Cube format",
        oxy.views.len()
    );

    // Build entity graph for join resolution
    println!("Building entity graph for join resolution...");
    let entity_graph = EntityGraph::from_semantic_layer(&oxy)?;
    println!(
        "Generated {} join relationships",
        entity_graph.get_joins().len()
    );

    // Generate data sources
    let data_sources = generate_data_sources(&oxy, databases).await?;

    // Convert Oxy views to Cube cubes or views
    for view in oxy.views {
        let cube_dimensions = convert_dimensions(view.clone(), &entity_graph)?;
        let cube_measures = convert_measures(&view, &entity_graph)?;

        // Create cube or view based on whether it has a table or SQL
        if let Some(table) = &view.table {
            // Generate joins for this cube
            let cube_joins = entity_graph.generate_cube_joins(&view.name);

            // This is a table-based cube
            cubes.push(CubeCube {
                name: view.name.clone(),
                sql_table: Some(table.clone()),
                sql: None,
                data_source: view.datasource.clone(),
                title: view.label.clone().or_else(|| Some(view.name.clone())),
                description: Some(view.description.clone()),
                dimensions: cube_dimensions,
                measures: cube_measures,
                joins: cube_joins,
                pre_aggregations: std::collections::HashMap::new(),
            });
        } else if let Some(sql) = &view.sql {
            // This is a SQL-based view
            cube_views.push(CubeView {
                name: view.name.clone(),
                sql: sql.clone(),
                data_source: view.datasource.clone(),
                title: view.label.clone().or_else(|| Some(view.name.clone())),
                description: Some(view.description.clone()),
                dimensions: cube_dimensions,
                measures: cube_measures,
            });
        } else {
            // Generate joins for this cube
            let cube_joins = entity_graph.generate_cube_joins(&view.name);

            // Default to table-based if neither is specified
            cubes.push(CubeCube {
                name: view.name.clone(),
                sql_table: Some(view.name.clone()), // Use view name as table name
                sql: None,
                data_source: view.datasource.clone(),
                title: view.label.clone().or_else(|| Some(view.name.clone())),
                description: Some(view.description.clone()),
                dimensions: cube_dimensions,
                measures: cube_measures,
                joins: cube_joins,
                pre_aggregations: std::collections::HashMap::new(),
            });
        }
    }

    println!(
        "Successfully translated to {} cubes, {} views, and {} data sources in Cube format",
        cubes.len(),
        cube_views.len(),
        data_sources.len()
    );

    // Log join information
    let total_joins: usize = cubes.iter().map(|c| c.joins.len()).sum();
    if total_joins > 0 {
        println!(
            "Generated {} join relationships across all cubes",
            total_joins
        );
        for cube in &cubes {
            if !cube.joins.is_empty() {
                println!("  - Cube '{}' has {} joins", cube.name, cube.joins.len());
            }
        }
    }

    Ok(CubeSemanticLayerWithDataSources {
        cubes,
        views: cube_views,
        data_sources,
    })
}

/// Convert Oxy dimensions to CubeJS dimensions
fn convert_dimensions(
    view: View,
    entity_graph: &EntityGraph,
) -> Result<Vec<CubeDimension>, SemanticLayerError> {
    let mut cube_dimensions = Vec::new();

    // Convert dimensions
    for dimension in &view.dimensions {
        let dimension_type = match dimension.dimension_type {
            DimensionType::String => "string",
            DimensionType::Number => "number",
            DimensionType::Date => "time",
            DimensionType::Datetime => "time",
            DimensionType::Boolean => "boolean",
        }
        .to_string();

        // Check if this dimension is the key for a primary entity
        let is_primary_key = dimension.primary_key.unwrap_or(false)
            || view.entities.iter().any(|entity| {
                entity.entity_type == EntityType::Primary && entity.key == dimension.name
            });

        let translated_sql = translate_cross_entity_references(&dimension.expr, entity_graph)?;

        cube_dimensions.push(CubeDimension {
            name: dimension.name.clone(),
            sql: translated_sql,
            dimension_type,
            title: Some(dimension.name.clone()),
            description: dimension.description.clone(),
            primary_key: if is_primary_key { Some(true) } else { None },
        });
    }

    Ok(cube_dimensions)
}

/// Convert Oxy measures to CubeJS measures
fn convert_measures(
    view: &View,
    entity_graph: &EntityGraph,
) -> Result<Vec<CubeMeasure>, SemanticLayerError> {
    let mut cube_measures = Vec::new();

    // Convert measures
    if let Some(measures) = &view.measures {
        for measure in measures {
            let measure_type = match measure.measure_type {
                MeasureType::Count => "count",
                MeasureType::Sum => "sum",
                MeasureType::Average => "avg",
                MeasureType::Min => "min",
                MeasureType::Max => "max",
                MeasureType::CountDistinct => "countDistinct",
                MeasureType::Median => "avg", // Cube.js doesn't have median, use avg
                MeasureType::Custom => "number",
            }
            .to_string();

            let sql_expr = measure.expr.clone().unwrap_or_else(|| "1".to_string());
            let translated_sql = translate_cross_entity_references(&sql_expr, entity_graph)?;

            // Convert measure filters from Oxy to CubeJS format
            let cube_filters = if let Some(oxy_filters) = &measure.filters {
                Some(convert_measure_filters(oxy_filters, entity_graph)?)
            } else {
                None
            };

            cube_measures.push(CubeMeasure {
                name: measure.name.clone(),
                sql: translated_sql,
                measure_type,
                title: Some(measure.name.clone()),
                description: measure.description.clone(),
                format: None,
                filters: cube_filters,
            });
        }
    }

    Ok(cube_measures)
}

/// Convert Oxy measure filters to CubeJS measure filters
fn convert_measure_filters(
    oxy_filters: &[MeasureFilter],
    entity_graph: &EntityGraph,
) -> Result<Vec<CubeMeasureFilter>, SemanticLayerError> {
    let mut cube_filters = Vec::new();

    for oxy_filter in oxy_filters {
        let translated_sql = translate_cross_entity_references(&oxy_filter.expr, entity_graph)?;

        cube_filters.push(CubeMeasureFilter {
            sql: translated_sql,
        });
    }

    Ok(cube_filters)
}

/// Public function to process semantics that can be used from other modules
pub async fn process_semantic_layer_to_cube(
    semantic_dir: PathBuf,
    target_dir: PathBuf,
    databases: HashMap<String, DatabaseDetails>,
) -> Result<(), SemanticLayerError> {
    println!("🔄 Processing semantic layer...");

    if !semantic_dir.exists() {
        println!(
            "ℹ️  No semantic directory found at {}, skipping semantic processing",
            semantic_dir.display()
        );
        return Ok(());
    }

    println!("📂 Loading semantic layer from: {}", semantic_dir.display());

    // Parse the semantic layer from the directory structure
    let parse_result = parse_semantic_layer_from_dir(semantic_dir).map_err(|e| {
        SemanticLayerError::ParsingError(format!("Failed to parse semantic layer: {}", e))
    })?;

    let semantic_layer = parse_result.semantic_layer;

    // Print warnings if any
    if !parse_result.warnings.is_empty() {
        println!("⚠️  Warnings during parsing:");
        for warning in &parse_result.warnings {
            println!("   - {}", warning);
        }
    }

    println!(
        "✅ Successfully loaded semantic layer with {} views",
        semantic_layer.views.len()
    );
    if let Some(topics) = &semantic_layer.topics {
        println!("   and {} topics", topics.len());
    }

    // Convert it to cube format using the CubeJSTranslator
    println!("🔄 Converting to Cube.js format...");

    let translator = CubeJSTranslator::new(target_dir)?;
    let cube_semantic_layer = translator.translate(&semantic_layer, databases).await?;

    println!(
        "✅ Converted to Cube.js format: {} cubes, {} views, {} data sources",
        cube_semantic_layer.cubes.len(),
        cube_semantic_layer.views.len(),
        cube_semantic_layer.data_sources.len()
    );

    // Save it to the configured directory
    println!("💾 Saving to cube directory...");

    translator.save_to_files(&cube_semantic_layer).await?;

    println!("🎉 Semantic layer processing completed successfully!");

    Ok(())
}

/// Translate cross-entity references from Oxy format to Cube.js format
///
/// Handles both {{entity.field}} and {entity.field} patterns and converts them
/// to Cube.js format {ViewName.fieldName} by mapping entities to their primary views.
/// Also handles simple field references like {{field_name}} within the same view.
fn translate_cross_entity_references(
    expr: &str,
    entity_graph: &EntityGraph,
) -> Result<String, SemanticLayerError> {
    // Pattern for double braces with entity.field: {{entity.field}}
    let double_brace_entity_pattern = Regex::new(r"\{\{([^}]+)\.([^}]+)\}\}")
        .map_err(|e| SemanticLayerError::ParsingError(format!("Invalid regex pattern: {}", e)))?;

    // Pattern for double braces with simple field: {{field}}
    let double_brace_simple_pattern = Regex::new(r"\{\{([^}.]+)\}\}")
        .map_err(|e| SemanticLayerError::ParsingError(format!("Invalid regex pattern: {}", e)))?;

    // Pattern for single braces: {entity.field}
    let single_brace_pattern = Regex::new(r"\{([^}]+)\.([^}]+)\}")
        .map_err(|e| SemanticLayerError::ParsingError(format!("Invalid regex pattern: {}", e)))?;

    let mut result = expr.to_string();

    // Handle double brace entity.field patterns first
    result = double_brace_entity_pattern
        .replace_all(&result, |caps: &regex::Captures| {
            let entity_name = &caps[1];
            let field_name = &caps[2];

            // Find the view that contains this entity as primary
            if let Some(view_name) = entity_graph.get_primary_entities().get(entity_name) {
                format!("{{{}.{}}}", view_name, field_name)
            } else {
                // If entity not found, keep original reference but convert to single braces
                // This allows for manual cube references that don't map to entities
                format!("{{{}.{}}}", entity_name, field_name)
            }
        })
        .to_string();

    // Handle double brace simple field patterns (convert {{field}} to {field})
    result = double_brace_simple_pattern
        .replace_all(&result, |caps: &regex::Captures| {
            let field_name = &caps[1];
            format!("{{{}}}", field_name)
        })
        .to_string();

    // Handle single brace patterns
    result = single_brace_pattern
        .replace_all(&result, |caps: &regex::Captures| {
            let entity_name = &caps[1];
            let field_name = &caps[2];

            // Find the view that contains this entity as primary
            if let Some(view_name) = entity_graph.get_primary_entities().get(entity_name) {
                format!("{{{}.{}}}", view_name, field_name)
            } else {
                // If entity not found, keep original reference
                // This allows for manual cube references that don't map to entities
                format!("{{{}.{}}}", entity_name, field_name)
            }
        })
        .to_string();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dimension, DimensionType, Entity, EntityType, SemanticLayer};

    #[test]
    fn test_convert_measure_filters() {
        // Create a simple entity graph for testing
        let view = View {
            name: "test_view".to_string(),
            description: "Test view".to_string(),
            table: Some("test_table".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "test_entity".to_string(),
                entity_type: EntityType::Primary,
                key: "id".to_string(),
                description: "Test entity".to_string(),
            }],
            dimensions: vec![Dimension {
                name: "test_dimension".to_string(),
                expr: "test_column".to_string(),
                dimension_type: DimensionType::String,
                description: None,
                primary_key: Some(false),
                synonyms: None,
                samples: None,
            }],
            measures: None,
        };

        let semantic_layer = SemanticLayer {
            views: vec![view],
            topics: None,
            metadata: None,
        };

        let entity_graph = EntityGraph::from_semantic_layer(&semantic_layer).unwrap();

        // Create test measure filters
        let oxy_filters = vec![
            MeasureFilter {
                expr: "status = 'active'".to_string(),
                description: Some("Filter for active records".to_string()),
            },
            MeasureFilter {
                expr: "{{test_entity.field}} > 100".to_string(),
                description: None,
            },
        ];

        // Convert filters
        let result = convert_measure_filters(&oxy_filters, &entity_graph);
        assert!(result.is_ok());

        let cube_filters = result.unwrap();
        assert_eq!(cube_filters.len(), 2);

        // Check first filter (simple SQL)
        assert_eq!(cube_filters[0].sql, "status = 'active'");

        // Check second filter (should translate cross-entity references)
        assert_eq!(cube_filters[1].sql, "{test_view.field} > 100");
    }

    #[test]
    fn test_translate_cross_entity_references_in_filters() {
        let view = View {
            name: "orders".to_string(),
            description: "Orders view".to_string(),
            table: Some("orders_table".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "order".to_string(),
                entity_type: EntityType::Primary,
                key: "order_id".to_string(),
                description: "Order entity".to_string(),
            }],
            dimensions: vec![],
            measures: None,
        };

        let semantic_layer = SemanticLayer {
            views: vec![view],
            topics: None,
            metadata: None,
        };

        let entity_graph = EntityGraph::from_semantic_layer(&semantic_layer).unwrap();

        // Test different patterns
        let test_cases = vec![
            ("status = 'active'", "status = 'active'"), // No references
            ("{{order.amount}} > 100", "{orders.amount} > 100"), // Double brace entity reference
            ("{order.status}", "{orders.status}"),      // Single brace entity reference
            ("{{field_name}}", "{field_name}"),         // Simple field reference
            (
                "{{order.total}} + {{order.tax}}",
                "{orders.total} + {orders.tax}",
            ), // Multiple references
        ];

        for (input, expected) in test_cases {
            let result = translate_cross_entity_references(input, &entity_graph);
            assert!(result.is_ok(), "Failed to translate: {}", input);
            assert_eq!(
                result.unwrap(),
                expected,
                "Translation mismatch for: {}",
                input
            );
        }
    }
}
