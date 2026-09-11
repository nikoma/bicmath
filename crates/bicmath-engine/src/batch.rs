//! Dependent batch execution with explicit references, cycle detection, and
//! request-local state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use bicmath_core::context::ExecContext;
use bicmath_core::envelope::{EngineInfo, ErrorResponse, ResultEnvelope};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{Expr, parse_expression};
use bicmath_core::fingerprint::{Receipt, calculation_fingerprint};
use bicmath_core::value::Value;

use crate::envelope::{FingerprintInput, build_envelope};
use crate::registry::Registry;
use crate::request::{CallOptions, ContextOverrides};

/// A batch request: a bounded dependency graph of calculations.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub nodes: Vec<BatchNode>,
    #[serde(default)]
    pub outputs: Option<Vec<String>>,
    #[serde(default)]
    pub fail_fast: Option<bool>,
    #[serde(default)]
    pub context: Option<ContextOverrides>,
    #[serde(default)]
    pub receipt: bool,
}

/// One batch node. References to earlier nodes use `{"$ref": "node_name"}` in
/// call arguments, or a bare identifier in an expression.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BatchNode {
    Call {
        name: String,
        function: String,
        #[serde(default)]
        arguments: serde_json::Value,
    },
    Evaluate {
        name: String,
        expression: String,
    },
    /// Run a multi-step workflow recipe (see `recipes`).
    Recipe {
        name: String,
        recipe: String,
        #[serde(default)]
        arguments: serde_json::Value,
    },
}

impl BatchNode {
    pub fn name(&self) -> &str {
        match self {
            BatchNode::Call { name, .. } => name,
            BatchNode::Evaluate { name, .. } => name,
            BatchNode::Recipe { name, .. } => name,
        }
    }
}

/// Node execution status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Ok,
    Error,
    Skipped,
}

/// One node's outcome.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeResult {
    pub name: String,
    pub status: NodeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ResultEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<EngineError>,
}

/// The batch result envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchEnvelope {
    pub schema_version: u32,
    pub engine: EngineInfo,
    pub node_count: usize,
    pub outputs: Vec<NodeResult>,
    pub fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<Receipt>,
}

struct PlannedNode {
    name: String,
    deps: Vec<String>,
    kind: PlannedKind,
}

enum PlannedKind {
    Call {
        function: String,
        arguments: serde_json::Value,
    },
    Evaluate {
        expression: Expr,
        source: String,
    },
    Recipe {
        recipe: String,
        arguments: serde_json::Value,
    },
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn collect_refs(raw: &serde_json::Value, out: &mut Vec<String>) {
    match raw {
        serde_json::Value::Object(map) => {
            if map.len() == 1
                && let Some(serde_json::Value::String(name)) = map.get("$ref")
            {
                out.push(name.clone());
                return;
            }
            for value in map.values() {
                collect_refs(value, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_refs(item, out);
            }
        }
        _ => {}
    }
}

fn resolve_refs(
    raw: &serde_json::Value,
    results: &BTreeMap<String, Value>,
) -> Result<serde_json::Value, EngineError> {
    match raw {
        serde_json::Value::Object(map) => {
            if map.len() == 1
                && let Some(serde_json::Value::String(name)) = map.get("$ref")
            {
                let value = results.get(name).ok_or_else(|| {
                    EngineError::new(
                        ErrorCode::NotFound,
                        format!("batch reference {name:?} has no earlier result"),
                    )
                })?;
                return serde_json::to_value(value).map_err(|e| {
                    EngineError::internal(format!("batch reference is not serializable: {e}"))
                });
            }
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                out.insert(key.clone(), resolve_refs(value, results)?);
            }
            Ok(serde_json::Value::Object(out))
        }
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(resolve_refs(item, results)?);
            }
            Ok(serde_json::Value::Array(out))
        }
        other => Ok(other.clone()),
    }
}

