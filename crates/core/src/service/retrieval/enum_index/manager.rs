use std::{collections::HashMap, fs, path::PathBuf};

use aho_corasick::AhoCorasick;
use once_cell::sync::OnceCell;
use rkyv::{self, Archived, Deserialize as RkyvDeserialize};

use crate::{
    adapters::{secrets::SecretsManager, vector_store::RetrievalObject},
    config::{
        ConfigManager,
        constants::{ENUM_ROUTING_PATH, RETRIEVAL_CACHE_PATH},
    },
    semantic::{SemanticManager, SemanticVariablesContexts},
};
use oxy_shared::errors::OxyError;

use super::{
    builder, renderer,
    types::{EnumRoutingBlob, RenderedRetrievalTemplate, SemanticEnum},
};

/// Configuration for enum index operations
#[derive(Debug, Clone)]
pub struct EnumIndexConfig {
    pub cache_path: PathBuf,
}

impl EnumIndexConfig {
    /// Get the path to the rkyv routing cache file
    pub fn routing_rkyv_path(&self) -> PathBuf {
        self.cache_path.join(format!("{}.rkyv", ENUM_ROUTING_PATH))
    }

    /// Get the path to the JSON routing cache file  
    pub fn routing_json_path(&self) -> PathBuf {
        self.cache_path.join(format!("{}.json", ENUM_ROUTING_PATH))
    }
}

static ENUM_INDEX: OnceCell<(AhoCorasick, EnumRoutingBlob)> = OnceCell::new();

/// Manages enum index caching and operations with centralized configuration
pub struct EnumIndexManager {
    pub(crate) config: EnumIndexConfig,
}

impl EnumIndexManager {
    /// Create a new EnumIndexManager with pre-resolved configuration
    pub fn new(config: EnumIndexConfig) -> Self {
        EnumIndexManager { config }
    }

    /// Create EnumIndexManager from ConfigManager
    /// If build_retrieval_objects is true, builds all retrieval objects from config.
    /// If false, uses empty retrieval objects (useful for minimal setups like VectorStore).
    pub async fn from_config(config: &ConfigManager) -> Result<Self, OxyError> {
        let cache_root = config
            .resolve_file(RETRIEVAL_CACHE_PATH)
            .await
            .unwrap_or_else(|_| RETRIEVAL_CACHE_PATH.to_string());
        let cache_path = PathBuf::from(&cache_root);

        let enum_config = EnumIndexConfig { cache_path };

        Ok(Self::new(enum_config))
    }

