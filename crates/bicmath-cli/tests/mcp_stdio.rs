//! End-to-end MCP interoperability tests through a real subprocess over stdio.

use rmcp::ClientLifecycleMode;
use rmcp::model::{CallToolRequestParams, ClientInfo, JsonObject, ProtocolVersion};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{ClientServiceExt, ServiceExt};
use serde_json::json;
use tokio::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_bicmath")
}

fn transport() -> TokioChildProcess {
    let command = Command::new(binary()).configure(|cmd| {
        cmd.arg("serve")
            .arg("--transport")
            .arg("stdio")
            .stderr(std::process::Stdio::null());
    });
    TokioChildProcess::new(command).expect("spawn bicmath")
}

fn args(value: serde_json::Value) -> JsonObject {
    value.as_object().expect("object").clone()
}

async fn call_structured(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let result = client
        .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(args(arguments)))
        .await
        .expect("tool call transport succeeds");
    assert_eq!(
        result.is_error,
        Some(false),
        "tool reported an error: {result:?}"
    );
    result.structured_content.expect("structured content")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_initialize_lifecycle_interop() {
    // `().serve(...)` uses the legacy initialize handshake, exercising the
    // 2025-11-25 compatibility path.
    let client = ().serve(transport()).await.expect("serve client");

    let tools = client.list_all_tools().await.expect("list tools");
    assert_eq!(tools.len(), 6);
    let names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
    for expected in [
        "list_modules",
        "list_functions",
        "describe_function",
        "calculate",
        "evaluate",
        "batch",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }

    // Exact decimal through the real transport.
    let envelope = call_structured(
        &client,
        "calculate",
        json!({"function": "arithmetic.add", "arguments": {"a": "0.1", "b": "0.2"}}),
    )
    .await;
    assert_eq!(
        envelope["result"],
        json!({"kind": "decimal", "value": "0.3"})
    );
    assert_eq!(envelope["exactness"], "exact");

    // Large integer survives JSON without a float round trip.
    let envelope = call_structured(
        &client,
        "calculate",
        json!({
            "function": "arithmetic.add",
            "arguments": {"a": {"kind": "integer", "value": "9007199254740993"}, "b": 1}
        }),
    )
    .await;
    assert_eq!(
        envelope["result"],
        json!({"kind": "integer", "value": "9007199254740994"})
    );

    // Expression evaluation.
    let envelope = call_structured(
        &client,
        "evaluate",
        json!({"expression": "finance.npv(rate = 0.08, cashflows = [-10000, 4000, 4000, 4000], currency = \"USD\")"}),
    )
    .await;
    assert!(envelope["result"]["npv"].is_object());

    // Batch with a reference.
    let envelope = call_structured(
        &client,
        "batch",
        json!({
            "nodes": [
                {"type": "call", "name": "total", "function": "arithmetic.add",
                 "arguments": {"a": 2, "b": 3}},
                {"type": "call", "name": "double", "function": "arithmetic.mul",
                 "arguments": {"a": {"$ref": "total"}, "b": 2}}
            ],
            "outputs": ["double"]
        }),
    )
    .await;
    assert_eq!(
        envelope["outputs"][0]["result"]["result"],
        json!({"kind": "integer", "value": "10"})
    );

    // Discovery tools.
    let envelope = call_structured(&client, "list_modules", json!({})).await;
    assert!(envelope["modules"].as_array().unwrap().len() >= 6);
    let envelope = call_structured(
        &client,
        "list_functions",
        json!({"module": "statistics", "limit": 5}),
    )
    .await;
    assert_eq!(envelope["page"]["functions"].as_array().unwrap().len(), 5);
    let envelope = call_structured(
        &client,
        "describe_function",
        json!({"function": "statistics.median"}),
    )
    .await;
    assert_eq!(envelope["function"]["id"], "statistics.median");
    assert!(
        envelope["markdown"]
            .as_str()
            .unwrap()
            .contains("Parameters")
    );

    // Structured error shape, not a protocol failure.
    let result = client
        .call_tool(
            CallToolRequestParams::new("calculate").with_arguments(args(json!({
                "function": "arithmetic.div",
                "arguments": {"a": 1, "b": 0}
            }))),
        )
        .await
        .expect("transport succeeds");
    assert_eq!(result.is_error, Some(true));
    let structured = result.structured_content.expect("structured error");
    assert_eq!(structured["error"]["code"], "division_by_zero");
    assert_eq!(structured["schema_version"], 1);

    // Unknown tool is a protocol-level method-not-found.
    let error = client
        .call_tool(CallToolRequestParams::new("definitely_not_a_tool"))
        .await
        .expect_err("unknown tool must be a protocol error");
    match error {
        rmcp::ServiceError::McpError(data) => {
            assert_eq!(data.code, rmcp::model::ErrorCode::METHOD_NOT_FOUND);
        }
        other => panic!("expected method-not-found, got {other:?}"),
    }

    client.cancel().await.expect("shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn modern_discover_lifecycle_interop() {
    let client = ClientInfo::default()
        .serve_with_lifecycle(
            transport(),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .expect("serve modern client");
    let tools = client.list_all_tools().await.expect("list tools");
    assert_eq!(tools.len(), 6);
    let result = client
        .call_tool(
            CallToolRequestParams::new("calculate").with_arguments(args(json!({
                "function": "statistics.median",
                "arguments": {"values": [2, 10, 30]}
            }))),
        )
        .await
        .expect("call");
    assert_eq!(result.is_error, Some(false));
    assert_eq!(
        result.structured_content.unwrap()["result"],
        json!({"kind": "integer", "value": "10"})
    );
    client.cancel().await.expect("shutdown");
}
