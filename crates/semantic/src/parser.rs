use crate::SemanticLayerError;
use crate::models::*;
use crate::validation::{ValidationResult, validate_semantic_layer, validate_variable_syntax};
use crate::variables::VariableEncoder;
use minijinja::{Environment, context};
use oxy_globals::{GlobalRegistry, TemplateResolver};
use regex::Regex;
use serde_yaml;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for the semantic layer parser
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Base directory containing the semantic layer files
    pub base_path: PathBuf,
    /// Whether to validate files during parsing
    pub validate: bool,
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
}

impl ParserConfig {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            validate: true,
            follow_symlinks: false,
        }
    }

    pub fn with_validation(mut self, validate: bool) -> Self {
        self.validate = validate;
        self
    }

    pub fn with_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }
}

/// Result of parsing semantic layer files
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// The parsed semantic layer
    pub semantic_layer: SemanticLayer,
    /// Validation result if validation was enabled
    pub validation: Option<ValidationResult>,
    /// Warnings encountered during parsing
    pub warnings: Vec<String>,
    /// List of files that were parsed
    pub parsed_files: Vec<PathBuf>,
    /// All variables found across the semantic layer
    pub variables_found: HashSet<String>,
}

/// Parser for semantic layer configurations
pub struct SemanticLayerParser {
    config: ParserConfig,
    global_registry: GlobalRegistry,
}

impl SemanticLayerParser {
    /// Creates a new parser with the given configuration
    pub fn new(config: ParserConfig, global_registry: GlobalRegistry) -> Self {
        Self {
            config,
            global_registry,
        }
    }

    /// Parses the semantic layer from the configured directory structure
    pub fn parse(&self) -> Result<ParseResult, SemanticLayerError> {
        let mut views = Vec::new();
        let mut topics = Vec::new();
        let mut parsed_files = Vec::new();

        // Parse views from views/ directory
        let views_dir = self.config.base_path.join("views");
        if views_dir.exists() {
            let (parsed_views, view_files) = self.parse_views(&views_dir)?;
            views.extend(parsed_views);
            parsed_files.extend(view_files);
        } else {
            return Err(SemanticLayerError::IOError(format!(
                "Views directory not found: {}",
                views_dir.display()
            )));
        }

        // Parse topics from topics/ directory
        let topics_dir = self.config.base_path.join("topics");
        if topics_dir.exists() {
            let (parsed_topics, topic_files) = self.parse_topics(&topics_dir)?;
            topics.extend(parsed_topics);
            parsed_files.extend(topic_files);
        }

        // Create semantic layer
        let semantic_layer = SemanticLayer {
            views,
            topics: if topics.is_empty() {
                None
            } else {
                Some(topics)
            },
            metadata: None,
        };

        // Validate if enabled
        let validation = if self.config.validate {
            Some(validate_semantic_layer(&semantic_layer)?)
        } else {
            None
        };

        // Collect all variables used across the semantic layer
        let mut variables_found = HashSet::new();
        let encoder = VariableEncoder::new();

        for view in &semantic_layer.views {
            // Collect variables from dimensions
            for dimension in &view.dimensions {
                if dimension.has_variables() {
                    let vars = encoder.extract_variables(&dimension.expr);
                    variables_found.extend(vars);
                }
            }
            // Collect variables from measures
            if let Some(measures) = &view.measures {
                for measure in measures {
                    if measure.has_variables() {
                        if let Some(expr) = &measure.expr {
                            let vars = encoder.extract_variables(expr);
                            variables_found.extend(vars);
                        }
                        // Also check filters
                        if let Some(filters) = &measure.filters {
                            for filter in filters {
                                if filter.has_variables() {
                                    let vars = encoder.extract_variables(&filter.expr);
                                    variables_found.extend(vars);
                                }
                            }
                        }
                    }
                }
            }
            // Collect variables from table references
            if let Some(table) = &view.table
                && encoder.has_variables(table)
            {
                let vars = encoder.extract_variables(table);
                variables_found.extend(vars);
            }
            if let Some(sql) = &view.sql
                && encoder.has_variables(sql)
            {
                let vars = encoder.extract_variables(sql);
                variables_found.extend(vars);
            }
        }

        // Topics don't have their own dimensions/measures, they reference views
        // So no need to check topics for variables

        Ok(ParseResult {
            semantic_layer,
            validation,
            warnings: Vec::new(), // No warnings anymore - errors fail immediately
            parsed_files,
            variables_found,
        })
    }

