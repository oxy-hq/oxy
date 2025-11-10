use crate::errors::{GlobalError, GlobalResult};
use crate::reference::GlobalReference;
use crate::registry::GlobalRegistry;
use serde_yaml::{Mapping, Value};
use std::collections::HashSet;

pub struct ObjectInheritanceEngine {
    registry: GlobalRegistry,
}

impl ObjectInheritanceEngine {
    /// Create a new ObjectInheritanceEngine with the given registry
    ///
    /// # Arguments
    ///
    /// * `registry` - The GlobalRegistry to use for resolving global object references
    pub fn new(registry: GlobalRegistry) -> Self {
        Self { registry }
    }

    pub fn resolve_inheritance(&self, object: &mut Value) -> GlobalResult<Value> {
        let mut visited = HashSet::new();
        self.resolve_inheritance_recursive(object, &mut visited)
    }

    /// Internal recursive method for resolving inheritance with cycle detection
    fn resolve_inheritance_recursive(
        &self,
        object: &mut Value,
        visited: &mut HashSet<String>,
    ) -> GlobalResult<Value> {
        // Only process mappings (objects)
        let mapping = match object {
            Value::Mapping(map) => map,
            _ => return Ok(object.clone()),
        };

        // Check if this object has an inherits_from field
        let inherits_from = match mapping.get(&Value::String("inherits_from".to_string())) {
            Some(Value::String(reference)) => reference.clone(),
            Some(_) => {
                return Err(GlobalError::InvalidYamlStructure {
                    file: "object".to_string(),
                    error: "inherits_from must be a string".to_string(),
                });
            }
            None => return Ok(object.clone()), // No inheritance
        };

        // Check for circular inheritance
        if visited.contains(&inherits_from) {
            return Err(GlobalError::InvalidObjectPath(format!(
                "Circular inheritance detected: {}",
                inherits_from
            )));
        }
        visited.insert(inherits_from.clone());

        // Parse the global reference
        let reference = GlobalReference::parse(&inherits_from).map_err(|_| {
            GlobalError::InvalidObjectPath(format!(
                "Invalid inherits_from reference: {}",
                inherits_from
            ))
        })?;

        // Get the parent object from the registry
        let mut parent_object = self.registry.get_object_by_reference(&reference)?;

        // Recursively resolve parent's inheritance
        let resolved_parent = self.resolve_inheritance_recursive(&mut parent_object, visited)?;

        // Remove the reference from visited set (for other branches)
        visited.remove(&inherits_from);

        // Merge parent with child (child properties override parent)
        let merged = self.merge_objects(&resolved_parent, object)?;

        Ok(merged)
    }

    fn merge_objects(&self, parent: &Value, child: &Value) -> GlobalResult<Value> {
        let parent_map = match parent {
            Value::Mapping(map) => map,
            Value::Null => {
                return Err(GlobalError::InvalidYamlStructure {
                    file: "parent_object".to_string(),
                    error: "Parent object cannot be null".to_string(),
                });
            }
            _ => {
                return Err(GlobalError::InvalidYamlStructure {
                    file: "parent_object".to_string(),
                    error: "Parent object must be a mapping".to_string(),
                });
            }
        };

        let child_map = match child {
            Value::Mapping(map) => map,
            _ => {
                return Err(GlobalError::InvalidYamlStructure {
                    file: "child_object".to_string(),
                    error: "Child object must be a mapping".to_string(),
                });
            }
        };

        let mut merged = Mapping::new();

        // Start with parent properties
        for (key, value) in parent_map {
            merged.insert(key.clone(), value.clone());
        }

        // Override with child properties
        for (key, value) in child_map {
            // Skip the inherits_from field in the final result
            if key == &Value::String("inherits_from".to_string()) {
                continue;
            }

            // Child properties override parent properties
            if parent_map.contains_key(key) {
                merged.insert(key.clone(), value.clone());
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }

        Ok(Value::Mapping(merged))
    }

    /// Validate that an `inherits_from` reference is valid and exists
    ///
    /// This method checks:
    /// 1. The reference syntax is valid
    /// 2. The referenced global object exists
    /// 3. The referenced object is a valid structure for inheritance
    pub fn validate_inheritance_reference(&self, inherits_from: &str) -> GlobalResult<()> {
        // Parse the reference
        let reference = GlobalReference::parse(inherits_from).map_err(|_| {
            GlobalError::InvalidObjectPath(format!(
                "Invalid inherits_from reference syntax: {}",
                inherits_from
            ))
        })?;

        // Check that the object exists
        let object = self.registry.get_object_by_reference(&reference)?;

        // Validate that the object is a mapping (suitable for inheritance)
        match object {
            Value::Mapping(_) => Ok(()),
            _ => Err(GlobalError::InvalidYamlStructure {
                file: reference.file_name,
                error: format!(
                    "Referenced object {} is not a mapping and cannot be inherited from",
                    inherits_from
                ),
            }),
        }
    }

    /// Get detailed information about what properties would be inherited
    ///
    /// This method is useful for debugging and understanding inheritance resolution.
    /// It returns information about:
    /// - What properties come from the parent
    /// - What properties are overridden by the child
    /// - What properties are new in the child
    pub fn get_inheritance_info(
        &self,
        inherits_from: &str,
        child_object: &Value,
    ) -> GlobalResult<InheritanceInfo> {
        let reference = GlobalReference::parse(inherits_from).map_err(|_| {
            GlobalError::InvalidObjectPath(format!(
                "Invalid inherits_from reference: {}",
                inherits_from
            ))
        })?;

        let parent_object = self.registry.get_object_by_reference(&reference)?;

        let parent_map = match &parent_object {
            Value::Mapping(map) => map,
            _ => {
                return Err(GlobalError::InvalidYamlStructure {
                    file: reference.file_name,
                    error: "Parent object must be a mapping".to_string(),
                });
            }
        };

        let child_map = match child_object {
            Value::Mapping(map) => map,
            _ => {
                return Err(GlobalError::InvalidYamlStructure {
                    file: "child_object".to_string(),
                    error: "Child object must be a mapping".to_string(),
                });
            }
        };

        let mut inherited_properties = Vec::new();
        let mut overridden_properties = Vec::new();
        let mut new_properties = Vec::new();

        // Analyze parent properties
        for (key, _value) in parent_map {
            if let Value::String(key_str) = key {
                if child_map.contains_key(key) && key_str != "inherits_from" {
                    overridden_properties.push(key_str.clone());
                } else {
                    inherited_properties.push(key_str.clone());
                }
            }
        }

        // Analyze child properties
        for (key, _value) in child_map {
            if let Value::String(key_str) = key {
                if key_str != "inherits_from" && !parent_map.contains_key(key) {
                    new_properties.push(key_str.clone());
                }
            }
        }

        Ok(InheritanceInfo {
            parent_reference: inherits_from.to_string(),
            inherited_properties,
            overridden_properties,
            new_properties,
        })
    }
}

/// Information about object inheritance resolution
#[derive(Debug, Clone)]
pub struct InheritanceInfo {
    /// The parent reference that is being inherited from
    pub parent_reference: String,
    /// Properties that are inherited from the parent without modification
    pub inherited_properties: Vec<String>,
    /// Properties from the parent that are overridden by the child
    pub overridden_properties: Vec<String>,
    /// New properties added by the child that don't exist in the parent
    pub new_properties: Vec<String>,
}
