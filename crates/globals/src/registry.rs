use crate::errors::{GlobalError, GlobalResult};
use crate::parser::GlobalParser;
use crate::reference::GlobalReference;
use indexmap::IndexMap;
use minijinja::Value as JinjaValue;
use regex::Regex;
use serde_yaml::{Mapping, Value};
use std::path::Path;
use std::sync::{OnceLock, RwLock};

/// Registry for managing global objects with caching
///
/// The GlobalRegistry provides a centralized interface for loading and accessing
/// global objects from the `globals/` directory. It includes simple in-memory
/// caching to avoid repeated file reads within the same operation.
///
/// # Examples
///
/// ```rust,no_run
/// # use oxy_globals::{GlobalRegistry, GlobalReference};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let registry = GlobalRegistry::new("/path/to/globals");
///
/// // Load all global files
/// registry.load_all()?;
///
/// // Get a specific object
/// let object = registry.get_object_by_path("semantics", "entities.Customer")?;
///
/// // Or use a GlobalReference
/// let reference = GlobalReference::parse("globals.semantics.entities.Customer")?;
/// let object = registry.get_object_by_reference(&reference)?;
/// # Ok(())
/// # }
/// ```
pub struct GlobalRegistry {
    /// Parser for loading global files
    parser: GlobalParser,
    /// Cache of loaded global files (file_name -> parsed YAML)
    /// Using RwLock for interior mutability to allow caching in async/multi-threaded contexts
    cache: RwLock<IndexMap<String, IndexMap<String, Value>>>,
    /// Runtime overrides for global values (file_name.path -> value)
    /// These take precedence over values loaded from files
    overrides: RwLock<IndexMap<String, Value>>,
}

impl GlobalRegistry {
    /// Create a new GlobalRegistry with the given globals directory path
    ///
    /// # Arguments
    ///
    /// * `globals_dir` - The path to the globals directory
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oxy_globals::GlobalRegistry;
    ///
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// ```
    pub fn new<P: AsRef<Path>>(globals_dir: P) -> Self {
        Self {
            parser: GlobalParser::new(globals_dir),
            cache: RwLock::new(IndexMap::new()),
            overrides: RwLock::new(IndexMap::new()),
        }
    }

