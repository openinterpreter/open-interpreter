//! Visit schema-valued children while leaving enum values and other instance data untouched.

use super::DEFINITION_TABLE_KEYS;
use super::SCHEMA_CHILD_KEYS;
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DefinitionTraversal {
    Include,
    Skip,
}

pub(super) fn for_each_schema_child(
    map: &serde_json::Map<String, JsonValue>,
    definition_traversal: DefinitionTraversal,
    visitor: &mut impl FnMut(&JsonValue),
) {
    if let Some(properties) = map.get("properties")
        && let Some(properties_map) = properties.as_object()
    {
        for value in properties_map.values() {
            visitor(value);
        }
    }

    for key in SCHEMA_CHILD_KEYS {
        if let Some(value) = map.get(key) {
            visitor(value);
        }
    }

    if let Some(additional_properties) = map.get("additionalProperties")
        && !matches!(additional_properties, JsonValue::Bool(_))
    {
        visitor(additional_properties);
    }

    if definition_traversal == DefinitionTraversal::Include {
        for key in DEFINITION_TABLE_KEYS {
            if let Some(definitions) = map.get(key)
                && let Some(definitions_map) = definitions.as_object()
            {
                for value in definitions_map.values() {
                    visitor(value);
                }
            }
        }
    }
}

pub(super) fn for_each_schema_child_mut(
    map: &mut serde_json::Map<String, JsonValue>,
    definition_traversal: DefinitionTraversal,
    visitor: &mut impl FnMut(&mut JsonValue),
) {
    if let Some(properties) = map.get_mut("properties")
        && let Some(properties_map) = properties.as_object_mut()
    {
        for value in properties_map.values_mut() {
            visitor(value);
        }
    }

    for key in SCHEMA_CHILD_KEYS {
        if let Some(value) = map.get_mut(key) {
            visitor(value);
        }
    }

    if let Some(additional_properties) = map.get_mut("additionalProperties")
        && !matches!(additional_properties, JsonValue::Bool(_))
    {
        visitor(additional_properties);
    }

    if definition_traversal == DefinitionTraversal::Include {
        for key in DEFINITION_TABLE_KEYS {
            if let Some(definitions) = map.get_mut(key)
                && let Some(definitions_map) = definitions.as_object_mut()
            {
                for value in definitions_map.values_mut() {
                    visitor(value);
                }
            }
        }
    }
}
