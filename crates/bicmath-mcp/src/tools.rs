//! MCP tool schema generation from the registry.
//!
//! Tool schemas are generated from the same [`FunctionDescriptor`] used for
//! validation, so handwritten MCP descriptions cannot drift from runtime
//! behaviour.

use std::collections::BTreeMap;
use std::sync::Arc;

use bicmath_core::contract::FunctionDescriptor;
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::json;

/// Which tools the server exposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ToolProfile {
    /// Six generic dispatcher tools plus function discovery.
    #[default]
    Compact,
    /// Every enabled function as an individually typed tool, generated from
    /// the same registry. Chosen at startup only.
    Expanded,
}

/// MCP server configuration.
#[derive(Clone, Debug)]
pub struct McpConfig {
    pub profile: ToolProfile,
    pub max_in_flight: usize,
    pub server_name: String,
    pub server_version: String,
    pub instructions: Option<String>,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        McpConfig {
            profile: ToolProfile::Compact,
            max_in_flight: 4,
            server_name: "bicmath".to_string(),
            server_version: bicmath_core::VERSION.to_string(),
            instructions: Some(
                "BicMath is a deterministic computation engine for mathematics, statistics, \
                 science, and business finance. Use list_modules and list_functions to \
                 discover functions, describe_function for schemas and conventions, then \
                 calculate, evaluate, or batch. Exact decimal, rational, and integer inputs \
                 stay exact; float64 results are approximate. The engine executes defined \
                 methods and returns inspectable results; it does not certify that the right \
                 business question or statistical design was chosen."
                    .to_string(),
            ),
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
        }
    }
}

fn object_schema(value: serde_json::Value) -> Arc<JsonObject> {
    match value {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::new(JsonObject::new()),
    }
}

fn annotations() -> ToolAnnotations {
    ToolAnnotations::new()
        .read_only(true)
        .destructive(false)
        .idempotent(true)
        .open_world(false)
}

/// The stable compact tool surface.
pub fn compact_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "list_modules",
            "List enabled computation modules with their capabilities, supported numeric \
             modes, and function counts.",
            object_schema(json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            })),
        )
        .with_annotations(annotations()),
        Tool::new(
            "list_functions",
            "List registered functions with filtering by module and free-text query, and \
             pagination. Use describe_function for full schemas.",
            object_schema(json!({
                "type": "object",
                "properties": {
                    "module": {"type": "string", "description": "Restrict to one module id."},
                    "query": {"type": "string", "description": "Case-insensitive text match on id, title, and summary."},
                    "offset": {"type": "integer", "minimum": 0, "description": "Pagination offset."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200, "description": "Page size (default 50, maximum 200)."}
                },
                "additionalProperties": false
            })),
        )
        .with_annotations(annotations()),
        Tool::new(
            "describe_function",
            "Return the full descriptor for one function: typed parameters, output schema, \
             supported numeric modes, domain constraints, method reference, and executable \
             examples.",
            object_schema(json!({
                "type": "object",
                "properties": {
                    "function": {"type": "string", "description": "Qualified function id, e.g. statistics.median."}
                },
                "required": ["function"],
                "additionalProperties": false
            })),
        )
        .with_annotations(annotations()),
        Tool::new(
            "calculate",
            "Execute one qualified function with typed arguments. Arguments are validated \
             against that function's real schema; the outer schema is only a dispatcher.",
            object_schema(json!({
                "type": "object",
                "properties": {
                    "function": {"type": "string", "description": "Qualified function id."},
                    "arguments": {"type": "object", "description": "Function arguments; see describe_function."},
                    "context": {"type": "object", "description": "Optional numeric context and limit reductions."},
                    "receipt": {"type": "boolean", "description": "Include a replay receipt."}
                },
                "required": ["function"],
                "additionalProperties": false
            })),
        )
        .with_annotations(annotations()),
        Tool::new(
            "evaluate",
            "Evaluate a complete restricted expression with named bindings. Supports decimal, \
             integer, and rational literals, arithmetic, comparisons, arrays, records, and \
             qualified function calls with named arguments.",
            object_schema(json!({
                "type": "object",
                "properties": {
                    "expression": {"type": "string", "description": "Restricted expression, e.g. finance.npv(rate=0.08, cashflows=[-1000, 500, 500])."},
                    "bindings": {"type": "object", "description": "Named typed values referenced by identifier."},
                    "context": {"type": "object", "description": "Optional numeric context and limit reductions."},
                    "receipt": {"type": "boolean", "description": "Include a replay receipt."}
                },
                "required": ["expression"],
                "additionalProperties": false
            })),
        )
        .with_annotations(annotations()),
        Tool::new(
            "batch",
            "Execute a bounded dependency graph of calculations. Nodes have unique names; \
             earlier results are referenced with {\"$ref\": \"node_name\"} in call arguments \
             or by identifier in expressions. Cycles, duplicate names, and unknown references \
             are rejected before execution.",
            object_schema(json!({
                "type": "object",
                "properties": {
                    "nodes": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "properties": {
                                        "type": {"const": "call"},
                                        "name": {"type": "string"},
                                        "function": {"type": "string"},
                                        "arguments": {"type": "object"}
                                    },
                                    "required": ["type", "name", "function"]
                                },
                                {
                                    "type": "object",
                                    "properties": {
                                        "type": {"const": "evaluate"},
                                        "name": {"type": "string"},
                                        "expression": {"type": "string"}
                                    },
                                    "required": ["type", "name", "expression"]
                                }
                            ]
                        }
                    },
                    "outputs": {"type": "array", "items": {"type": "string"}},
                    "fail_fast": {"type": "boolean"},
                    "context": {"type": "object"},
                    "receipt": {"type": "boolean"}
                },
                "required": ["nodes"],
                "additionalProperties": false
            })),
        )
        .with_annotations(annotations()),
    ]
}

