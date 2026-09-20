//! Normalization of generated JSON Schemas into the MCP-portable subset.
//!
//! `schemars` emits full JSON Schema draft 2020-12: a `$schema` declaration, a
//! `$defs` section with `$ref` pointers, `anyOf` for optional composite types, and
//! Rust-specific annotations such as `format: "uint"`, `minimum` and `maximum`.
//! That is valid JSON Schema, and the reference `@modelcontextprotocol/sdk` client
//! (ajv in non-strict mode) accepts it — it merely logs `unknown format "uint"`.
//!
//! Stricter hosts do not. DeepSeek Harness validates every registered tool schema
//! against an enforced subset of `type` / `oneOf` / `properties` / `required` /
//! `additionalProperties` / `items` / `enum` / `const` plus annotations, and rejects
//! the whole tool when it sees `$defs`, `$ref`, `$schema`, a `type` array, `anyOf`,
//! or `format`. A rejected schema means the tool is silently unavailable to the
//! model, so a Transcend server that advertises raw `schemars` output registers
//! **zero** tools there.
//!
//! Rewriting the schema once, at the boundary, keeps the Rust request types
//! ergonomic (`Option<T>`, `usize`, nested structs) while making the advertised
//! contract portable:
//!
//! - `$ref` is replaced by the referenced `$defs` subschema (recursively); the
//!   `$defs`/`$schema` keywords are then dropped since nothing points at them.
//! - `type: ["integer", "null"]` collapses to the single non-null type. Optionality
//!   is already carried by `required`, which is how JSON Schema expresses it.
//! - `anyOf` that only expresses nullability is collapsed to its non-null branch.
//! - `format`, `minimum`, `maximum`, `examples` and other annotation-only keywords
//!   are dropped: they constrain nothing the protocol enforces, and the model
//!   already receives the constraints that matter through `enum`/`const` and the
//!   `description` text.
//!
//! The transformation is lossless for the properties a client actually enforces.

use serde_json::{Map, Value};

/// Normalize a generated tool input schema for portable MCP consumption.
pub fn normalize_schema(schema: &Value) -> Value {
    let definitions = schema
        .get("$defs")
        .or_else(|| schema.get("definitions"))
        .cloned()
        .unwrap_or(Value::Null);

    let mut out = rewrite_schema(schema, &definitions, 0);
    if let Some(obj) = out.as_object_mut() {
        // Definitions are inlined by now; keeping them would re-introduce the
        // unsupported `$defs` keyword.
        obj.remove("$defs");
        obj.remove("definitions");
    }
    out
}

/// Keywords whose *map keys* are user-chosen names rather than schema keywords.
///
/// `rewrite_schema` must not treat an entry of these maps as a keyword, or a request
/// field that happens to share a name with an unrelated JSON Schema keyword (for
/// example the `pattern` field of `SearchRequest`) is silently deleted from the
/// advertised contract.
const NAME_MAPS: &[&str] = &["properties", "patternProperties", "$defs", "definitions"];

/// Keys that carry no validation meaning in the enforced subset.
const DROPPED_KEYWORDS: &[&str] = &[
    "$schema",
    "format",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "pattern",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
    "examples",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
];

/// Maximum `$ref` expansion depth. Guards against a self-referential definition
/// producing unbounded output; a recursive request type is a modelling error here
/// because MCP tool arguments are finite JSON.
const MAX_REF_DEPTH: usize = 12;

