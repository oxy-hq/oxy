use oxy_globals::GlobalRegistry;
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
    variables::VariableEncoder,
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

/// Helper function to resolve the table name for a view
/// For Domo databases, uses the dataset_id instead of the table name
fn resolve_table_name(
    default_table: &str,
    datasource: &Option<String>,
    databases: &HashMap<String, DatabaseDetails>,
) -> String {
    if let Some(datasource_name) = datasource
        && let Some(db_details) = databases.get(datasource_name)
    {
        // For Domo databases, use the dataset_id as the table name
        if db_details.db_type == "domo"
            && let Some(dataset_id) = &db_details.dataset_id
        {
            return format!("\"{}\"", dataset_id); // Quote the dataset_id
        }
    }
    default_table.to_string()
}

/// Core translation function from Oxy to Cube format with data sources
async fn translate_oxy_to_cube(
    oxy: SemanticLayer,
    databases: HashMap<String, DatabaseDetails>,
) -> Result<CubeSemanticLayerWithDataSources, SemanticLayerError> {
    let mut cubes = Vec::new();
    let mut cube_views = Vec::new();
    let mut variable_mappings = HashMap::new();

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
    let data_sources = generate_data_sources(&oxy, databases.clone()).await?;

    // Convert Oxy views to Cube cubes or views
    for view in oxy.views {
        let (cube_dimensions, mut encoder) = convert_dimensions(view.clone(), &entity_graph)?;
        let cube_measures = convert_measures(&view, &entity_graph, &mut encoder)?;

        // Store variable mappings for this view if any variables were encoded
        let encoder_mapping = encoder.get_mapping();
        if !encoder_mapping.is_empty() {
            variable_mappings.insert(view.name.clone(), encoder_mapping.clone());
        }

        // Create cube or view based on whether it has a table or SQL
        if let Some(table) = &view.table {
            // Encode variables in table reference
            let encoded_table = if encoder.has_variables(table) {
                encoder.encode_expression(table)
            } else {
                table.clone()
            };

            // For Domo databases, override with dataset_id
            let final_table = resolve_table_name(&encoded_table, &view.datasource, &databases);

            // Generate joins for this cube
            let cube_joins = entity_graph.generate_cube_joins(&view.name);

            // This is a table-based cube
            cubes.push(CubeCube {
                name: view.name.clone(),
                sql_table: Some(final_table),
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
            // Encode variables in SQL query
            let encoded_sql = if encoder.has_variables(sql) {
                encoder.encode_expression(sql)
            } else {
                sql.clone()
            };

            // This is a SQL-based view
            cube_views.push(CubeView {
                name: view.name.clone(),
                sql: encoded_sql,
                data_source: view.datasource.clone(),
                title: view.label.clone().or_else(|| Some(view.name.clone())),
                description: Some(view.description.clone()),
                dimensions: cube_dimensions,
                measures: cube_measures,
            });
        } else {
            // Generate joins for this cube
            let cube_joins = entity_graph.generate_cube_joins(&view.name);

            // For Domo databases, use dataset_id instead of view name
            let table_name = resolve_table_name(&view.name, &view.datasource, &databases);

            // Default to table-based if neither is specified
            cubes.push(CubeCube {
                name: view.name.clone(),
                sql_table: Some(table_name),
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
        variable_mappings,
    })
}

/// Convert Oxy dimensions to CubeJS dimensions
fn convert_dimensions(
    view: View,
    entity_graph: &EntityGraph,
) -> Result<(Vec<CubeDimension>, VariableEncoder), SemanticLayerError> {
    let mut cube_dimensions = Vec::new();
    let mut encoder = VariableEncoder::new();

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
        let is_primary_key = view.entities.iter().any(|entity| {
            entity.entity_type == EntityType::Primary && entity.get_keys().contains(&dimension.name)
        });

        // Encode variables in the expression before processing
        let encoded_expr = if dimension.has_variables() {
            encoder.encode_expression(&dimension.expr)
        } else {
            dimension.expr.clone()
        };

        let translated_sql = translate_cross_entity_references(&encoded_expr, entity_graph)?;

        cube_dimensions.push(CubeDimension {
            name: dimension.name.clone(),
            sql: translated_sql,
            dimension_type,
            title: Some(dimension.name.clone()),
            description: dimension.description.clone(),
            primary_key: if is_primary_key { Some(true) } else { None },
        });
    }

    Ok((cube_dimensions, encoder))
}

/// Convert Oxy measures to CubeJS measures
fn convert_measures(
    view: &View,
    entity_graph: &EntityGraph,
    encoder: &mut VariableEncoder,
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

            // Encode variables in the expression before processing
            let encoded_expr = if measure.has_variables() && measure.expr.is_some() {
                encoder.encode_expression(&sql_expr)
            } else {
                sql_expr
            };

            let translated_sql = translate_cross_entity_references(&encoded_expr, entity_graph)?;

            // Convert measure filters from Oxy to CubeJS format
            let cube_filters = if let Some(oxy_filters) = &measure.filters {
                Some(convert_measure_filters(oxy_filters, entity_graph, encoder)?)
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
    encoder: &mut VariableEncoder,
) -> Result<Vec<CubeMeasureFilter>, SemanticLayerError> {
    let mut cube_filters = Vec::new();

    for oxy_filter in oxy_filters {
        // Encode variables in the filter expression before processing
        let encoded_expr = if oxy_filter.has_variables() {
            encoder.encode_expression(&oxy_filter.expr)
        } else {
            oxy_filter.expr.clone()
        };

        let translated_sql = translate_cross_entity_references(&encoded_expr, entity_graph)?;

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
    globals_registry: GlobalRegistry,
) -> Result<(), SemanticLayerError> {
    _process_semantic_layer_to_cube_internal(
        semantic_dir,
        target_dir,
        databases,
        globals_registry,
        true,
    )
    .await
}

/// Internal function to process semantic layer with optional manifest saving
async fn _process_semantic_layer_to_cube_internal(
    semantic_dir: PathBuf,
    target_dir: PathBuf,
    databases: HashMap<String, DatabaseDetails>,
    globals_registry: GlobalRegistry,
    save_manifest: bool,
) -> Result<(), SemanticLayerError> {
    // Clone paths and databases upfront for manifest saving later
    let semantic_dir_ref = semantic_dir.clone();
    let target_dir_ref = target_dir.clone();
    let databases_ref = databases.clone();

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
    let parse_result =
        parse_semantic_layer_from_dir(semantic_dir, globals_registry).map_err(|e| {
            SemanticLayerError::ParsingError(format!("Failed to parse semantic layer: {}", e))
        })?;

    let semantic_layer = parse_result.semantic_layer;

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

    // Save manifest if requested (for incremental builds)
    if save_manifest {
        use crate::build_manifest::BuildManifest;
        use crate::change_detector::hash_database_config;
        use crate::cube::entity_graph::EntityGraph;

        let mut manifest = BuildManifest::new();
        manifest.update_timestamp();

        // Scan all current files and compute hashes
        let views_dir = semantic_dir_ref.join("views");
        let topics_dir = semantic_dir_ref.join("topics");

        if views_dir.exists() {
            for entry in std::fs::read_dir(&views_dir).map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to read views directory: {}", e))
            })? {
                let entry = entry.map_err(|e| {
                    SemanticLayerError::IOError(format!("Failed to read directory entry: {}", e))
                })?;
                let path = entry.path();

                if path.is_file() && path.extension().map(|e| e == "yml").unwrap_or(false) {
                    let hash = crate::hash_file(&path)?;
                    let relative = path
                        .strip_prefix(&semantic_dir_ref)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    manifest.add_file_hash(&relative, hash);

                    // Output mapping
                    if let Some(stem) = path.file_stem() {
                        let view_name = stem.to_string_lossy().replace(".view", "");
                        let output_path = target_dir_ref
                            .join("model")
                            .join(format!("{}.yml", view_name))
                            .to_string_lossy()
                            .to_string();
                        manifest.add_output_mapping(&relative, vec![output_path]);
                    }
                }
            }
        }

        if topics_dir.exists() {
            for entry in std::fs::read_dir(&topics_dir).map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to read topics directory: {}", e))
            })? {
                let entry = entry.map_err(|e| {
                    SemanticLayerError::IOError(format!("Failed to read directory entry: {}", e))
                })?;
                let path = entry.path();

                if path.is_file() && path.extension().map(|e| e == "yml").unwrap_or(false) {
                    let hash = crate::hash_file(&path)?;
                    let relative = path
                        .strip_prefix(&semantic_dir_ref)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    manifest.add_file_hash(&relative, hash);

                    // Output mapping
                    if let Some(stem) = path.file_stem() {
                        let topic_name = stem.to_string_lossy().replace(".topic", "");
                        let output_path = target_dir_ref
                            .join("model")
                            .join(format!("{}.yml", topic_name))
                            .to_string_lossy()
                            .to_string();
                        manifest.add_output_mapping(&relative, vec![output_path]);
                    }
                }
            }
        }

        // Build entity graph for dependency tracking
        let entity_graph = EntityGraph::from_semantic_layer(&semantic_layer)?;
        manifest.set_dependency_graph(entity_graph.get_dependency_graph());

        // Set config and globals hashes
        let config_hash = hash_database_config(&databases_ref);
        manifest.set_config_hash(config_hash);

        let globals_hash = crate::change_detector::hash_globals_registry(
            &semantic_dir_ref
                .parent()
                .unwrap_or(&semantic_dir_ref)
                .join(".oxy/globals"),
        )
        .unwrap_or_default();
        manifest.set_globals_hash(globals_hash);

        // Compute and set embedding file hashes
        let change_detector = crate::ChangeDetector::new(&semantic_dir_ref, &target_dir_ref);
        let embedding_hashes = change_detector.scan_embedding_files().unwrap_or_default();
        manifest.set_embedding_file_hashes(embedding_hashes);

        // Save manifest
        manifest.save(target_dir_ref.join(".build_manifest.json"))?;
    }

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
                key: Some("id".to_string()),
                keys: None,
                description: "Test entity".to_string(),
            }],
            dimensions: vec![Dimension {
                name: "test_dimension".to_string(),
                expr: "test_column".to_string(),
                original_expr: None,
                dimension_type: DimensionType::String,
                description: None,
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
                original_expr: None,
                description: Some("Filter for active records".to_string()),
            },
            MeasureFilter {
                expr: "{{test_entity.field}} > 100".to_string(),
                original_expr: None,
                description: None,
            },
        ];

        // Convert filters
        let mut encoder = VariableEncoder::new();
        let result = convert_measure_filters(&oxy_filters, &entity_graph, &mut encoder);
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
                key: Some("order_id".to_string()),
                keys: None,
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

    #[test]
    fn test_resolve_table_name_domo() {
        let mut databases = HashMap::new();
        databases.insert(
            "my_domo".to_string(),
            DatabaseDetails {
                name: "my_domo".to_string(),
                db_type: "domo".to_string(),
                dataset_id: Some("b24c28ba-11fc-4c3f-885a-762c06baa7f1".to_string()),
            },
        );

        // Domo with dataset_id returns quoted dataset_id
        let result = resolve_table_name("default_table", &Some("my_domo".to_string()), &databases);
        assert_eq!(result, "\"b24c28ba-11fc-4c3f-885a-762c06baa7f1\"");
    }

    #[test]
    fn test_resolve_table_name_non_domo() {
        let mut databases = HashMap::new();
        databases.insert(
            "postgres_db".to_string(),
            DatabaseDetails {
                name: "postgres_db".to_string(),
                db_type: "postgres".to_string(),
                dataset_id: None,
            },
        );

        // Non-Domo databases return default table name unchanged
        let result = resolve_table_name("my_table", &Some("postgres_db".to_string()), &databases);
        assert_eq!(result, "my_table");
    }
}