    /// Load all global files from the `globals/` directory into the cache
    ///
    /// This method scans the globals directory and loads all YAML files into memory.
    /// Subsequent calls to get_object will use the cached data instead of reading
    /// from disk again.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The globals directory cannot be read
    /// - Any global file cannot be parsed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// registry.load_all()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_all(&self) -> GlobalResult<()> {
        let files = self.parser.list_global_files()?;
        let mut cache = self.cache.write().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire write lock: {}", e))
        })?;

        for file_name in files {
            if !cache.contains_key(&file_name) {
                let content = self.parser.load_file(&file_name)?;
                cache.insert(file_name, content);
            }
        }

        Ok(())
    }

    /// Load a specific global file into the cache if not already loaded
    ///
    /// This method loads a single global file on-demand. If the file is already
    /// in the cache, it does nothing.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// registry.load_file("semantics")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_file(&self, file_name: &str) -> GlobalResult<()> {
        let mut cache = self.cache.write().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire write lock: {}", e))
        })?;

        if !cache.contains_key(file_name) {
            let content = self.parser.load_file(file_name)?;
            cache.insert(file_name.to_string(), content);
        }

        Ok(())
    }

    /// Set an override value for a specific global reference path
    ///
    /// Overrides take precedence over values loaded from files.
    /// The key format is "file_name.path" (e.g., "semantics.entities.Customer").
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)
    /// * `path` - The path to the object (e.g., "entities.Customer")
    /// * `value` - The value to set as an override
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # use serde_yaml::Value;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let value = serde_yaml::from_str("{ name: 'Customer', id: 'customer_override' }")?;
    /// registry.set_override("semantics", "entities.Customer", value)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_override(&self, file_name: &str, path: &str, value: Value) -> GlobalResult<()> {
        let mut overrides = self.overrides.write().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire write lock for overrides: {}", e))
        })?;
        let key = format!("{}.{}", file_name, path);
        overrides.insert(key, value);
        Ok(())
    }

    /// Set multiple override values at once
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)  
    /// * `overrides_map` - A map of path -> value pairs
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # use indexmap::IndexMap;
    /// # use serde_yaml::Value;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let mut overrides = IndexMap::new();
    /// overrides.insert("entities.Customer".to_string(), serde_yaml::from_str("{ name: 'Customer' }")?);
    /// registry.set_overrides("semantics", overrides)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_overrides(
        &self,
        file_name: &str,
        overrides_map: IndexMap<String, Value>,
    ) -> GlobalResult<()> {
        for (path, value) in overrides_map {
            self.set_override(file_name, &path, value)?;
        }
        Ok(())
    }

    /// Clear all overrides
    pub fn clear_overrides(&self) -> GlobalResult<()> {
        let mut overrides = self.overrides.write().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire write lock for overrides: {}", e))
        })?;
        overrides.clear();
        Ok(())
    }

    /// Get a read-only reference to all current overrides
    ///
    /// # Returns
    ///
    /// A Result containing a read guard to the overrides map
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let overrides = registry.get_overrides()?;
    /// for (key, value) in overrides.iter() {
    ///     println!("Override: {} = {:?}", key, value);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_overrides(
        &self,
    ) -> GlobalResult<std::sync::RwLockReadGuard<'_, IndexMap<String, Value>>> {
        self.overrides.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock for overrides: {}", e))
        })
    }

    /// Apply global overrides from external sources (e.g., API calls)
    ///
    /// This method accepts a map of full global reference paths (e.g., "semantics.entities.customer")
    /// to their override values. This is useful for runtime configuration changes.
    ///
    /// # Arguments
    ///
    /// * `global_overrides` - A map of global reference paths to their values
    ///   Keys should be in the format "file_name.object_path" (e.g., "semantics.entities.customer")
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # use indexmap::IndexMap;
    /// # use serde_yaml::Value;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    ///
    /// let mut overrides = IndexMap::new();
    /// overrides.insert(
    ///     "semantics.entities.customer".to_string(),
    ///     serde_yaml::from_str(r#"{ "name": "Customer", "type": "primary" }"#)?
    /// );
    /// overrides.insert(
    ///     "tables.production.users".to_string(),
    ///     Value::String("override_users_table".to_string())
    /// );
    ///
    /// registry.apply_global_overrides(overrides)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn apply_global_overrides(
        &self,
        global_overrides: IndexMap<String, Value>,
    ) -> GlobalResult<()> {
        let mut overrides = self.overrides.write().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire write lock for overrides: {}", e))
        })?;

        for (global_path, value) in global_overrides {
            // Validate that the global path has the correct format
            if !global_path.contains('.') {
                return Err(GlobalError::InvalidObjectPath(format!(
                    "Global override path must contain at least one dot: {}",
                    global_path
                )));
            }

            overrides.insert(global_path, value);
        }

        Ok(())
    }

    /// Validate that template paths exist in the global files
    ///
    /// This method checks that all template references in a YAML value
    /// point to existing global objects or override values.
    ///
    /// # Arguments
    ///
    /// * `value` - The YAML value to validate for template references
    ///
    /// # Returns
    ///
    /// A vector of validation errors, empty if all template references are valid
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # use serde_yaml::Value;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let template = Value::String("{{globals.tables.production.users}}".to_string());
    /// let errors = registry.validate_template_paths(&template)?;
    /// if !errors.is_empty() {
    ///     println!("Template validation errors: {:?}", errors);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn validate_template_paths(&self, value: &Value) -> GlobalResult<Vec<String>> {
        static TEMPLATE_REGEX: OnceLock<Regex> = OnceLock::new();
        let regex = TEMPLATE_REGEX.get_or_init(|| {
            Regex::new(r"\{\{globals\.([^}]+)\}\}").expect("Failed to compile template regex")
        });

        let mut errors = Vec::new();
        self.collect_template_validation_errors(value, regex, &mut errors)?;
        Ok(errors)
    }

    /// Helper method to recursively collect template validation errors
    fn collect_template_validation_errors(
        &self,
        value: &Value,
        regex: &Regex,
        errors: &mut Vec<String>,
    ) -> GlobalResult<()> {
        match value {
            Value::String(s) => {
                for captures in regex.captures_iter(s) {
                    let path = captures.get(1).unwrap().as_str();
                    if let Err(e) = self.resolve_global_path_for_validation(path) {
                        errors.push(format!(
                            "Template {{{{globals.{}}}}} is invalid: {}",
                            path, e
                        ));
                    }
                }
            }
            Value::Mapping(m) => {
                for (k, v) in m {
                    self.collect_template_validation_errors(k, regex, errors)?;
                    self.collect_template_validation_errors(v, regex, errors)?;
                }
            }
            Value::Sequence(seq) => {
                for item in seq {
                    self.collect_template_validation_errors(item, regex, errors)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Helper method to resolve a global path for validation purposes
    fn resolve_global_path_for_validation(&self, path: &str) -> GlobalResult<Value> {
        // Parse the path into file and object path components
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return Err(GlobalError::InvalidObjectPath(format!(
                "Empty global path in template: {}",
                path
            )));
        }

        let file_name = parts[0];
        let object_path = if parts.len() > 1 {
            parts[1..].join(".")
        } else {
            return Err(GlobalError::InvalidObjectPath(format!(
                "Global path must have at least file.path format: {}",
                path
            )));
        };

        // Check for runtime overrides first
        let override_key = format!("{}.{}", file_name, object_path);
        if let Ok(overrides) = self.get_overrides()
            && let Some(override_value) = overrides.get(&override_key)
        {
            return Ok(override_value.clone());
        }

        // Fallback to file-based values
        self.get_object_by_path(file_name, &object_path)
    }

    /// Get an object by file name and path (e.g., "entities.customer")
    ///
    /// This method retrieves a specific object from a global file using a path.
    /// If the file is not already in the cache, it will be loaded on-demand.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)
    /// * `path` - The path to the object (e.g., "entities.customer")
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file does not exist
    /// - The path is invalid or object is not found
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let object = registry.get_object_by_path("semantics", "entities.Customer")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_object_by_path(&self, file_name: &str, path: &str) -> GlobalResult<Value> {
        // Check if there's an override for this exact path
        let override_key = format!("{}.{}", file_name, path);
        let overrides = self.overrides.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock for overrides: {}", e))
        })?;

        if let Some(override_value) = overrides.get(&override_key) {
            // Return the override value directly
            return Ok(override_value.clone());
        }
        drop(overrides); // Release the lock before potentially expensive file operations

        // Ensure the file is loaded
        self.load_file(file_name)?;

        // Get from cache
        let cache = self.cache.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock: {}", e))
        })?;
        let yaml = cache.get(file_name).ok_or_else(|| {
            GlobalError::FileNotFound(format!("Global file '{}' not loaded", file_name))
        })?;

        // Split path into components
        let components: Vec<&str> = path.split('.').collect();

        if components.is_empty() {
            return Err(GlobalError::InvalidObjectPath(format!(
                "Path '{}' is empty",
                path
            )));
        }

        // Navigate through the path
        let mut current_value: Value = Value::Mapping(
            yaml.iter()
                .map(|(k, v)| (Value::String(k.clone()), v.clone()))
                .collect(),
        );

        for (index, component) in components.iter().enumerate() {
            match &current_value {
                // Array format: [{ name: "Customer", ... }, ...]
                Value::Sequence(items) => {
                    let mut found = false;
                    for item in items {
                        if let Value::Mapping(map) = item
                            && let Some(Value::String(name)) =
                                map.get(Value::String("name".to_string()))
                            && name == component
                        {
                            current_value = item.clone();
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(GlobalError::ObjectNotFound(format!(
                            "Object '{}' not found in path '{}' (component {} of {})",
                            component,
                            path,
                            index + 1,
                            components.len()
                        )));
                    }
                }
                // Map format: { customer_id: { ... }, ... }
                Value::Mapping(map) => {
                    current_value = map
                        .get(Value::String(component.to_string()))
                        .cloned()
                        .ok_or_else(|| {
                            GlobalError::ObjectNotFound(format!(
                                "Key '{}' not found in path '{}' (component {} of {})",
                                component,
                                path,
                                index + 1,
                                components.len()
                            ))
                        })?;
                }
                _ => {
                    return Err(GlobalError::InvalidYamlStructure {
                        file: file_name.to_string(),
                        error: format!(
                            "Expected array or map at path component '{}', found {:?}",
                            component, current_value
                        ),
                    });
                }
            }
        }

        Ok(current_value)
    }

    /// Get an object using a GlobalReference
    ///
    /// This is a convenience method that accepts a GlobalReference and retrieves
    /// the corresponding object.
    ///
    /// # Arguments
    ///
    /// * `reference` - A parsed GlobalReference
    ///
    /// # Errors
    ///
    /// Returns an error if the object cannot be found
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::{GlobalRegistry, GlobalReference};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let reference = GlobalReference::parse("globals.semantics.entities.Customer")?;
    /// let object = registry.get_object_by_reference(&reference)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_object_by_reference(&self, reference: &GlobalReference) -> GlobalResult<Value> {
        self.get_object_by_path(&reference.file_name, &reference.path)
    }

    /// Get an object by parsing a reference string
    ///
    /// This is a convenience method that parses a reference string and retrieves
    /// the corresponding object in one call.
    ///
    /// # Arguments
    ///
    /// * `reference_str` - A global reference string (e.g., "globals.semantics.entities.Customer")
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The reference string is invalid
    /// - The object cannot be found
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let object = registry.get_object_by_string("globals.semantics.entities.Customer")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_object_by_string(&self, reference_str: &str) -> GlobalResult<Value> {
        let reference = GlobalReference::parse(reference_str)?;
        self.get_object_by_reference(&reference)
    }

    /// List all loaded global files
    ///
    /// Returns the names of all files currently in the cache.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// registry.load_all()?;
    /// let files = registry.list_loaded_files();
    /// println!("Loaded files: {:?}", files);
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_loaded_files(&self) -> Vec<String> {
        let cache = self.cache.read().expect("Failed to acquire read lock");
        cache.keys().cloned().collect()
    }

    /// List all available global files in the globals directory
    ///
    /// This method scans the globals directory and returns all available files,
    /// whether or not they are currently loaded in the cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the globals directory cannot be read
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let files = registry.list_available_files()?;
    /// println!("Available files: {:?}", files);
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_available_files(&self) -> GlobalResult<Vec<String>> {
        self.parser.list_global_files()
    }

    /// Check if a file is currently loaded in the cache
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// if !registry.is_file_loaded("semantics") {
    ///     registry.load_file("semantics")?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_file_loaded(&self, file_name: &str) -> bool {
        let cache = self.cache.read().expect("Failed to acquire read lock");
        cache.contains_key(file_name)
    }

    /// Check if an object exists using a path
    ///
    /// This method checks if an object exists by attempting to retrieve it.
    /// It returns true if the object exists, false otherwise.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)
    /// * `path` - The path to the object (e.g., "entities.customer")
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oxy_globals::GlobalRegistry;
    ///
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// if registry.object_exists_by_path("semantics", "entities.Customer") {
    ///     println!("Customer entity exists!");
    /// }
    /// ```
    pub fn object_exists_by_path(&self, file_name: &str, path: &str) -> bool {
        self.get_object_by_path(file_name, path).is_ok()
    }

    /// Check if an object exists using a GlobalReference
    ///
    /// # Arguments
    ///
    /// * `reference` - A parsed GlobalReference
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::{GlobalRegistry, GlobalReference};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let reference = GlobalReference::parse("globals.semantics.entities.Customer")?;
    /// if registry.object_exists_by_reference(&reference) {
    ///     println!("Object exists!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn object_exists_by_reference(&self, reference: &GlobalReference) -> bool {
        self.get_object_by_reference(reference).is_ok()
    }

    /// Build a Jinja2-compatible context object from all loaded globals
    ///
    /// This method creates a nested structure suitable for use with Jinja2 templating.
    /// The returned value can be used as part of a Jinja2 context, allowing templates
    /// to reference globals using `{{globals.file.path.to.value}}` syntax.
    ///
    /// # Returns
    ///
    /// A Result containing a minijinja Value representing all globals in a nested structure
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # use minijinja::Environment;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// registry.load_all()?;
    ///
    /// let globals_context = registry.to_jinja_context()?;
    ///
    /// // Use in minijinja template
    /// let mut env = Environment::new();
    /// let template = env.template_from_str("{{ globals.semantics.entities.customer.name }}")?;
    /// let context = minijinja::context! { globals => globals_context };
    /// let result = template.render(context)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_jinja_context(&self) -> GlobalResult<JinjaValue> {
        // Load all global files if not already loaded
        self.load_all()?;

        let cache = self.cache.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock: {}", e))
        })?;

        let overrides = self.overrides.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock for overrides: {}", e))
        })?;

        // Convert the entire cache to a JSON-compatible structure
        let mut globals_map = serde_json::Map::new();

        for (file_name, file_content) in cache.iter() {
            // Convert serde_yaml::Value to serde_json::Value
            let json_value = yaml_to_json_value(file_content)?;
            globals_map.insert(file_name.clone(), json_value);
        }

        // Apply overrides
        for (override_key, override_value) in overrides.iter() {
            // Parse override_key (format: "file_name.path.to.value")
            let parts: Vec<&str> = override_key.split('.').collect();
            if parts.len() < 2 {
                continue;
            }

            let file_name = parts[0];
            let path_parts = &parts[1..];

            // Ensure the file exists in the map
            if !globals_map.contains_key(file_name) {
                globals_map.insert(
                    file_name.to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }

            // Convert override value to JSON
            let json_override = yaml_to_json_single(override_value)?;

            // Navigate and set the override value
            if let Some(serde_json::Value::Object(file_obj)) = globals_map.get_mut(file_name) {
                set_nested_value(file_obj, path_parts, json_override);
            }
        }

        // Convert to minijinja Value
        let json_globals = serde_json::Value::Object(globals_map);
        Ok(JinjaValue::from_serialize(&json_globals))
    }

    /// Clear the cache
    ///
    /// This method clears all loaded files from the cache. Subsequent calls to
    /// get_object will reload files from disk.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// registry.load_all()?;
    /// // ... do some work ...
    /// registry.clear_cache(); // Force reload on next access
    /// # Ok(())
    /// # }
    /// ```
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().expect("Failed to acquire write lock");
        cache.clear();
    }

    /// Get the number of files currently in the cache
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// registry.load_all()?;
    /// println!("Cached files: {}", registry.cache_size());
    /// # Ok(())
    /// # }
    /// ```
    pub fn cache_size(&self) -> usize {
        let cache = self.cache.read().expect("Failed to acquire read lock");
        cache.len()
    }

    /// Check if the globals directory exists
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oxy_globals::GlobalRegistry;
    ///
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// if !registry.globals_dir_exists() {
    ///     println!("No globals directory found");
    /// }
    /// ```
    pub fn globals_dir_exists(&self) -> bool {
        self.parser.globals_dir_exists()
    }

    /// Get all objects at a specific path from a file
    ///
    /// This method retrieves all objects at a given path (e.g., "entities",
    /// "dimensions") from a global file.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)
    /// * `path` - The path to the collection (e.g., "entities", "dimensions", "measures")
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file does not exist
    /// - The path is not found in the file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let entities = registry.get_all_at_path("semantics", "entities")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_all_at_path(&self, file_name: &str, path: &str) -> GlobalResult<Value> {
        // Ensure the file is loaded
        self.load_file(file_name)?;

        // Get from cache
        let cache = self.cache.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock: {}", e))
        })?;
        let yaml = cache.get(file_name).ok_or_else(|| {
            GlobalError::FileNotFound(format!("Global file '{}' not loaded", file_name))
        })?;

        // Get the value at path
        yaml.get(path).cloned().ok_or_else(|| {
            GlobalError::InvalidObjectPath(format!(
                "Path '{}' not found in file '{}'",
                path, file_name
            ))
        })
    }

    /// Get the entire content of a global file
    ///
    /// This method returns the full parsed YAML content of a global file.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the global file (without .yml extension)
    ///
    /// # Errors
    ///
    /// Returns an error if the file does not exist or cannot be parsed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// let content = registry.get_file_content("semantics")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_file_content(&self, file_name: &str) -> GlobalResult<IndexMap<String, Value>> {
        // Ensure the file is loaded
        self.load_file(file_name)?;

        // Get from cache
        let cache = self.cache.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock: {}", e))
        })?;
        cache.get(file_name).cloned().ok_or_else(|| {
            GlobalError::FileNotFound(format!("Global file '{}' not loaded", file_name))
        })
    }

    /// Resolve all global references in a value by traversing the object tree
    ///
    /// This method recursively traverses a JSON/YAML value and replaces all strings
    /// that match the global reference pattern (globals.file.type.name) with their
    /// actual resolved values from the global registry.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to resolve (can be any YAML/JSON value)
    ///
    /// # Returns
    ///
    /// A new value with all global references resolved to their actual values
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A global reference is invalid or doesn't exist
    /// - A file cannot be loaded or parsed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # use serde_yaml::Value;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    ///
    /// // Create a value with global references
    /// let input = serde_yaml::from_str::<Value>(r#"
    /// entity:
    ///   type: globals.semantics.entities.customer
    ///   dimensions:
    ///     - globals.semantics.dimensions.customer_id
    ///     - name: custom_field
    ///       type: string
    /// "#)?;
    ///
    /// // Resolve all references
    /// let resolved = registry.resolve_all_references(&input)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn resolve_all_references(&self, value: &Value) -> GlobalResult<Value> {
        match value {
            // String: Check if it's a global reference and resolve it
            Value::String(s) => {
                if GlobalReference::is_global_reference(s) {
                    let reference = GlobalReference::parse(s)?;
                    self.get_object_by_reference(&reference)
                } else {
                    Ok(value.clone())
                }
            }
            // Mapping: Recursively resolve all values
            // Note: Only values are resolved, keys remain unchanged to preserve mapping structure
            Value::Mapping(map) => {
                let mut resolved_map = Mapping::new();
                for (key, val) in map {
                    let resolved_val = self.resolve_all_references(val)?;
                    resolved_map.insert(key.clone(), resolved_val);
                }
                Ok(Value::Mapping(resolved_map))
            }
            // Sequence: Recursively resolve all elements
            Value::Sequence(seq) => {
                let mut resolved_seq = Vec::new();
                for item in seq {
                    resolved_seq.push(self.resolve_all_references(item)?);
                }
                Ok(Value::Sequence(resolved_seq))
            }
            // All other types: Return as-is (null, bool, number, tagged)
            _ => Ok(value.clone()),
        }
    }

    /// Resolve all global references in a value with inheritance support
    ///
    /// This method is similar to `resolve_all_references`, but also handles
    /// `inherits_from` fields by using the ObjectInheritanceEngine to merge
    /// parent and child objects.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to resolve (can be any YAML/JSON value)
    ///
    /// # Returns
    ///
    /// A new value with all global references resolved and inheritance applied
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A global reference is invalid or doesn't exist
    /// - Circular inheritance is detected
    /// - A file cannot be loaded or parsed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # use serde_yaml::Value;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    ///
    /// // Create a value with inheritance
    /// let input = serde_yaml::from_str::<Value>(r#"
    /// entity:
    ///   inherits_from: globals.semantics.entities.customer
    ///   description: Extended customer entity
    /// "#)?;
    ///
    /// // Resolve all references with inheritance
    /// let resolved = registry.resolve_with_inheritance(&input)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn resolve_with_inheritance(&self, value: &Value) -> GlobalResult<Value> {
        use crate::inheritance::ObjectInheritanceEngine;

        match value {
            // String: Check if it's a global reference and resolve it
            Value::String(s) => {
                if GlobalReference::is_global_reference(s) {
                    let reference = GlobalReference::parse(s)?;
                    self.get_object_by_reference(&reference)
                } else {
                    Ok(value.clone())
                }
            }
            // Mapping: Check for inherits_from first, then resolve nested references
            Value::Mapping(map) => {
                // Check if this mapping has an inherits_from field
                let has_inheritance = map
                    .get(Value::String("inherits_from".to_string()))
                    .and_then(|v| v.as_str())
                    .map(GlobalReference::is_global_reference)
                    .unwrap_or(false);

                if has_inheritance {
                    // Apply inheritance first (this handles the inherits_from field)
                    let engine = ObjectInheritanceEngine::new(self.clone());
                    let mut value_mut = value.clone();
                    let inherited = engine.resolve_inheritance(&mut value_mut)?;

                    // Then resolve any remaining nested references in the result
                    self.resolve_all_references(&inherited)
                } else {
                    // No inheritance, just resolve nested references
                    // Note: Only values are resolved, keys remain unchanged to preserve mapping structure
                    let mut resolved_map = Mapping::new();
                    for (key, val) in map {
                        let resolved_val = self.resolve_with_inheritance(val)?;
                        resolved_map.insert(key.clone(), resolved_val);
                    }
                    Ok(Value::Mapping(resolved_map))
                }
            }
            // Sequence: Recursively resolve all elements
            Value::Sequence(seq) => {
                let mut resolved_seq = Vec::new();
                for item in seq {
                    resolved_seq.push(self.resolve_with_inheritance(item)?);
                }
                Ok(Value::Sequence(resolved_seq))
            }
            // All other types: Return as-is
            _ => Ok(value.clone()),
        }
    }

    /// Get all globals as a combined YAML value for use in template contexts
    ///
    /// This method returns all loaded global files combined into a single mapping,
    /// where each top-level key is the file name (e.g., "tables", "semantics").
    /// Runtime overrides are applied to the file contents before returning.
    ///
    /// # Returns
    ///
    /// A YAML mapping containing all global files, with overrides applied
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use oxy_globals::GlobalRegistry;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = GlobalRegistry::new("/path/to/globals");
    /// registry.load_all()?;
    /// let globals = registry.get_all_globals()?;
    /// // globals will contain { "tables": {...}, "semantics": {...}, ... }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_all_globals(&self) -> GlobalResult<Value> {
        // Load all files if not already loaded
        self.load_all()?;

        // Get the cache
        let cache = self.cache.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock: {}", e))
        })?;

        // Get overrides
        let overrides = self.overrides.read().map_err(|e| {
            GlobalError::FileReadError(format!("Failed to acquire read lock for overrides: {}", e))
        })?;

        // Build a combined mapping
        let mut combined = Mapping::new();

        for (file_name, file_content) in cache.iter() {
            // Start with the file content
            let mut file_map = Mapping::new();
            for (key, value) in file_content {
                file_map.insert(Value::String(key.clone()), value.clone());
            }

            // Apply any overrides for this file
            for (override_key, override_value) in overrides.iter() {
                // Override keys are in format "file_name.path"
                if let Some(stripped) = override_key.strip_prefix(&format!("{}.", file_name)) {
                    // Parse the path and apply the override
                    let path_parts: Vec<&str> = stripped.split('.').collect();
                    Self::apply_override_to_mapping(
                        &mut file_map,
                        &path_parts,
                        override_value.clone(),
                    )?;
                }
            }

            combined.insert(Value::String(file_name.clone()), Value::Mapping(file_map));
        }

        Ok(Value::Mapping(combined))
    }

    /// Helper method to apply an override value at a specific path in a mapping
    fn apply_override_to_mapping(
        mapping: &mut Mapping,
        path: &[&str],
        value: Value,
    ) -> GlobalResult<()> {
        if path.is_empty() {
            return Ok(());
        }

        if path.len() == 1 {
            // Base case: set the value
            mapping.insert(Value::String(path[0].to_string()), value);
            return Ok(());
        }

        // Recursive case: navigate deeper
        let key = Value::String(path[0].to_string());
        let nested = mapping
            .entry(key.clone())
            .or_insert_with(|| Value::Mapping(Mapping::new()));

        if let Value::Mapping(nested_map) = nested {
            Self::apply_override_to_mapping(nested_map, &path[1..], value)?;
        }

        Ok(())
    }
}