/// Map a qualified function id to an MCP tool name for the expanded profile.
pub fn function_tool_name(function_id: &str) -> String {
    function_id.replace('.', "__")
}

/// Build the expanded profile: one typed tool per enabled function.
pub fn expanded_tools<'a>(
    descriptors: impl Iterator<Item = &'a FunctionDescriptor>,
) -> (Vec<Tool>, BTreeMap<String, String>) {
    let mut tools = Vec::new();
    let mut mapping = BTreeMap::new();
    for descriptor in descriptors {
        let name = function_tool_name(&descriptor.id);
        let mut properties = JsonObject::new();
        let mut required = Vec::new();
        for param in &descriptor.parameters {
            let mut schema = param.schema.to_json_schema();
            if !param.description.is_empty() {
                schema["description"] = json!(param.description);
            }
            if let serde_json::Value::Object(fields) = &mut schema {
                properties.insert(
                    param.name.clone(),
                    serde_json::Value::Object(fields.clone()),
                );
            }
            if param.required {
                required.push(serde_json::Value::String(param.name.clone()));
            }
        }
        let input = json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        });
        let output = descriptor.output.to_json_schema();
        let mut description = format!("{} {}", descriptor.summary, descriptor.description);
        if !descriptor.method_ref.is_empty() {
            description.push_str(&format!(" Method: {}.", descriptor.method_ref));
        }
        let mut tool = Tool::new(
            name.clone(),
            description.trim().to_string(),
            object_schema(input),
        )
        .with_title(descriptor.title.clone())
        .with_annotations(annotations());
        if let serde_json::Value::Object(output) = output {
            tool = tool.with_raw_output_schema(Arc::new(output));
        }
        tools.push(tool);
        mapping.insert(name, descriptor.id.clone());
    }
    (tools, mapping)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_stable_and_unique() {
        assert_eq!(
            function_tool_name("statistics.median"),
            "statistics__median"
        );
        assert_eq!(
            function_tool_name("linear_algebra.solve"),
            "linear_algebra__solve"
        );
        let names: Vec<String> = compact_tools()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        let unique: std::collections::BTreeSet<&String> = names.iter().collect();
        assert_eq!(names.len(), unique.len());
    }
}
