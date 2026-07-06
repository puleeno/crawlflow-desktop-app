use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Field Schema (mirrors Python output) ─────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCondition {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldValidation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    pub order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<FieldValidation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<FieldCondition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<SelectOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<FieldDef>>,
    #[serde(rename = "item_field", skip_serializing_if = "Option::is_none")]
    pub item_field: Option<Box<FieldDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: HashMap<String, FieldDef>,
}

// ── Validation ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl SettingsSchema {
    pub fn validate(&self, values: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for (key, field_def) in &self.properties {
            let val = values.get(key);

            // Required check
            let is_required = field_def.required.unwrap_or(false)
                || field_def.validation.as_ref().and_then(|v| v.required).unwrap_or(false);

            if is_required {
                match val {
                    None | Some(serde_json::Value::Null) => {
                        errors.push(ValidationError {
                            field: key.clone(),
                            message: format!("{} is required", field_def.title),
                        });
                        continue;
                    }
                    Some(serde_json::Value::String(s)) if s.is_empty() => {
                        errors.push(ValidationError {
                            field: key.clone(),
                            message: format!("{} is required", field_def.title),
                        });
                        continue;
                    }
                    _ => {}
                }
            }

            let val = match val {
                Some(v) if !v.is_null() => v,
                _ => continue,
            };

            let v = match &field_def.validation {
                Some(validation) => validation,
                None => continue,
            };

            // String validations
            if let Some(s) = val.as_str() {
                if let Some(min) = v.min_length {
                    if s.len() < min {
                        errors.push(ValidationError {
                            field: key.clone(),
                            message: format!("{} must be at least {} characters", field_def.title, min),
                        });
                    }
                }
                if let Some(max) = v.max_length {
                    if s.len() > max {
                        errors.push(ValidationError {
                            field: key.clone(),
                            message: format!("{} must be at most {} characters", field_def.title, max),
                        });
                    }
                }
                if let Some(pattern) = &v.pattern {
                    if let Ok(re) = regex::Regex::new(pattern) {
                        if !re.is_match(s) {
                            errors.push(ValidationError {
                                field: key.clone(),
                                message: format!("{} format is invalid", field_def.title),
                            });
                        }
                    }
                }
            }

            // Number validations
            if let Some(n) = val.as_f64() {
                if let Some(min) = v.minimum {
                    if n < min {
                        errors.push(ValidationError {
                            field: key.clone(),
                            message: format!("{} must be >= {}", field_def.title, min),
                        });
                    }
                }
                if let Some(max) = v.maximum {
                    if n > max {
                        errors.push(ValidationError {
                            field: key.clone(),
                            message: format!("{} must be <= {}", field_def.title, max),
                        });
                    }
                }
            }

            // Enum validations
            if let Some(enum_vals) = &v.enum_values {
                let val_str = val.to_string();
                if !enum_vals.iter().any(|e| e == &val_str) {
                    errors.push(ValidationError {
                        field: key.clone(),
                        message: format!("{} must be one of: {}", field_def.title, enum_vals.join(", ")),
                    });
                }
            }
        }

        errors
    }

    pub fn apply_defaults(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (key, field_def) in &self.properties {
            if let Some(default) = &field_def.default {
                map.insert(key.clone(), default.clone());
            }
        }
        serde_json::Value::Object(map)
    }

    pub fn merge_with_defaults(&self, values: &serde_json::Value) -> serde_json::Value {
        let mut defaults = match self.apply_defaults() {
            serde_json::Value::Object(m) => m,
            _ => serde_json::Map::new(),
        };
        if let Some(overrides) = values.as_object() {
            for (k, v) in overrides {
                defaults.insert(k.clone(), v.clone());
            }
        }
        serde_json::Value::Object(defaults)
    }
}

// ── Processor Settings Registry ─────────────────────────

use std::sync::RwLock;

lazy_static::lazy_static! {
    static ref PROCESSOR_SCHEMAS: RwLock<HashMap<String, SettingsSchema>> =
        RwLock::new(HashMap::new());
}

pub fn register_processor_schema(processor_id: &str, schema: SettingsSchema) {
    if let Ok(mut schemas) = PROCESSOR_SCHEMAS.write() {
        schemas.insert(processor_id.to_string(), schema);
    }
}

pub fn get_processor_schema(processor_id: &str) -> Option<SettingsSchema> {
    if let Ok(schemas) = PROCESSOR_SCHEMAS.read() {
        schemas.get(processor_id).cloned()
    } else {
        None
    }
}

pub fn list_processor_schemas() -> HashMap<String, SettingsSchema> {
    if let Ok(schemas) = PROCESSOR_SCHEMAS.read() {
        schemas.clone()
    } else {
        HashMap::new()
    }
}
