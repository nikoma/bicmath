//! MCP transport and schema adapters for BicMath.
//!
//! Uses the official Rust SDK (`rmcp`) pinned to a stable release. The compact
//! profile exposes six dispatcher tools; the expanded profile exposes one typed
//! tool per enabled function, generated from the same registry. stdout is
//! reserved for protocol messages; logs go to stderr.

pub mod tools;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use bicmath_core::context::ExecContext;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::limits::CancellationToken;
use bicmath_engine::Engine;
use bicmath_engine::batch::BatchRequest;
use bicmath_engine::request::{CallRequest, EvaluateRequest, FunctionFilter};
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer};
use tokio::sync::Semaphore;

pub use tools::{McpConfig, ToolProfile};

/// Protocol revisions this server promises and tests.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[ProtocolVersion] =
    &[ProtocolVersion::V_2026_07_28, ProtocolVersion::V_2025_11_25];

/// The BicMath MCP server handler.
#[derive(Clone)]
pub struct BicMathServer {
    engine: Arc<Engine>,
    tools: Arc<Vec<Tool>>,
    expanded: Arc<BTreeMap<String, String>>,
    semaphore: Arc<Semaphore>,
    timeout_ms: u64,
    config: McpConfig,
}

impl BicMathServer {
    pub fn new(engine: Engine, config: McpConfig) -> BicMathServer {
        let (tools, expanded) = match config.profile {
            ToolProfile::Compact => (tools::compact_tools(), BTreeMap::new()),
            ToolProfile::Expanded => {
                tools::expanded_tools(engine.registry().function_descriptors())
            }
        };
        let timeout_ms = engine.config().limits.request_timeout_ms;
        BicMathServer {
            engine: Arc::new(engine),
            tools: Arc::new(tools),
            expanded: Arc::new(expanded),
            semaphore: Arc::new(Semaphore::new(config.max_in_flight.max(1))),
            timeout_ms,
            config,
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn profile(&self) -> ToolProfile {
        self.config.profile
    }

    /// Run a CPU-bound engine operation on a bounded blocking worker, bridging
    /// MCP cancellation to the engine's cooperative token.
    async fn run<T, F>(&self, context: &RequestContext<RoleServer>, f: F) -> Result<T, EngineError>
    where
        T: Send + 'static,
        F: FnOnce(&ExecContext) -> Result<T, EngineError> + Send + 'static,
    {
        let token = CancellationToken::new();
        let mcp_token = context.ct.clone();
        let bridge = token.clone();
        tokio::spawn(async move {
            mcp_token.cancelled().await;
            bridge.cancel();
        });
        self.run_with_token(token, f).await
    }

    /// Run on a bounded blocking worker with an explicit cooperative token.
    /// The worker checks the token throughout execution, so cancellation stops
    /// actual computation and releases worker capacity.
    pub(crate) async fn run_with_token<T, F>(
        &self,
        token: CancellationToken,
        f: F,
    ) -> Result<T, EngineError>
    where
        T: Send + 'static,
        F: FnOnce(&ExecContext) -> Result<T, EngineError> + Send + 'static,
    {
        let base = self.engine.base_context();
        let exec = ExecContext {
            cancellation: token,
            ..base
        };
        let permit = match tokio::time::timeout(
            Duration::from_millis(self.timeout_ms.max(1)),
            self.semaphore.clone().acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                return Err(EngineError::internal("calculation worker pool is closed"));
            }
            Err(_) => {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    "server is at capacity; retry later",
                ));
            }
        };
        let join = tokio::task::spawn_blocking(move || {
            let result = f(&exec);
            drop(permit);
            result
        });
        match join.await {
            Ok(result) => result,
            Err(error) => Err(EngineError::internal(format!(
                "calculation worker failed: {error}"
            ))),
        }
    }

    fn error_result(&self, error: EngineError) -> CallToolResult {
        let response = bicmath_core::envelope::ErrorResponse::new(
            self.engine.registry().engine_info().clone(),
            error,
        );
        match serde_json::to_value(response) {
            Ok(value) => CallToolResult::structured_error(value),
            Err(error) => CallToolResult::structured_error(serde_json::json!({
                "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
                "error": {"code": "internal", "message": error.to_string()}
            })),
        }
    }

    fn success_result<T: serde::Serialize>(&self, value: T) -> CallToolResult {
        match serde_json::to_value(value) {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => self.error_result(EngineError::internal(format!(
                "result is not serializable: {error}"
            ))),
        }
    }

    fn parse_args<T: serde::de::DeserializeOwned>(
        &self,
        args: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<T, EngineError> {
        serde_json::from_value(serde_json::Value::Object(args.clone()))
            .map_err(|error| EngineError::malformed(format!("invalid tool arguments: {error}")))
    }

    async fn list_modules_tool(&self) -> CallToolResult {
        let modules = self.engine.list_modules();
        self.success_result(serde_json::json!({
            "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
            "engine": self.engine.registry().engine_info(),
            "modules": modules
        }))
    }

    async fn list_functions_tool(
        &self,
        args: &serde_json::Map<String, serde_json::Value>,
    ) -> CallToolResult {
        match self.parse_args::<FunctionFilter>(args) {
            Ok(filter) => {
                let page = self.engine.list_functions(&filter);
                self.success_result(serde_json::json!({
                    "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
                    "page": page
                }))
            }
            Err(error) => self.error_result(error),
        }
    }

    async fn describe_function_tool(
        &self,
        args: &serde_json::Map<String, serde_json::Value>,
    ) -> CallToolResult {
        #[derive(serde::Deserialize)]
        struct DescribeArgs {
            function: String,
        }
        match self.parse_args::<DescribeArgs>(args) {
            Ok(describe) => match self.engine.describe(&describe.function) {
                Ok(value) => self.success_result(value),
                Err(error) => self.error_result(error),
            },
            Err(error) => self.error_result(error),
        }
    }

    async fn calculate_tool(
        &self,
        args: &serde_json::Map<String, serde_json::Value>,
        context: &RequestContext<RoleServer>,
    ) -> CallToolResult {
        let request = match self.parse_args::<CallRequest>(args) {
            Ok(request) => request,
            Err(error) => return self.error_result(error),
        };
        let engine = self.engine.clone();
        match self
            .run(context, move |exec| engine.call(&request, exec))
            .await
        {
            Ok(envelope) => self.success_result(envelope),
            Err(error) => self.error_result(error),
        }
    }

    async fn evaluate_tool(
        &self,
        args: &serde_json::Map<String, serde_json::Value>,
        context: &RequestContext<RoleServer>,
    ) -> CallToolResult {
        let request = match self.parse_args::<EvaluateRequest>(args) {
            Ok(request) => request,
            Err(error) => return self.error_result(error),
        };
        let engine = self.engine.clone();
        match self
            .run(context, move |exec| engine.evaluate(&request, exec))
            .await
        {
            Ok(envelope) => self.success_result(envelope),
            Err(error) => self.error_result(error),
        }
    }

    async fn batch_tool(
        &self,
        args: &serde_json::Map<String, serde_json::Value>,
        context: &RequestContext<RoleServer>,
    ) -> CallToolResult {
        let request = match self.parse_args::<BatchRequest>(args) {
            Ok(request) => request,
            Err(error) => return self.error_result(error),
        };
        let engine = self.engine.clone();
        match self
            .run(context, move |exec| engine.batch(&request, exec))
            .await
        {
            Ok(envelope) => self.success_result(envelope),
            Err(error) => self.error_result(error),
        }
    }

    /// Call one function directly (expanded profile), using the same engine
    /// path as `calculate`.
    async fn expanded_call(
        &self,
        function_id: &str,
        args: &serde_json::Map<String, serde_json::Value>,
        context: &RequestContext<RoleServer>,
    ) -> CallToolResult {
        let mut request = serde_json::Map::new();
        request.insert(
            "function".to_string(),
            serde_json::Value::String(function_id.to_string()),
        );
        request.insert(
            "arguments".to_string(),
            serde_json::Value::Object(args.clone()),
        );
        self.calculate_tool(&request, context).await
    }
}

