use schemars::schema::{InstanceType, SchemaObject, SingleOrVec};
use serde_json::Value;

/// Result type for type conversion operations.
/// Returns (expected_type, actual_type, details) on error.
pub type ConversionResult = Result<Value, (String, String, String)>;

/// Convert a value to an integer according to JSON Schema semantics
pub fn to_integer(value: &Value) -> ConversionResult {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();

            // Try parsing as integer first
            if let Ok(i) = trimmed.parse::<i64>() {
                return Ok(Value::Number(serde_json::Number::from(i)));
            }

            // Try parsing as float and converting to int
            if let Ok(f) = trimmed.parse::<f64>()
                && f.fract() == 0.0
                && f >= i64::MIN as f64
                && f <= i64::MAX as f64
            {
                return Ok(Value::Number(serde_json::Number::from(f as i64)));
            }

            Err((
                "integer".to_string(),
                "string".to_string(),
                format!("Cannot convert '{s}' to integer"),
            ))
        }
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                Ok(value.clone())
            } else if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Ok(Value::Number(serde_json::Number::from(f as i64)))
                } else {
                    Err((
                        "integer".to_string(),
                        "number".to_string(),
                        format!("Cannot convert float {f} to integer without precision loss"),
                    ))
                }
            } else {
                Err((
                    "integer".to_string(),
                    "number".to_string(),
                    format!("Invalid number format: {n:?}"),
                ))
            }
        }
        _ => Err((
            "integer".to_string(),
            value_type_name(value),
            format!("Expected integer value, got {value:?}"),
        )),
    }
}

/// Convert a value to a number (float) according to JSON Schema semantics
pub fn to_number(value: &Value) -> ConversionResult {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            trimmed
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| {
                    (
                        "number".to_string(),
                        "string".to_string(),
                        format!("Cannot convert '{s}' to number"),
                    )
                })
        }
        Value::Number(_) => Ok(value.clone()),
        _ => Err((
            "number".to_string(),
            value_type_name(value),
            format!("Expected number value, got {value:?}"),
        )),
    }
}

/// Convert a value to a boolean according to JSON Schema semantics
pub fn to_boolean(value: &Value) -> ConversionResult {
    match value {
        Value::String(s) => match s.to_lowercase().trim() {
            "true" | "yes" | "1" | "on" | "y" => Ok(Value::Bool(true)),
            "false" | "no" | "0" | "off" | "n" => Ok(Value::Bool(false)),
            _ => Err((
                "boolean".to_string(),
                "string".to_string(),
                format!("Cannot convert '{s}' to boolean"),
            )),
        },
        Value::Bool(_) => Ok(value.clone()),
        Value::Number(n) => {
            let is_truthy = n
                .as_i64()
                .map(|i| i != 0)
                .or_else(|| n.as_f64().map(|f| f != 0.0))
                .ok_or_else(|| {
                    (
                        "boolean".to_string(),
                        "number".to_string(),
                        format!("Cannot convert number {n:?} to boolean"),
                    )
                })?;
            Ok(Value::Bool(is_truthy))
        }
        _ => Err((
            "boolean".to_string(),
            value_type_name(value),
            format!("Expected boolean value, got {value:?}"),
        )),
    }
}

/// Convert a value to a string according to JSON Schema semantics
pub fn to_string(value: &Value) -> ConversionResult {
    let string_value = match value {
        Value::String(_) => value.clone(),
        Value::Number(n) => Value::String(n.to_string()),
        Value::Bool(b) => Value::String(b.to_string()),
        Value::Null => Value::String("null".to_string()),
        Value::Array(_) | Value::Object(_) => {
            Value::String(serde_json::to_string(value).map_err(|e| {
                (
                    "string".to_string(),
                    value_type_name(value),
                    format!("Serialization error: {e}"),
                )
            })?)
        }
    };
    Ok(string_value)
}

/// Convert a value to a specific instance type
pub fn convert_to_single_type(value: &Value, target_type: &InstanceType) -> ConversionResult {
    match target_type {
        InstanceType::Integer => to_integer(value),
        InstanceType::Number => to_number(value),
        InstanceType::Boolean => to_boolean(value),
        InstanceType::String => to_string(value),
        InstanceType::Array => match value {
            Value::Array(_) => Ok(value.clone()),
            _ => Err((
                "array".to_string(),
                value_type_name(value),
                format!("Expected array value, got {value:?}"),
            )),
        },
        InstanceType::Object => match value {
            Value::Object(_) => Ok(value.clone()),
            _ => Err((
                "object".to_string(),
                value_type_name(value),
                format!("Expected object value, got {value:?}"),
            )),
        },
        InstanceType::Null => match value {
            Value::Null => Ok(value.clone()),
            _ => Err((
                "null".to_string(),
                value_type_name(value),
                format!("Expected null value, got {value:?}"),
            )),
        },
    }
}

/// Check if a value already matches a type without conversion
pub fn value_matches_type(value: &Value, instance_type: &InstanceType) -> bool {
    match (value, instance_type) {
        (Value::String(_), InstanceType::String) => true,
        (Value::Number(n), InstanceType::Integer) => n.is_i64() || n.is_u64(),
        (Value::Number(_), InstanceType::Number) => true,
        (Value::Bool(_), InstanceType::Boolean) => true,
        (Value::Array(_), InstanceType::Array) => true,
        (Value::Object(_), InstanceType::Object) => true,
        (Value::Null, InstanceType::Null) => true,
        _ => false,
    }
}