// Implement Clone for GlobalRegistry to support ObjectInheritanceEngine
impl Clone for GlobalRegistry {
    fn clone(&self) -> Self {
        Self {
            parser: GlobalParser::new(self.parser.globals_dir()),
            cache: RwLock::new(
                self.cache
                    .read()
                    .expect("Failed to acquire read lock")
                    .clone(),
            ),
            overrides: RwLock::new(
                self.overrides
                    .read()
                    .expect("Failed to acquire read lock for overrides")
                    .clone(),
            ),
        }
    }
}

/// Helper function to convert a serde_yaml IndexMap to serde_json::Value
fn yaml_to_json_value(yaml_map: &IndexMap<String, Value>) -> GlobalResult<serde_json::Value> {
    let mut json_map = serde_json::Map::new();

    for (key, value) in yaml_map {
        let json_value = yaml_to_json_single(value)?;
        json_map.insert(key.clone(), json_value);
    }

    Ok(serde_json::Value::Object(json_map))
}

/// Helper function to convert a single serde_yaml::Value to serde_json::Value
fn yaml_to_json_single(value: &Value) -> GlobalResult<serde_json::Value> {
    match value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(serde_json::Value::Number(i.into()))
            } else if let Some(u) = n.as_u64() {
                Ok(serde_json::Value::Number(u.into()))
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| GlobalError::InvalidYamlStructure {
                        file: "conversion".to_string(),
                        error: format!("Invalid float value: {}", f),
                    })
            } else {
                Err(GlobalError::InvalidYamlStructure {
                    file: "conversion".to_string(),
                    error: format!("Unsupported number type: {:?}", n),
                })
            }
        }
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Sequence(seq) => {
            let mut json_array = Vec::new();
            for item in seq {
                json_array.push(yaml_to_json_single(item)?);
            }
            Ok(serde_json::Value::Array(json_array))
        }
        Value::Mapping(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                if let Value::String(key_str) = k {
                    json_map.insert(key_str.clone(), yaml_to_json_single(v)?);
                } else {
                    return Err(GlobalError::InvalidYamlStructure {
                        file: "conversion".to_string(),
                        error: format!("Non-string mapping key: {:?}", k),
                    });
                }
            }
            Ok(serde_json::Value::Object(json_map))
        }
        Value::Tagged(tagged) => yaml_to_json_single(&tagged.value),
    }
}