impl ServerHandler for BicMathServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                self.config.server_name.clone(),
                self.config.server_version.clone(),
            ))
            .with_protocol_version(ProtocolVersion::V_2026_07_28);
        if let Some(instructions) = &self.config.instructions {
            info = info.with_instructions(instructions.clone());
        }
        info
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_PROTOCOL_VERSIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.as_ref().clone()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools
            .iter()
            .find(|tool| tool.name.as_ref() == name)
            .cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request.arguments.clone().unwrap_or_default();
        let result = match request.name.as_ref() {
            "list_modules" => self.list_modules_tool().await,
            "list_functions" => self.list_functions_tool(&args).await,
            "describe_function" => self.describe_function_tool(&args).await,
            "calculate" => self.calculate_tool(&args, &context).await,
            "evaluate" => self.evaluate_tool(&args, &context).await,
            "batch" => self.batch_tool(&args, &context).await,
            other => match self.expanded.get(other) {
                Some(function_id) => {
                    let function_id = function_id.clone();
                    self.expanded_call(&function_id, &args, &context).await
                }
                None => {
                    return Err(ErrorData::new(
                        rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                        format!("unknown tool {other:?}"),
                        None,
                    ));
                }
            },
        };
        Ok(CallToolResponse::Complete(result))
    }
}