/// Rewrite a schema *keyword* map (the node itself carries schema keywords).
fn rewrite_schema(node: &Value, definitions: &Value, depth: usize) -> Value {
    if depth > MAX_REF_DEPTH {
        // Bail out to an unconstrained schema rather than recursing forever.
        return Value::Object(Map::new());
    }

    match node {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| rewrite_schema(item, definitions, depth))
                .collect(),
        ),
        Value::Object(obj) => {
            // 1. Resolve a $ref by inlining the target definition.
            if let Some(Value::String(reference)) = obj.get("$ref") {
                if let Some(name) = reference.strip_prefix("#/$defs/")
                    && let Some(target) = definitions.get(name)
                {
                    let mut resolved = rewrite_schema(target, definitions, depth + 1);
                    // Sibling keywords alongside $ref win over the target.
                    if let Some(resolved_obj) = resolved.as_object_mut() {
                        for (key, value) in obj {
                            if key != "$ref" {
                                resolved_obj
                                    .insert(key.clone(), rewrite_schema(value, definitions, depth));
                            }
                        }
                    }
                    return resolved;
                }
                // Unresolvable reference: fall back to an unconstrained schema
                // rather than emitting a keyword the host will reject.
                return Value::Object(Map::new());
            }

            let mut out = Map::new();
            for (key, value) in obj {
                if NAME_MAPS.contains(&key.as_str()) {
                    // Recurse into the *values*; the keys are names, not keywords.
                    out.insert(key.clone(), rewrite_name_map(value, definitions, depth));
                    continue;
                }
                if DROPPED_KEYWORDS.contains(&key.as_str()) {
                    continue;
                }
                out.insert(key.clone(), rewrite_schema(value, definitions, depth));
            }

            collapse_combinators(&mut out);
            collapse_type_array(&mut out);
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Rewrite a map whose keys are user-chosen property names.
fn rewrite_name_map(node: &Value, definitions: &Value, depth: usize) -> Value {
    match node {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(name, subschema)| {
                    (name.clone(), rewrite_schema(subschema, definitions, depth))
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Collapse nullability expressed as `anyOf`/`oneOf` with a null branch, or as a
/// single-branch `anyOf` (which the enforced subset does not support).
fn collapse_combinators(out: &mut Map<String, Value>) {
    for combinator in ["anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = out.get(combinator).cloned() else {
            continue;
        };

        let non_null: Vec<Value> = branches
            .iter()
            .filter(|branch| !is_null_schema(branch))
            .cloned()
            .collect();

        if non_null.is_empty() {
            // `anyOf: [{type: null}]` describes only null.
            out.remove(combinator);
            out.insert("type".to_string(), Value::String("null".to_string()));
            continue;
        }
        if non_null.len() == branches.len() {
            // No null branch. `oneOf` is supported as-is; `anyOf` is not, so only
            // collapse it when it is really a single shape.
            if combinator == "anyOf" && non_null.len() == 1 {
                out.remove(combinator);
                merge_into(out, &non_null[0]);
            }
            continue;
        }

        // Nullable: drop the null branch.
        if non_null.len() == 1 {
            out.remove(combinator);
            merge_into(out, &non_null[0]);
        } else {
            out.insert(combinator.to_string(), Value::Array(non_null));
        }
    }
}

/// Collapse a `type` array to a single non-null type.
fn collapse_type_array(out: &mut Map<String, Value>) {
    let Some(Value::Array(types)) = out.get("type").cloned() else {
        return;
    };
    let concrete: Vec<&Value> = types
        .iter()
        .filter(|t| t.as_str() != Some("null"))
        .collect();

    match concrete.len() {
        0 => {
            out.insert("type".to_string(), Value::String("null".to_string()));
        }
        1 => {
            out.insert("type".to_string(), concrete[0].clone());
        }
        _ => {
            // Genuine union (rare): express it as oneOf so the shape survives
            // without an unsupported type array.
            out.remove("type");
            let alternatives: Vec<Value> = concrete
                .into_iter()
                .map(|t| {
                    let mut branch = Map::new();
                    branch.insert("type".to_string(), t.clone());
                    Value::Object(branch)
                })
                .collect();
            out.insert("oneOf".to_string(), Value::Array(alternatives));
        }
    }
}

/// Whether a schema node accepts only JSON `null`.
fn is_null_schema(node: &Value) -> bool {
    node.get("type").and_then(Value::as_str) == Some("null")
        || matches!(node.get("const"), Some(Value::Null))
}

/// Merge a resolved branch's keywords into the parent, parent keywords winning.
fn merge_into(target: &mut Map<String, Value>, branch: &Value) {
    let Some(branch_obj) = branch.as_object() else {
        return;
    };
    for (key, value) in branch_obj {
        target.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_drops_schema_and_defs() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Req",
            "type": "object",
            "$defs": { "Inner": { "type": "string" } },
            "properties": { "a": { "type": "string" } },
            "required": ["a"]
        });
        let out = normalize_schema(&schema);
        assert!(out.get("$schema").is_none());
        assert!(out.get("$defs").is_none());
        assert_eq!(out["type"], json!("object"));
        assert_eq!(out["properties"]["a"]["type"], json!("string"));
    }

    #[test]
    fn test_inlines_ref_and_keeps_description() {
        let schema = json!({
            "type": "object",
            "$defs": {
                "Mode": {
                    "type": "string",
                    "description": "Splicing mode",
                    "enum": ["replace", "insert_before"]
                }
            },
            "properties": {
                "mode": {
                    "$ref": "#/$defs/Mode",
                    "description": "Overrides the definition text"
                }
            }
        });
        let out = normalize_schema(&schema);
        let mode = &out["properties"]["mode"];
        assert_eq!(mode["type"], json!("string"));
        assert_eq!(mode["enum"], json!(["replace", "insert_before"]));
        // Sibling keyword wins over the referenced definition.
        assert_eq!(mode["description"], json!("Overrides the definition text"));
        assert!(mode.get("$ref").is_none());
    }

    #[test]
    fn test_collapses_nullable_type_array() {
        let schema = json!({
            "type": "object",
            "properties": {
                "start_line": { "type": ["integer", "null"], "format": "uint", "minimum": 0 }
            }
        });
        let out = normalize_schema(&schema);
        let prop = &out["properties"]["start_line"];
        assert_eq!(prop["type"], json!("integer"));
        assert!(prop.get("format").is_none());
        assert!(prop.get("minimum").is_none());
    }

    #[test]
    fn test_collapses_nullable_anyof_to_object() {
        let schema = json!({
            "type": "object",
            "properties": {
                "options": {
                    "anyOf": [
                        { "$ref": "#/$defs/SearchOptions" },
                        { "type": "null" }
                    ]
                }
            },
            "$defs": {
                "SearchOptions": {
                    "type": "object",
                    "properties": { "case_sensitive": { "type": ["boolean", "null"] } }
                }
            }
        });
        let out = normalize_schema(&schema);
        let options = &out["properties"]["options"];
        assert_eq!(options["type"], json!("object"));
        assert_eq!(
            options["properties"]["case_sensitive"]["type"],
            json!("boolean")
        );
        assert!(options.get("anyOf").is_none());
    }

    #[test]
    fn test_recurses_into_arrays_and_nested_objects() {
        let schema = json!({
            "type": "object",
            "properties": {
                "patches": {
                    "type": "array",
                    "items": { "$ref": "#/$defs/PatchRequest" }
                }
            },
            "$defs": {
                "PatchRequest": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "dry_run": { "type": ["boolean", "null"], "format": "uint" }
                    },
                    "required": ["path", "replacement"]
                }
            }
        });
        let out = normalize_schema(&schema);
        let item = &out["properties"]["patches"]["items"];
        assert_eq!(item["type"], json!("object"));
        assert_eq!(item["properties"]["path"]["type"], json!("string"));
        assert_eq!(item["properties"]["dry_run"]["type"], json!("boolean"));
        assert_eq!(item["required"], json!(["path", "replacement"]));
    }

    #[test]
    fn test_unresolvable_ref_becomes_unconstrained() {
        let schema =
            json!({ "type": "object", "properties": { "x": { "$ref": "#/$defs/Missing" } } });
        let out = normalize_schema(&schema);
        let x = &out["properties"]["x"];
        assert!(x.get("$ref").is_none());
        assert_eq!(x.as_object().map(Map::len), Some(0));
    }

    #[test]
    fn test_genuine_type_union_becomes_one_of() {
        let schema = json!({ "type": ["string", "integer"] });
        let out = normalize_schema(&schema);
        assert!(out.get("type").is_none());
        let one_of = out["oneOf"].as_array().expect("oneOf array");
        assert_eq!(one_of.len(), 2);
        assert_eq!(one_of[0]["type"], json!("string"));
        assert_eq!(one_of[1]["type"], json!("integer"));
    }

    /// A request field whose name collides with an unrelated JSON Schema keyword
    /// must survive. `SearchRequest.pattern` is a *property* named `pattern`; an
    /// earlier implementation filtered it as the `pattern` schema keyword and
    /// silently deleted it from the advertised contract, leaving the tool unusable.
    #[test]
    fn test_property_names_colliding_with_keywords_are_kept() {
        let schema = json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "search pattern" },
                "format": { "type": "string" },
                "default": { "type": "string" },
                "examples": { "type": "array", "items": { "type": "string" } },
                "minimum": { "type": "integer" }
            },
            "required": ["pattern"]
        });
        let out = normalize_schema(&schema);
        let props = out["properties"].as_object().expect("properties object");

        for name in ["pattern", "format", "default", "examples", "minimum"] {
            assert!(
                props.contains_key(name),
                "property '{name}' was dropped as if it were a schema keyword"
            );
        }
        assert_eq!(props["pattern"]["type"], json!("string"));
        assert_eq!(props["pattern"]["description"], json!("search pattern"));
        assert_eq!(out["required"], json!(["pattern"]));

        // Every name in `required` must exist in `properties`, or strict hosts
        // reject the whole schema.
        for required in out["required"].as_array().expect("required array") {
            let name = required.as_str().expect("string");
            assert!(
                props.contains_key(name),
                "'{name}' is required but not declared in properties"
            );
        }
    }

    /// The schema keyword `pattern` must still be dropped where it really is a
    /// keyword, since the enforced subset does not support it.
    #[test]
    fn test_pattern_keyword_still_dropped_on_a_schema_node() {
        let schema = json!({
            "type": "object",
            "properties": { "version": { "type": "string", "pattern": "^\\d+\\.\\d+$" } }
        });
        let out = normalize_schema(&schema);
        let version = &out["properties"]["version"];
        assert_eq!(version["type"], json!("string"));
        assert!(version.get("pattern").is_none(), "keyword must be dropped");
    }

    #[test]
    fn test_recursive_definition_terminates() {
        let schema = json!({
            "$defs": { "Node": { "type": "object", "properties": { "next": { "$ref": "#/$defs/Node" } } } },
            "type": "object",
            "properties": { "root": { "$ref": "#/$defs/Node" } }
        });
        // Must return rather than overflow the stack.
        let out = normalize_schema(&schema);
        assert_eq!(out["type"], json!("object"));
    }
}