/// Helper function to set a nested value in a JSON object
fn set_nested_value(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
    value: serde_json::Value,
) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        obj.insert(path[0].to_string(), value);
        return;
    }

    let key = path[0];
    let remaining_path = &path[1..];

    if !obj.contains_key(key) {
        obj.insert(
            key.to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
    }

    if let Some(serde_json::Value::Object(nested_obj)) = obj.get_mut(key) {
        set_nested_value(nested_obj, remaining_path, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Mapping;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_globals(temp_dir: &TempDir) {
        let globals_dir = temp_dir.path().join("globals");
        fs::create_dir(&globals_dir).unwrap();

        // Create semantics.yml
        let semantics_content = r#"
entities:
  - name: Customer
    type: primary
    key: customer_id
  - name: Product
    type: foreign
    key: product_id

dimensions:
  customer_id:
    type: number
    expr: customer_id
  product_name:
    type: string
    expr: name

measures:
  total_sales:
    type: sum
    expr: amount
"#;
        fs::write(globals_dir.join("semantics.yml"), semantics_content).unwrap();

        // Create another file for testing
        let config_content = r#"
settings:
  - name: timeout
    value: 30
"#;
        fs::write(globals_dir.join("config.yml"), config_content).unwrap();
    }

    #[test]
    fn test_new_registry() {
        let temp_dir = TempDir::new().unwrap();
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));
        assert_eq!(registry.cache_size(), 0);
    }

    #[test]
    fn test_load_all() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        registry.load_all().unwrap();
        assert_eq!(registry.cache_size(), 2);
        assert!(registry.is_file_loaded("semantics"));
        assert!(registry.is_file_loaded("config"));
    }

    #[test]
    fn test_load_file() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        assert!(!registry.is_file_loaded("semantics"));
        registry.load_file("semantics").unwrap();
        assert!(registry.is_file_loaded("semantics"));
        assert_eq!(registry.cache_size(), 1);
    }

    #[test]
    fn test_load_file_twice() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        registry.load_file("semantics").unwrap();
        registry.load_file("semantics").unwrap(); // Should not fail
        assert_eq!(registry.cache_size(), 1);
    }

    #[test]
    fn test_get_object_array_format() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let object = registry
            .get_object_by_path("semantics", "entities.Customer")
            .unwrap();

        if let Value::Mapping(map) = object {
            assert_eq!(
                map.get(&Value::String("name".to_string())),
                Some(&Value::String("Customer".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_get_object_map_format() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let object = registry
            .get_object_by_path("semantics", "dimensions.customer_id")
            .unwrap();

        if let Value::Mapping(map) = object {
            assert_eq!(
                map.get(&Value::String("type".to_string())),
                Some(&Value::String("number".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_get_object_by_reference() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let reference = GlobalReference::parse("globals.semantics.entities.Customer").unwrap();
        let object = registry.get_object_by_reference(&reference).unwrap();

        if let Value::Mapping(map) = object {
            assert_eq!(
                map.get(&Value::String("name".to_string())),
                Some(&Value::String("Customer".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_get_object_by_string() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let object = registry
            .get_object_by_string("globals.semantics.entities.Product")
            .unwrap();

        if let Value::Mapping(map) = object {
            assert_eq!(
                map.get(&Value::String("name".to_string())),
                Some(&Value::String("Product".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_get_object_not_found() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let result = registry.get_object_by_path("semantics", "entities.NonExistent");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GlobalError::ObjectNotFound(_)
        ));
    }

    #[test]
    fn test_list_loaded_files() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        registry.load_file("semantics").unwrap();
        let files = registry.list_loaded_files();
        assert_eq!(files.len(), 1);
        assert!(files.contains(&"semantics".to_string()));
    }

    #[test]
    fn test_list_available_files() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let files = registry.list_available_files().unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"semantics".to_string()));
        assert!(files.contains(&"config".to_string()));
    }

    #[test]
    fn test_object_exists() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        assert!(registry.object_exists_by_path("semantics", "entities.Customer"));
        assert!(!registry.object_exists_by_path("semantics", "entities.NonExistent"));
    }

    #[test]
    fn test_object_exists_by_reference() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let reference = GlobalReference::parse("globals.semantics.entities.Customer").unwrap();
        assert!(registry.object_exists_by_reference(&reference));

        let reference = GlobalReference::parse("globals.semantics.entities.NonExistent").unwrap();
        assert!(!registry.object_exists_by_reference(&reference));
    }

    #[test]
    fn test_clear_cache() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        registry.load_all().unwrap();
        assert_eq!(registry.cache_size(), 2);

        registry.clear_cache();
        assert_eq!(registry.cache_size(), 0);
    }

    #[test]
    fn test_get_all_at_path() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let entities = registry.get_all_at_path("semantics", "entities").unwrap();

        if let Value::Sequence(items) = entities {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected sequence");
        }
    }

    #[test]
    fn test_get_file_content() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let content = registry.get_file_content("semantics").unwrap();
        assert!(content.contains_key("entities"));
        assert!(content.contains_key("dimensions"));
        assert!(content.contains_key("measures"));
    }

    #[test]
    fn test_globals_dir_exists() {
        let temp_dir = TempDir::new().unwrap();
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));
        assert!(!registry.globals_dir_exists());

        create_test_globals(&temp_dir);
        assert!(registry.globals_dir_exists());
    }

    #[test]
    fn test_caching_avoids_duplicate_reads() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // First access loads from file
        let _object1 = registry
            .get_object_by_path("semantics", "entities.Customer")
            .unwrap();
        assert_eq!(registry.cache_size(), 1);

        // Second access uses cache
        let _object2 = registry
            .get_object_by_path("semantics", "entities.Product")
            .unwrap();
        assert_eq!(registry.cache_size(), 1); // Still only 1 file in cache
    }

    #[test]
    fn test_resolve_all_references_simple() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // Create a simple value with a global reference
        let input = Value::String("globals.semantics.entities.Customer".to_string());
        let resolved = registry.resolve_all_references(&input).unwrap();

        // Should be resolved to the actual entity
        if let Value::Mapping(map) = resolved {
            assert_eq!(
                map.get(&Value::String("name".to_string())),
                Some(&Value::String("Customer".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_resolve_all_references_in_mapping() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // Create a mapping with global references
        let mut input_map = Mapping::new();
        input_map.insert(
            Value::String("entity".to_string()),
            Value::String("globals.semantics.entities.Customer".to_string()),
        );
        input_map.insert(
            Value::String("static_field".to_string()),
            Value::String("not a reference".to_string()),
        );
        let input = Value::Mapping(input_map);

        let resolved = registry.resolve_all_references(&input).unwrap();

        // Check the resolved value
        if let Value::Mapping(map) = resolved {
            // entity should be resolved
            if let Some(Value::Mapping(entity_map)) = map.get(&Value::String("entity".to_string()))
            {
                assert_eq!(
                    entity_map.get(&Value::String("name".to_string())),
                    Some(&Value::String("Customer".to_string()))
                );
            } else {
                panic!("Expected entity to be resolved to mapping");
            }

            // static_field should remain unchanged
            assert_eq!(
                map.get(&Value::String("static_field".to_string())),
                Some(&Value::String("not a reference".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_resolve_all_references_in_sequence() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // Create a sequence with global references
        let input = Value::Sequence(vec![
            Value::String("globals.semantics.entities.Customer".to_string()),
            Value::String("not a reference".to_string()),
            Value::String("globals.semantics.entities.Product".to_string()),
        ]);

        let resolved = registry.resolve_all_references(&input).unwrap();

        if let Value::Sequence(seq) = resolved {
            assert_eq!(seq.len(), 3);

            // First should be resolved
            if let Value::Mapping(map) = &seq[0] {
                assert_eq!(
                    map.get(&Value::String("name".to_string())),
                    Some(&Value::String("Customer".to_string()))
                );
            } else {
                panic!("Expected first element to be resolved to mapping");
            }

            // Second should remain unchanged
            assert_eq!(seq[1], Value::String("not a reference".to_string()));

            // Third should be resolved
            if let Value::Mapping(map) = &seq[2] {
                assert_eq!(
                    map.get(&Value::String("name".to_string())),
                    Some(&Value::String("Product".to_string()))
                );
            } else {
                panic!("Expected third element to be resolved to mapping");
            }
        } else {
            panic!("Expected sequence");
        }
    }

    #[test]
    fn test_resolve_all_references_nested() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // Create a deeply nested structure
        let mut inner_map = Mapping::new();
        inner_map.insert(
            Value::String("dimension".to_string()),
            Value::String("globals.semantics.dimensions.customer_id".to_string()),
        );

        let mut outer_map = Mapping::new();
        outer_map.insert(
            Value::String("nested".to_string()),
            Value::Mapping(inner_map),
        );

        let input = Value::Mapping(outer_map);
        let resolved = registry.resolve_all_references(&input).unwrap();

        // Traverse and verify
        if let Value::Mapping(outer) = resolved {
            if let Some(Value::Mapping(inner)) = outer.get(&Value::String("nested".to_string())) {
                if let Some(Value::Mapping(dimension)) =
                    inner.get(&Value::String("dimension".to_string()))
                {
                    assert_eq!(
                        dimension.get(&Value::String("type".to_string())),
                        Some(&Value::String("number".to_string()))
                    );
                } else {
                    panic!("Expected dimension to be resolved");
                }
            } else {
                panic!("Expected nested mapping");
            }
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_resolve_all_references_non_reference_unchanged() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // Test various non-reference values
        assert_eq!(
            registry
                .resolve_all_references(&Value::String("just a string".to_string()))
                .unwrap(),
            Value::String("just a string".to_string())
        );

        assert_eq!(
            registry
                .resolve_all_references(&Value::Number(42.into()))
                .unwrap(),
            Value::Number(42.into())
        );

        assert_eq!(
            registry.resolve_all_references(&Value::Bool(true)).unwrap(),
            Value::Bool(true)
        );

        assert_eq!(
            registry.resolve_all_references(&Value::Null).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn test_resolve_all_references_invalid_reference() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        let input = Value::String("globals.nonexistent.type.name".to_string());
        let result = registry.resolve_all_references(&input);

        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_with_inheritance() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // Create an object with inheritance
        let mut child_map = Mapping::new();
        child_map.insert(
            Value::String("inherits_from".to_string()),
            Value::String("globals.semantics.entities.Customer".to_string()),
        );
        child_map.insert(
            Value::String("description".to_string()),
            Value::String("Extended customer".to_string()),
        );
        let input = Value::Mapping(child_map);

        let resolved = registry.resolve_with_inheritance(&input).unwrap();

        if let Value::Mapping(map) = resolved {
            // Should have inherited name and type from parent
            assert_eq!(
                map.get(&Value::String("name".to_string())),
                Some(&Value::String("Customer".to_string()))
            );
            assert_eq!(
                map.get(&Value::String("type".to_string())),
                Some(&Value::String("primary".to_string()))
            );
            // Should have overridden description
            assert_eq!(
                map.get(&Value::String("description".to_string())),
                Some(&Value::String("Extended customer".to_string()))
            );
            // inherits_from should be removed
            assert!(!map.contains_key(&Value::String("inherits_from".to_string())));
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_registry_clone() {
        let temp_dir = TempDir::new().unwrap();
        create_test_globals(&temp_dir);
        let registry = GlobalRegistry::new(temp_dir.path().join("globals"));

        // Load some data
        registry.load_file("semantics").unwrap();
        assert_eq!(registry.cache_size(), 1);

        // Clone the registry
        let cloned = registry.clone();
        assert_eq!(cloned.cache_size(), 1);

        // Both should be able to access the same data
        let object1 = registry
            .get_object_by_path("semantics", "entities.Customer")
            .unwrap();
        let object2 = cloned
            .get_object_by_path("semantics", "entities.Customer")
            .unwrap();

        assert_eq!(object1, object2);
    }
}