/// Serve over stdio. stdout is protocol-only; diagnostics belong on stderr.
pub async fn serve_stdio(
    engine: Engine,
    config: McpConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::ServiceExt;
    let server = BicMathServer::new(engine, config);
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

#[cfg(feature = "http")]
pub mod http {
    //! Optional Streamable HTTP transport, bound to loopback by default.

    use super::*;
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };
    use std::net::SocketAddr;

    /// Serve the MCP Streamable HTTP transport at `/mcp`.
    pub async fn serve(
        engine: Engine,
        config: McpConfig,
        bind: SocketAddr,
        max_request_bytes: usize,
    ) -> std::io::Result<()> {
        let mut http_config = StreamableHttpServerConfig::default()
            .with_json_response(true)
            .with_max_request_body_bytes(max_request_bytes);
        if !config.allowed_hosts.is_empty() {
            http_config = http_config.with_allowed_hosts(config.allowed_hosts.clone());
        }
        if !config.allowed_origins.is_empty() {
            http_config = http_config.with_allowed_origins(config.allowed_origins.clone());
        }
        let shared = Arc::new((engine, config));
        let factory = {
            let shared = shared.clone();
            move || Ok(BicMathServer::new(shared.0.clone(), shared.1.clone()))
        };
        let service =
            StreamableHttpService::new(factory, LocalSessionManager::default().into(), http_config);
        let router = axum::Router::new().nest_service("/mcp", service);
        let listener = tokio::net::TcpListener::bind(bind).await?;
        axum::serve(listener, router).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_engine::EngineConfig;

    fn server(profile: ToolProfile) -> BicMathServer {
        let engine = Engine::full(EngineConfig::default()).unwrap();
        BicMathServer::new(
            engine,
            McpConfig {
                profile,
                ..McpConfig::default()
            },
        )
    }

    #[test]
    fn compact_profile_exposes_six_tools() {
        let server = server(ToolProfile::Compact);
        assert_eq!(server.tools.len(), 6);
    }

    #[test]
    fn expanded_profile_exposes_typed_tools() {
        let server = server(ToolProfile::Expanded);
        assert!(server.tools.len() > 100);
        assert!(server.expanded.contains_key("statistics__median"));
        let tool = server
            .tools
            .iter()
            .find(|tool| tool.name == "statistics__median")
            .unwrap();
        let schema = tool.input_schema.as_ref();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["values"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "values")
        );
    }

    #[test]
    fn expanded_tool_schemas_do_not_drift_from_descriptors() {
        let engine = Engine::full(EngineConfig::default()).unwrap();
        let (tools, _) = tools::expanded_tools(engine.registry().function_descriptors());
        for descriptor in engine.registry().function_descriptors() {
            let name = tools::function_tool_name(&descriptor.id);
            let tool = tools.iter().find(|tool| tool.name == name).unwrap();
            for param in &descriptor.parameters {
                assert!(
                    tool.input_schema["properties"]
                        .as_object()
                        .unwrap()
                        .contains_key(&param.name),
                    "{} missing parameter {}",
                    descriptor.id,
                    param.name
                );
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_stops_computation_and_releases_capacity() {
        let engine = Engine::full(EngineConfig::default()).unwrap();
        let server = BicMathServer::new(
            engine,
            McpConfig {
                max_in_flight: 1,
                ..McpConfig::default()
            },
        );
        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel.cancel();
        });
        let error = server
            .run_with_token(token, |exec| {
                // A long, cooperative loop: checks cancellation and deadline.
                for _ in 0..10_000_000u64 {
                    exec.check()?;
                }
                Ok(())
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
        // The worker permit must have been released: a second call succeeds.
        let value = server
            .run_with_token(CancellationToken::new(), |_exec| Ok(42u64))
            .await
            .unwrap();
        assert_eq!(value, 42);
    }
}