/// Validate array and its items according to schema
pub fn validate_array(value: &Value, schema: &SchemaObject) -> ConversionResult {
    let Value::Array(arr) = value else {
        return Err((
            "array".to_string(),
            value_type_name(value),
            format!("Expected array value, got {value:?}"),
        ));
    };

    // Get array validation if it exists
    let Some(array_validation) = &schema.array else {
        return Ok(value.clone());
    };

    // Get the item schema if specified
    if let Some(items_schema) = &array_validation.items {
        // Handle single schema for all items
        let item_schema = match items_schema {
            SingleOrVec::<schemars::schema::Schema>::Single(schema) => schema.as_ref(),
            SingleOrVec::<schemars::schema::Schema>::Vec(schemas) => {
                // For simplicity, use the first schema if multiple are provided
                // In a more complete implementation, you might want to validate against all schemas
                if schemas.is_empty() {
                    return Ok(value.clone());
                }
                &schemas[0]
            }
        };

        // Extract SchemaObject from Schema
        let item_schema_obj = if let schemars::schema::Schema::Object(obj) = item_schema {
            obj
        } else {
            return Ok(value.clone());
        };

        // Validate each item in the array
        let mut validated_items = Vec::new();
        for (idx, item) in arr.iter().enumerate() {
            let validated_item = if let Some(item_type) = &item_schema_obj.instance_type {
                match item_type {
                    SingleOrVec::<InstanceType>::Single(instance_type) => convert_to_single_type(
                        item,
                        instance_type.as_ref(),
                    )
                    .map_err(|_| {
                        (
                            format!("array of {:?}", instance_type),
                            format!(
                                "array containing {} at index {}",
                                value_type_name(item),
                                idx
                            ),
                            format!(
                                "Array item at index {} has invalid type: expected {:?}, got {:?}",
                                idx, instance_type, item
                            ),
                        )
                    })?,
                    SingleOrVec::<InstanceType>::Vec(types) => {
                        // Try each type until one succeeds
                        let mut converted_value = None;
                        for instance_type in types {
                            if let Ok(converted) = convert_to_single_type(item, instance_type) {
                                converted_value = Some(converted);
                                break;
                            }
                        }
                        if let Some(converted) = converted_value {
                            converted
                        } else {
                            return Err((
                                format!("array of {:?}", types),
                                format!(
                                    "array containing {} at index {}",
                                    value_type_name(item),
                                    idx
                                ),
                                format!(
                                    "Array item at index {} cannot be converted to any of the allowed types: {:?}",
                                    idx, types
                                ),
                            ));
                        }
                    }
                }
            } else {
                item.clone()
            };
            validated_items.push(validated_item);
        }

        Ok(Value::Array(validated_items))
    } else {
        // No item schema specified, accept any array
        Ok(value.clone())
    }
}

/// Get a human-readable type name for a value
pub fn value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

/// Convert a value to match a schema type, with support for union types and array validation
pub fn convert_value_to_schema_type(value: &Value, schema: &SchemaObject) -> ConversionResult {
    if let Some(instance_type) = &schema.instance_type {
        match instance_type {
            SingleOrVec::Single(instance_type) => {
                convert_to_single_type_with_schema(value, instance_type.as_ref(), schema)
            }
            SingleOrVec::Vec(types) => {
                // First, check if the value already matches one of the types without conversion
                for instance_type in types {
                    if value_matches_type(value, instance_type) {
                        if let Ok(converted) =
                            convert_to_single_type_with_schema(value, instance_type, schema)
                        {
                            return Ok(converted);
                        }
                    }
                }

                // If no exact match, try conversions
                for instance_type in types {
                    if let Ok(converted) =
                        convert_to_single_type_with_schema(value, instance_type, schema)
                    {
                        return Ok(converted);
                    }
                }

                Err((
                    format!("{:?}", types),
                    value_type_name(value),
                    format!(
                        "Cannot convert {:?} to any of the allowed types: {:?}",
                        value, types
                    ),
                ))
            }
        }
    } else {
        Ok(value.clone())
    }
}

/// Convert a value to a specific type with schema awareness (for array item validation)
fn convert_to_single_type_with_schema(
    value: &Value,
    target_type: &InstanceType,
    schema: &SchemaObject,
) -> ConversionResult {
    match target_type {
        InstanceType::Integer => to_integer(value),
        InstanceType::Number => to_number(value),
        InstanceType::Boolean => to_boolean(value),
        InstanceType::String => to_string(value),
        InstanceType::Array => validate_array(value, schema),
        InstanceType::Object => match value {
            Value::Object(_) => Ok(value.clone()),
            _ => Err((
                "object".to_string(),
                value_type_name(value),
                format!("Expected object value, got {:?}", value),
            )),
        },
        InstanceType::Null => match value {
            Value::Null => Ok(value.clone()),
            _ => Err((
                "null".to_string(),
                value_type_name(value),
                format!("Expected null value, got {:?}", value),
            )),
        },
    }
}