    /// Parses all view files from the given directory
    fn parse_views(
        &self,
        views_dir: &Path,
    ) -> Result<(Vec<View>, Vec<PathBuf>), SemanticLayerError> {
        let mut views = Vec::new();
        let mut parsed_files = Vec::new();

        let entries = fs::read_dir(views_dir).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to read views directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to read directory entry: {}", e))
            })?;
            let path = entry.path();

            if path.is_file() && self.is_view_file(&path) {
                let view = self.parse_view_file(&path)?;
                views.push(view);
                parsed_files.push(path);
            }
        }

        Ok((views, parsed_files))
    }

    /// Parses all topic files from the given directory
    fn parse_topics(
        &self,
        topics_dir: &Path,
    ) -> Result<(Vec<Topic>, Vec<PathBuf>), SemanticLayerError> {
        let mut topics = Vec::new();
        let mut parsed_files = Vec::new();

        let entries = fs::read_dir(topics_dir).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to read topics directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to read directory entry: {}", e))
            })?;
            let path = entry.path();

            if path.is_file() && self.is_topic_file(&path) {
                let topic = self.parse_topic_file(&path)?;
                topics.push(topic);
                parsed_files.push(path);
            }
        }

        Ok((topics, parsed_files))
    }

    /// Parses a single view file
    fn parse_view_file(&self, path: &Path) -> Result<View, SemanticLayerError> {
        // Read raw YAML content as a string (before parsing)
        let content = fs::read_to_string(path).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        // Render the YAML template with Jinja2 (resolves both {{globals.*}} and {{variables.*}})
        let rendered_content = self.render_yaml_template(&content, path)?;

        // Parse the rendered YAML
        let mut yaml_value: serde_yaml::Value =
            serde_yaml::from_str(&rendered_content).map_err(|e| {
                let location_info = if let Some(location) = e.location() {
                    format!(" at line {}, column {}", location.line(), location.column())
                } else {
                    String::new()
                };
                SemanticLayerError::ParsingError(format!(
                    "Failed to parse rendered YAML in {}{}: {}",
                    path.display(),
                    location_info,
                    e
                ))
            })?;

        // Resolve inheritance (inherits_from fields)
        yaml_value = self
            .global_registry
            .resolve_with_inheritance(&yaml_value)
            .map_err(|e| {
                SemanticLayerError::ParsingError(format!(
                    "Failed to resolve global references in {}: {}",
                    path.display(),
                    e
                ))
            })?;

        // Store original expressions before any variable processing
        yaml_value = self.preprocess_variables(&yaml_value, path)?;

        // Now deserialize into View struct
        let view: View = serde_yaml::from_value(yaml_value).map_err(|e| {
            let location_info = if let Some(location) = e.location() {
                format!(" at line {}, column {}", location.line(), location.column())
            } else {
                String::new()
            };
            SemanticLayerError::ParsingError(format!(
                "Failed to deserialize view from {}{}: {}",
                path.display(),
                location_info,
                e
            ))
        })?;

        Ok(view)
    }

    /// Parses a single topic file
    fn parse_topic_file(&self, path: &Path) -> Result<Topic, SemanticLayerError> {
        let content = fs::read_to_string(path).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        // First parse as generic YAML value
        let mut yaml_value: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| {
            let location_info = if let Some(location) = e.location() {
                format!(" at line {}, column {}", location.line(), location.column())
            } else {
                String::new()
            };
            SemanticLayerError::ParsingError(format!(
                "Failed to parse YAML in {}{}: {}",
                path.display(),
                location_info,
                e
            ))
        })?;

        // Resolve templates and global references if registry is available
        // First resolve templates ({{globals.path}} expressions)
        yaml_value = self
            .global_registry
            .resolve_templates(&yaml_value)
            .map_err(|e| {
                SemanticLayerError::ParsingError(format!(
                    "Failed to resolve templates in {}: {}",
                    path.display(),
                    e
                ))
            })?;

        // Then resolve inheritance (inherits_from fields)
        yaml_value = self
            .global_registry
            .resolve_with_inheritance(&yaml_value)
            .map_err(|e| {
                SemanticLayerError::ParsingError(format!(
                    "Failed to resolve global references in {}: {}",
                    path.display(),
                    e
                ))
            })?;

        // Topics don't currently have direct variable support in their structure,
        // but validate any variable syntax if present for future compatibility
        self.validate_topic_variables(&yaml_value, path)?;

        // Now deserialize into Topic struct
        let topic: Topic = serde_yaml::from_value(yaml_value).map_err(|e| {
            let location_info = if let Some(location) = e.location() {
                format!(" at line {}, column {}", location.line(), location.column())
            } else {
                String::new()
            };
            SemanticLayerError::ParsingError(format!(
                "Failed to deserialize topic from {}{}: {}",
                path.display(),
                location_info,
                e
            ))
        })?;

        Ok(topic)
    }

    /// Checks if a file is a view file based on its extension
    fn is_view_file(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "yaml" || ext == "yml")
            .unwrap_or(false)
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.ends_with(".view"))
                .unwrap_or(false)
    }

    /// Checks if a file is a topic file based on its extension
    fn is_topic_file(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "yaml" || ext == "yml")
            .unwrap_or(false)
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.ends_with(".topic"))
                .unwrap_or(false)
    }

    /// Preprocesses variables in YAML value to store original expressions and validate syntax
    fn preprocess_variables(
        &self,
        yaml_value: &serde_yaml::Value,
        file_path: &Path,
    ) -> Result<serde_yaml::Value, SemanticLayerError> {
        let mut processed_value = yaml_value.clone();
        let encoder = VariableEncoder::new();

        // Process dimensions if they exist
        if let Some(dimensions) = processed_value.get_mut("dimensions")
            && let Some(dimensions_array) = dimensions.as_sequence_mut()
        {
            for dimension in dimensions_array {
                if let Some(dimension_map) = dimension.as_mapping_mut()
                    && let Some(expr_value) = dimension_map.get("expr")
                    && let Some(expr_str) = expr_value.as_str()
                {
                    // Validate variable syntax
                    let validation = validate_variable_syntax(
                        expr_str,
                        &format!("Dimension in {}", file_path.display()),
                    );
                    if !validation.is_valid {
                        return Err(SemanticLayerError::ValidationError(
                            validation.errors.join("; "),
                        ));
                    }

                    // Store original expression if it contains variables
                    if encoder.has_variables(expr_str) {
                        dimension_map.insert(
                            serde_yaml::Value::String("original_expr".to_string()),
                            serde_yaml::Value::String(expr_str.to_string()),
                        );
                    }
                }
            }
        }

        // Process measures if they exist
        if let Some(measures) = processed_value.get_mut("measures")
            && let Some(measures_array) = measures.as_sequence_mut()
        {
            for measure in measures_array {
                if let Some(measure_map) = measure.as_mapping_mut() {
                    // Process measure expression
                    if let Some(expr_value) = measure_map.get("expr")
                        && let Some(expr_str) = expr_value.as_str()
                    {
                        // Validate variable syntax
                        let validation = validate_variable_syntax(
                            expr_str,
                            &format!("Measure in {}", file_path.display()),
                        );
                        if !validation.is_valid {
                            return Err(SemanticLayerError::ValidationError(
                                validation.errors.join("; "),
                            ));
                        }

                        // Store original expression if it contains variables
                        if encoder.has_variables(expr_str) {
                            measure_map.insert(
                                serde_yaml::Value::String("original_expr".to_string()),
                                serde_yaml::Value::String(expr_str.to_string()),
                            );
                        }
                    }

                    // Process measure filters
                    if let Some(filters) = measure_map.get_mut("filters")
                        && let Some(filters_array) = filters.as_sequence_mut()
                    {
                        for filter in filters_array {
                            if let Some(filter_map) = filter.as_mapping_mut()
                                && let Some(expr_value) = filter_map.get("expr")
                                && let Some(expr_str) = expr_value.as_str()
                            {
                                // Validate variable syntax
                                let validation = validate_variable_syntax(
                                    expr_str,
                                    &format!("Measure filter in {}", file_path.display()),
                                );
                                if !validation.is_valid {
                                    return Err(SemanticLayerError::ValidationError(
                                        validation.errors.join("; "),
                                    ));
                                }

                                // Store original expression if it contains variables
                                if encoder.has_variables(expr_str) {
                                    filter_map.insert(
                                        serde_yaml::Value::String("original_expr".to_string()),
                                        serde_yaml::Value::String(expr_str.to_string()),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Process table references
        if let Some(table_value) = processed_value.get("table")
            && let Some(table_str) = table_value.as_str()
        {
            let validation = validate_variable_syntax(
                table_str,
                &format!("Table reference in {}", file_path.display()),
            );
            if !validation.is_valid {
                return Err(SemanticLayerError::ValidationError(
                    validation.errors.join("; "),
                ));
            }
        }

        // Process SQL queries
        if let Some(sql_value) = processed_value.get("sql")
            && let Some(sql_str) = sql_value.as_str()
        {
            let validation =
                validate_variable_syntax(sql_str, &format!("SQL query in {}", file_path.display()));
            if !validation.is_valid {
                return Err(SemanticLayerError::ValidationError(
                    validation.errors.join("; "),
                ));
            }
        }

        Ok(processed_value)
    }

    /// Validates variable syntax in topic files (for future compatibility)
    fn validate_topic_variables(
        &self,
        yaml_value: &serde_yaml::Value,
        file_path: &Path,
    ) -> Result<(), SemanticLayerError> {
        // Topics don't currently support direct variables, but we validate
        // any variable-like syntax for future compatibility and clear error messages

        // Check string values recursively for variable syntax
        fn check_value(value: &serde_yaml::Value, context: &str) -> Result<(), String> {
            match value {
                serde_yaml::Value::String(s) => {
                    let validation = validate_variable_syntax(s, context);
                    if !validation.is_valid {
                        return Err(validation.errors.join("; "));
                    }
                }
                serde_yaml::Value::Mapping(map) => {
                    for (key, val) in map {
                        if let Some(key_str) = key.as_str() {
                            let new_context = format!("{}.{}", context, key_str);
                            check_value(val, &new_context)?;
                        }
                    }
                }
                serde_yaml::Value::Sequence(seq) => {
                    for (i, val) in seq.iter().enumerate() {
                        let new_context = format!("{}[{}]", context, i);
                        check_value(val, &new_context)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }

        check_value(yaml_value, &format!("Topic in {}", file_path.display()))
            .map_err(SemanticLayerError::ValidationError)?;

        Ok(())
    }

    /// Parses a single semantic layer file (for backwards compatibility)
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<SemanticLayer, SemanticLayerError> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| {
            SemanticLayerError::IOError(format!(
                "Failed to read file {}: {}",
                path.as_ref().display(),
                e
            ))
        })?;

        let semantic_layer: SemanticLayer = serde_yaml::from_str(&content).map_err(|e| {
            let location_info = if let Some(location) = e.location() {
                format!(" at line {}, column {}", location.line(), location.column())
            } else {
                String::new()
            };
            SemanticLayerError::ParsingError(format!(
                "Failed to parse YAML{}: {}",
                location_info, e
            ))
        })?;

        Ok(semantic_layer)
    }

    /// Render YAML template using Jinja2 with globals and variables context
    ///
    /// This method creates a unified Jinja2 context with both globals and variables,
    /// then renders the YAML template in a single pass.
    /// Only renders templates that reference globals.* or variables.*
    fn render_yaml_template(
        &self,
        yaml_content: &str,
        path: &Path,
    ) -> Result<String, SemanticLayerError> {
        // Build Jinja2 context with globals
        let globals_context = self.global_registry.to_jinja_context().map_err(|e| {
            SemanticLayerError::ParsingError(format!(
                "Failed to build globals context for {}: {}",
                path.display(),
                e
            ))
        })?;

        // Pre-process: temporarily replace non-globals templates with placeholders
        // Variables should be preserved as-is for encoding by the CubeJS translator
        let placeholder_prefix = "___TEMPLATE_PLACEHOLDER_";
        let mut placeholders = Vec::new();
        let re = Regex::new(r"\{\{[^}]+\}\}").unwrap();

        let protected_content = re
            .replace_all(yaml_content, |caps: &regex::Captures| {
                let matched = caps.get(0).unwrap().as_str();
                // Check if this template references globals.* or variables.*
                let inner = matched
                    .trim_start_matches("{{")
                    .trim_end_matches("}}")
                    .trim();
                if inner.starts_with("globals.") {
                    // Keep globals templates for rendering
                    matched.to_string()
                } else {
                    // Replace ALL other templates (including variables.*) with placeholders
                    // Variables will be encoded later by the CubeJS translator
                    let placeholder = format!("{}{}", placeholder_prefix, placeholders.len());
                    placeholders.push(matched.to_string());
                    placeholder
                }
            })
            .to_string();

        // Create minijinja environment
        let mut env = Environment::new();
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);

        // Compile template from the protected YAML string
        let template = env.template_from_str(&protected_content).map_err(|e| {
            SemanticLayerError::ParsingError(format!(
                "Failed to compile template in {}: {}",
                path.display(),
                e
            ))
        })?;

        // Render with context (only globals are rendered at parse time)
        let mut rendered = template
            .render(context! {
                globals => globals_context,
            })
            .map_err(|e| {
                SemanticLayerError::ParsingError(format!(
                    "Failed to render template in {}: {}",
                    path.display(),
                    e
                ))
            })?;

        // Restore the original templates that were placeholders
        for (i, original) in placeholders.iter().enumerate() {
            let placeholder = format!("{}{}", placeholder_prefix, i);
            rendered = rendered.replace(&placeholder, original);
        }

        Ok(rendered)
    }

    /// Exports a semantic layer to YAML format
    pub fn export_to_yaml(semantic_layer: &SemanticLayer) -> Result<String, SemanticLayerError> {
        serde_yaml::to_string(semantic_layer).map_err(|e| {
            SemanticLayerError::ParsingError(format!("Failed to serialize to YAML: {}", e))
        })
    }

    /// Exports a semantic layer to JSON format
    pub fn export_to_json(semantic_layer: &SemanticLayer) -> Result<String, SemanticLayerError> {
        serde_json::to_string_pretty(semantic_layer).map_err(|e| {
            SemanticLayerError::ParsingError(format!("Failed to serialize to JSON: {}", e))
        })
    }

    /// Writes a semantic layer to files in the configured directory structure
    pub fn write_to_files(
        &self,
        semantic_layer: &SemanticLayer,
    ) -> Result<Vec<PathBuf>, SemanticLayerError> {
        let mut written_files = Vec::new();

        // Ensure base directory exists
        fs::create_dir_all(&self.config.base_path).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to create base directory: {}", e))
        })?;

        // Write views
        let views_dir = self.config.base_path.join("views");
        fs::create_dir_all(&views_dir).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to create views directory: {}", e))
        })?;

        for view in &semantic_layer.views {
            let file_path = views_dir.join(format!("{}.view.yaml", view.name));
            let content = serde_yaml::to_string(view).map_err(|e| {
                SemanticLayerError::ParsingError(format!("Failed to serialize view: {}", e))
            })?;

            fs::write(&file_path, content).map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to write view file: {}", e))
            })?;

            written_files.push(file_path);
        }

        // Write topics
        if let Some(topics) = &semantic_layer.topics {
            let topics_dir = self.config.base_path.join("topics");
            fs::create_dir_all(&topics_dir).map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to create topics directory: {}", e))
            })?;

            for topic in topics {
                let file_path = topics_dir.join(format!("{}.topic.yaml", topic.name));
                let content = serde_yaml::to_string(topic).map_err(|e| {
                    SemanticLayerError::ParsingError(format!("Failed to serialize topic: {}", e))
                })?;

                fs::write(&file_path, content).map_err(|e| {
                    SemanticLayerError::IOError(format!("Failed to write topic file: {}", e))
                })?;

                written_files.push(file_path);
            }
        }

        Ok(written_files)
    }
}

/// Convenience function to parse semantic layer from a directory
pub fn parse_semantic_layer_from_dir<P: AsRef<Path>>(
    path: P,
    global_registry: GlobalRegistry,
) -> Result<ParseResult, SemanticLayerError> {
    let parser_config = ParserConfig::new(path);
    let parser = SemanticLayerParser::new(parser_config, global_registry);
    parser.parse()
}