    /// One-shot initialization: create manager from config and initialize index
    /// Returns Ok(()) even if cache files don't exist (graceful degradation)
    pub async fn init_from_config(config: ConfigManager) -> Result<(), OxyError> {
        let manager = Self::from_config(&config).await?;

        let cache_exists = manager.config.routing_rkyv_path().exists()
            || manager.config.routing_json_path().exists();

        if !cache_exists {
            tracing::debug!(
                "Enum index cache files do not exist, skipping initialization. Please create the enum index cache files (see documentation for instructions)."
            );
            return Ok(());
        }

        match manager.init_index().await {
            Ok(_) => {
                tracing::debug!("Enum index initialized successfully");
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize enum index from cache: {}. Continuing without enum index.",
                    e
                );
                Ok(()) // Return Ok to allow graceful degradation
            }
        }
    }

    /// One-shot build and persist: create manager from config and build cache
    pub async fn build_from_config(
        config: &ConfigManager,
        secrets_manager: &SecretsManager,
        retrieval_objects: &Vec<RetrievalObject>,
    ) -> Result<(), OxyError> {
        // Exit early if no retrieval objects with inclusions
        let has_inclusions = retrieval_objects
            .iter()
            .any(|obj| !obj.inclusions.is_empty());
        if !has_inclusions {
            tracing::debug!("Enum index: no retrieval objects found; skipping build and persist");
            return Ok(());
        }

        let manager = Self::from_config(config).await?;

        // Collect enum semantic dimension enums. Semantic variables that cannot be enums (i.e. models.<model>.<dimension>)
        // and non-enum variables more generally are not supported for retrieval.
        let semantic_manager =
            SemanticManager::from_config(config.clone(), secrets_manager.clone(), false).await?;
        let semantic_variables_ctx =
            SemanticVariablesContexts::new(HashMap::new(), HashMap::new())?;
        let semantic_dimensions_ctx = semantic_manager
            .get_semantic_dimensions_contexts(&semantic_variables_ctx)
            .await?;
        let mut semantic_enums: Vec<SemanticEnum> = Vec::new();
        for (dim_name, schema) in semantic_dimensions_ctx.dimensions.iter() {
            if let Some(enum_values) = &schema.enum_values {
                let values: Vec<String> = enum_values
                    .iter()
                    .cloned()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .collect();
                if !values.is_empty() {
                    semantic_enums.push((format!("dimensions.{}", dim_name), values));
                }
            }
        }

        let has_semantic_enums = !semantic_enums.is_empty();

        // Check if any retrieval objects have enum variables
        let has_retrieval_enums = retrieval_objects.iter().any(|obj| {
            obj.enum_variables
                .as_ref()
                .is_some_and(|vars| !vars.is_empty())
        });

        if has_semantic_enums || has_retrieval_enums {
            manager
                .build_and_persist(retrieval_objects, &semantic_enums)
                .await?;
        } else {
            tracing::debug!("Enum index: no enum variables found; skipping build and persist");
        }

        Ok(())
    }

    /// Facade for callers: given a query string, produce rendered retrieval templates
    /// based on enum matches and routing. Internals (AC, routing, matching, rendering)
    /// are encapsulated within this manager.
    pub async fn render_items_for_query(
        &self,
        query: &str,
    ) -> Result<Vec<RenderedRetrievalTemplate>, OxyError> {
        let (ac, routing) = match self.get_index() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "Enum index unavailable at runtime: {}. Skipping enum retrieval.",
                    e
                );
                return Ok(Vec::new());
            }
        };

        let matches = renderer::find_enum_matches(ac, query);
        if matches.is_empty() {
            return Ok(Vec::new());
        }

        let templates = renderer::get_templates_to_render(routing, &matches);
        if templates.is_empty() {
            return Ok(Vec::new());
        }

        let mut rendered: Vec<RenderedRetrievalTemplate> = Vec::with_capacity(templates.len());
        for template in templates {
            let rendered_text = renderer::render_enum_template(template, &matches, routing)?;

            rendered.push(RenderedRetrievalTemplate {
                rendered_text,
                is_exclusion: template.is_exclusion,
                source_identifier: template.source_identifier.clone(),
                source_type: template.source_type.clone(),
                original_template: template.template.clone(),
            });
        }

        Ok(rendered)
    }

    /// Get the enum index without attempting to rebuild missing caches. Use at runtime/query time.
    pub fn get_index(&self) -> Result<&'static (AhoCorasick, EnumRoutingBlob), OxyError> {
        ENUM_INDEX.get().ok_or_else(|| {
            OxyError::RuntimeError(
                "Enum index not initialized. Run 'oxy build' or restart the server to initialize."
                    .into(),
            )
        })
    }

    /// Initialize the enum index, ensuring cache is ready (may rebuild). Use at build or app start time.
    pub async fn init_index(&self) -> Result<&'static (AhoCorasick, EnumRoutingBlob), OxyError> {
        match ENUM_INDEX.get_or_try_init(|| self.load_from_cache()) {
            Ok(val) => Ok(val),
            Err(err) => {
                tracing::warn!("Enum index cache load failed: {}.", err);
                Err(err)
            }
        }
    }

    /// Build and persist the enum index from workflow configurations
    async fn build_and_persist(
        &self,
        retrieval_objects: &[RetrievalObject],
        semantic_enums: &[SemanticEnum],
    ) -> Result<(), OxyError> {
        // Build routing blob in-memory using builder
        let routing_blob = builder::build_routing_blob(retrieval_objects, semantic_enums)?;

        // Persist both JSON (readable) and rkyv (runtime)
        fs::create_dir_all(&self.config.cache_path)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create cache dir: {e}")))?;

        // Write JSON
        let json_bytes = serde_json::to_vec_pretty(&routing_blob).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to serialize routing JSON: {e}"))
        })?;
        fs::write(self.config.routing_json_path(), json_bytes)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write routing JSON: {e}")))?;

        // Write rkyv
        let rkyv_bytes = rkyv::to_bytes::<_, 256>(&routing_blob)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to archive routing blob: {e}")))?;
        fs::write(self.config.routing_rkyv_path(), rkyv_bytes)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write routing rkyv: {e}")))?;

        Ok(())
    }

    /// Load enum index from cached files (prefer rkyv, fallback to JSON)
    fn load_from_cache(&self) -> Result<(AhoCorasick, EnumRoutingBlob), OxyError> {
        // Determine blob source: prefer rkyv, fallback to JSON
        let blob: EnumRoutingBlob = if self.config.routing_rkyv_path().exists() {
            match self.try_load_rkyv() {
                Ok(b) => b,
                Err(err) => {
                    tracing::warn!(
                        "Failed to load rkyv routing blob: {}. Falling back to JSON.",
                        err
                    );
                    self.try_load_json()?
                }
            }
        } else {
            self.try_load_json()?
        };

        // Build AC automaton from the enum values (patterns) in the blob
        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(blob.patterns.clone())
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build AC automaton: {e}")))?;

        Ok((ac, blob))
    }

    fn try_load_rkyv(&self) -> Result<EnumRoutingBlob, OxyError> {
        let bytes = fs::read(self.config.routing_rkyv_path())
            .map_err(|e| OxyError::RuntimeError(format!("Failed to read rkyv file: {e}")))?;
        let archived: &Archived<EnumRoutingBlob> =
            rkyv::check_archived_root::<EnumRoutingBlob>(&bytes).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to check archived routing blob: {e}"))
            })?;
        let blob: EnumRoutingBlob = archived.deserialize(&mut rkyv::Infallible).map_err(|_| {
            OxyError::RuntimeError("Failed to deserialize archived routing blob".into())
        })?;

        Ok(blob)
    }

    fn try_load_json(&self) -> Result<EnumRoutingBlob, OxyError> {
        let bytes = fs::read(self.config.routing_json_path())
            .map_err(|e| OxyError::RuntimeError(format!("Failed to read routing JSON: {e}")))?;
        let blob: EnumRoutingBlob = serde_json::from_slice(&bytes)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse routing JSON: {e}")))?;
        Ok(blob)
    }
}
