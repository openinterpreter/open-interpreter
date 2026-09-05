use pretty_assertions::assert_eq;

#[test]
fn collapse_deep_schema_objects_traverses_schema_children() {
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": {
            "object_parent": {
                "type": "object",
                "properties": {
                    "complex": {
                        "type": "object",
                        "properties": {
                            "nested": {
                                "type": "object",
                                "properties": {
                                    "leaf": { "type": "string" }
                                }
                            }
                        }
                    },
                    "scalar": {
                        "type": "string"
                    }
                }
            },
            "array_parent": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "nested": {
                            "type": "object",
                            "properties": {
                                "leaf": { "type": "string" }
                            }
                        }
                    }
                }
            },
            "map_parent": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "properties": {
                        "nested": {
                            "type": "object",
                            "properties": {
                                "leaf": { "type": "string" }
                            }
                        }
                    }
                }
            },
            "union_parent": {
                "anyOf": [
                    {
                        "type": "object",
                        "properties": {
                            "nested": {
                                "type": "object",
                                "properties": {
                                    "leaf": { "type": "string" }
                                }
                            }
                        }
                    },
                    { "type": "string" }
                ]
            }
        }
    });

    super::collapse_deep_schema_objects(&mut schema, /*depth*/ 0);

    assert_eq!(
        schema,
        serde_json::json!({
            "type": "object",
            "properties": {
                "object_parent": {
                    "type": "object",
                    "properties": {
                        "complex": {
                            "type": "object",
                            "properties": {
                                "nested": {}
                            }
                        },
                        "scalar": {
                            "type": "string"
                        }
                    }
                },
                "array_parent": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "nested": {}
                        }
                    }
                },
                "map_parent": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "properties": {
                            "nested": {}
                        }
                    }
                },
                "union_parent": {
                    "anyOf": [
                        {
                            "type": "object",
                            "properties": {
                                "nested": {}
                            }
                        },
                        { "type": "string" }
                    ]
                }
            }
        })
    );
}
