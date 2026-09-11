//! End-to-end MCP Streamable HTTP interoperability test.
//!
//! Runs only with `--features http`; starts the real binary and talks to it
//! with the official SDK's Streamable HTTP client.

#![cfg(feature = "http")]

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::json;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("addr").port()
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server(port: u16) -> ChildGuard {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bicmath"))
        .args([
            "serve",
            "--transport",
            "http",
            "--bind",
            &format!("127.0.0.1:{port}"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");
    // Wait for the port to accept connections.
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return ChildGuard(child);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    panic!("server did not start: {stderr}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streamable_http_interop() {
    let port = free_port();
    let _guard = start_server(port);
    let transport = StreamableHttpClientTransport::from_uri(format!("http://127.0.0.1:{port}/mcp"));
    let client = ().serve(transport).await.expect("serve http client");

    let tools = client.list_all_tools().await.expect("list tools");
    assert_eq!(tools.len(), 6);

    let result = client
        .call_tool(
            CallToolRequestParams::new("calculate").with_arguments(
                json!({
                    "function": "arithmetic.add",
                    "arguments": {"a": "0.1", "b": "0.2"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .expect("call over http");
    assert_eq!(result.is_error, Some(false));
    let structured = result.structured_content.expect("structured");
    assert_eq!(
        structured["result"],
        json!({"kind": "decimal", "value": "0.3"})
    );
    assert_eq!(structured["exactness"], "exact");

    // A tool-level error still travels as structured content, not a transport
    // failure.
    let result = client
        .call_tool(
            CallToolRequestParams::new("calculate").with_arguments(
                json!({
                    "function": "arithmetic.div",
                    "arguments": {"a": 1, "b": 0}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .expect("transport succeeds");
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "division_by_zero"
    );

    client.cancel().await.expect("shutdown");
}
