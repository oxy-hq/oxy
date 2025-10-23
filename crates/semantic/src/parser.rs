use crate::SemanticLayerError;
use crate::models::*;
use crate::validation::{ValidationResult, validate_semantic_layer};
use serde_yaml;
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
}

/// Parser for semantic layer configurations
pub struct SemanticLayerParser {
    config: ParserConfig,
}

impl SemanticLayerParser {
    /// Creates a new parser with the given configuration
    pub fn new(config: ParserConfig) -> Self {
        Self { config }
    }

    /// Creates a new parser with default configuration for the given base path
    pub fn with_base_path<P: AsRef<Path>>(base_path: P) -> Self {
        Self::new(ParserConfig::new(base_path))
    }

    /// Parses the semantic layer from the configured directory structure
    pub fn parse(&self) -> Result<ParseResult, SemanticLayerError> {
        let mut views = Vec::new();
        let mut topics = Vec::new();
        let mut warnings = Vec::new();
        let mut parsed_files = Vec::new();

        // Parse views from views/ directory
        let views_dir = self.config.base_path.join("views");
        if views_dir.exists() {
            let (parsed_views, view_warnings, view_files) = self.parse_views(&views_dir)?;
            views.extend(parsed_views);
            warnings.extend(view_warnings);
            parsed_files.extend(view_files);
        } else {
            warnings.push(format!(
                "Views directory not found: {}",
                views_dir.display()
            ));
        }

        // Parse topics from topics/ directory
        let topics_dir = self.config.base_path.join("topics");
        if topics_dir.exists() {
            let (parsed_topics, topic_warnings, topic_files) = self.parse_topics(&topics_dir)?;
            topics.extend(parsed_topics);
            warnings.extend(topic_warnings);
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

        Ok(ParseResult {
            semantic_layer,
            validation,
            warnings,
            parsed_files,
        })
    }

    /// Parses all view files from the given directory
    fn parse_views(
        &self,
        views_dir: &Path,
    ) -> Result<(Vec<View>, Vec<String>, Vec<PathBuf>), SemanticLayerError> {
        let mut views = Vec::new();
        let mut warnings = Vec::new();
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
                match self.parse_view_file(&path) {
                    Ok(view) => {
                        views.push(view);
                        parsed_files.push(path);
                    }
                    Err(e) => {
                        warnings.push(format!(
                            "Failed to parse view file {}: {}",
                            path.display(),
                            e
                        ));
                    }
                }
            }
        }

        Ok((views, warnings, parsed_files))
    }

    /// Parses all topic files from the given directory
    fn parse_topics(
        &self,
        topics_dir: &Path,
    ) -> Result<(Vec<Topic>, Vec<String>, Vec<PathBuf>), SemanticLayerError> {
        let mut topics = Vec::new();
        let warnings = Vec::new();
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
                let topic = self.parse_topic_file(&path).map_err(|e| {
                    SemanticLayerError::ParsingError(format!(
                        "Failed to parse topic file {}: {}",
                        path.display(),
                        e
                    ))
                })?;
                topics.push(topic);
                parsed_files.push(path);
            }
        }

        Ok((topics, warnings, parsed_files))
    }

    /// Parses a single view file
    fn parse_view_file(&self, path: &Path) -> Result<View, SemanticLayerError> {
        let content = fs::read_to_string(path).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        let view: View = serde_yaml::from_str(&content).map_err(|e| {
            SemanticLayerError::ParsingError(format!(
                "Failed to parse YAML in {}: {}",
                path.display(),
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

        let topic: Topic = serde_yaml::from_str(&content).map_err(|e| {
            SemanticLayerError::ParsingError(format!(
                "Failed to parse YAML in {}: {}",
                path.display(),
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
            SemanticLayerError::ParsingError(format!("Failed to parse YAML: {}", e))
        })?;

        Ok(semantic_layer)
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
) -> Result<ParseResult, SemanticLayerError> {
    let parser = SemanticLayerParser::with_base_path(path);
    parser.parse()
}