fn plan(
    registry: &Registry,
    request: &BatchRequest,
    ctx: &ExecContext,
) -> Result<Vec<PlannedNode>, EngineError> {
    if request.nodes.is_empty() {
        return Err(EngineError::malformed(
            "batch must contain at least one node",
        ));
    }
    if request.nodes.len() > ctx.limits.max_batch_nodes {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "batch has {} nodes, exceeding the limit of {}",
                request.nodes.len(),
                ctx.limits.max_batch_nodes
            ),
        ));
    }
    let mut names = BTreeSet::new();
    let mut planned = Vec::with_capacity(request.nodes.len());
    for node in request.nodes.iter() {
        let name = node.name();
        if !is_identifier(name) {
            return Err(EngineError::malformed(format!(
                "batch node name {name:?} is not a valid identifier"
            )));
        }
        if !names.insert(name.to_string()) {
            return Err(EngineError::malformed(format!(
                "duplicate batch node name {name:?}"
            )));
        }
        match node {
            BatchNode::Call {
                function,
                arguments,
                ..
            } => {
                registry.function(function)?;
                let mut refs = Vec::new();
                collect_refs(arguments, &mut refs);
                planned.push(PlannedNode {
                    name: name.to_string(),
                    deps: refs,
                    kind: PlannedKind::Call {
                        function: function.clone(),
                        arguments: arguments.clone(),
                    },
                });
            }
            BatchNode::Evaluate { expression, .. } => {
                let parsed = parse_expression(expression, &ctx.limits)?;
                let mut bindings = BTreeMap::new();
                parsed.collect_bindings(&mut bindings);
                planned.push(PlannedNode {
                    name: name.to_string(),
                    deps: bindings.keys().cloned().collect(),
                    kind: PlannedKind::Evaluate {
                        expression: parsed,
                        source: expression.clone(),
                    },
                });
            }
            BatchNode::Recipe {
                recipe, arguments, ..
            } => {
                if !crate::recipes::exists(recipe) {
                    return Err(EngineError::new(
                        ErrorCode::NotFound,
                        format!("unknown recipe {recipe:?}"),
                    ));
                }
                let mut refs = Vec::new();
                collect_refs(arguments, &mut refs);
                planned.push(PlannedNode {
                    name: name.to_string(),
                    deps: refs,
                    kind: PlannedKind::Recipe {
                        recipe: recipe.clone(),
                        arguments: arguments.clone(),
                    },
                });
            }
        }
    }
    let known: BTreeSet<String> = names.clone();
    for node in &planned {
        for dep in &node.deps {
            if !known.contains(dep) {
                return Err(EngineError::new(
                    ErrorCode::NotFound,
                    format!("node {:?} references unknown node {dep:?}", node.name),
                ));
            }
        }
    }
    // Stable topological order: repeatedly pick the earliest node whose
    // dependencies are all satisfied.
    let mut ordered = Vec::with_capacity(planned.len());
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut remaining: Vec<usize> = (0..planned.len()).collect();
    while !remaining.is_empty() {
        let mut progress = false;
        let mut next_remaining = Vec::new();
        for index in remaining {
            let node = &planned[index];
            if node.deps.iter().all(|dep| done.contains(dep)) {
                done.insert(node.name.clone());
                ordered.push(index);
                progress = true;
            } else {
                next_remaining.push(index);
            }
        }
        remaining = next_remaining;
        if !progress {
            let names: Vec<&str> = remaining
                .iter()
                .map(|i| planned[*i].name.as_str())
                .collect();
            return Err(EngineError::malformed(format!(
                "batch dependency cycle detected among nodes {names:?}"
            )));
        }
    }
    let mut by_index: Vec<Option<PlannedNode>> = planned.into_iter().map(Some).collect();
    let ordered_nodes: Vec<PlannedNode> = ordered
        .into_iter()
        .map(|index| {
            by_index[index]
                .take()
                .ok_or_else(|| EngineError::internal("planner lost a planned node"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ordered_nodes)
}

/// Execute a batch request. Node-level failures are reported per node; the
/// top-level result is `Err` only for malformed or invalid requests.
#[allow(clippy::too_many_arguments)]
pub fn run_batch(
    registry: &Registry,
    request: &BatchRequest,
    ctx: &ExecContext,
    options: &CallOptions,
    batch_request_json: &serde_json::Value,
) -> Result<BatchEnvelope, EngineError> {
    let planned = plan(registry, request, ctx)?;
    let fail_fast = request.fail_fast.unwrap_or(true);
    let mut results: BTreeMap<String, Value> = BTreeMap::new();
    let mut statuses: BTreeMap<String, NodeStatus> = BTreeMap::new();
    let mut node_results: BTreeMap<String, NodeResult> = BTreeMap::new();
    let mut stopped = false;

    for node in &planned {
        let name = node.name.clone();
        if stopped {
            statuses.insert(name.clone(), NodeStatus::Skipped);
            node_results.insert(
                name.clone(),
                NodeResult {
                    name,
                    status: NodeStatus::Skipped,
                    result: None,
                    error: None,
                },
            );
            continue;
        }
        let dependency_failed = node
            .deps
            .iter()
            .any(|dep| !matches!(statuses.get(dep), Some(NodeStatus::Ok)));
        if dependency_failed {
            statuses.insert(name.clone(), NodeStatus::Skipped);
            node_results.insert(
                name.clone(),
                NodeResult {
                    name,
                    status: NodeStatus::Skipped,
                    result: None,
                    error: None,
                },
            );
            continue;
        }
        let outcome: Result<ResultEnvelope, EngineError> = match &node.kind {
            PlannedKind::Call {
                function,
                arguments,
            } => {
                let resolved = resolve_refs(arguments, &results)?;
                registry.call(function, &resolved, ctx, options)
            }
            PlannedKind::Recipe { recipe, arguments } => {
                let resolved = resolve_refs(arguments, &results)?;
                crate::recipes::run(registry, recipe, &resolved, ctx, options)
            }
            PlannedKind::Evaluate { expression, source } => {
                let output = crate::evaluator::evaluate(registry, expression, &results, ctx)?;
                let outcome = bicmath_core::contract::Outcome {
                    value: output.value,
                    exactness: output.exactness,
                    warnings: output.warnings,
                    assumptions: output.assumptions,
                    error_estimate: output.error_estimate,
                    trace: output.trace,
                    conversions: output.conversions,
                };
                build_envelope(
                    registry,
                    None,
                    outcome,
                    ctx,
                    options,
                    FingerprintInput::Raw {
                        request: &serde_json::json!({
                            "node": node.name,
                            "expression": source,
                        }),
                    },
                )
            }
        };
        match outcome {
            Ok(envelope) => {
                statuses.insert(name.clone(), NodeStatus::Ok);
                results.insert(name.clone(), envelope.result.clone());
                node_results.insert(
                    name.clone(),
                    NodeResult {
                        name,
                        status: NodeStatus::Ok,
                        result: Some(envelope),
                        error: None,
                    },
                );
            }
            Err(error) => {
                statuses.insert(name.clone(), NodeStatus::Error);
                if fail_fast {
                    stopped = true;
                }
                node_results.insert(
                    name.clone(),
                    NodeResult {
                        name,
                        status: NodeStatus::Error,
                        result: None,
                        error: Some(error),
                    },
                );
            }
        }
    }

    let requested: Vec<String> = match &request.outputs {
        Some(outputs) => {
            for name in outputs {
                if !node_results.contains_key(name) {
                    return Err(EngineError::new(
                        ErrorCode::NotFound,
                        format!("requested output {name:?} is not a batch node"),
                    ));
                }
            }
            outputs.clone()
        }
        None => planned.iter().map(|node| node.name.clone()).collect(),
    };
    let outputs: Vec<NodeResult> = requested
        .into_iter()
        .map(|name| {
            node_results.get(&name).cloned().ok_or_else(|| {
                EngineError::internal(format!("missing output node result for {name:?}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let fingerprint = calculation_fingerprint(
        "batch",
        batch_request_json,
        None,
        &registry.engine_info().modules,
        &ctx.effective(),
        ctx.seed,
    )?;
    let receipt = if options.include_receipt {
        Some(Receipt::new(
            fingerprint.clone(),
            batch_request_json.clone(),
            None,
            registry.engine_info().clone(),
            ctx.effective(),
            ctx.seed,
        ))
    } else {
        None
    };
    Ok(BatchEnvelope {
        schema_version: bicmath_core::WIRE_SCHEMA_VERSION,
        engine: registry.engine_info().clone(),
        node_count: planned.len(),
        outputs,
        fingerprint,
        receipt,
    })
}

/// Build an error response for a malformed batch.
pub fn batch_error(registry: &Registry, error: EngineError) -> ErrorResponse {
    ErrorResponse::new(registry.engine_info().clone(), error)
}
