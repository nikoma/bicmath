//! BicMath command line interface and native MCP server.
//!
//! Exit codes:
//! - 0: success
//! - 1: calculation or engine error (a structured error is printed)
//! - 2: configuration, transport, or I/O error (including usage errors)

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

mod sdk;
use serde::{Deserialize, Serialize};

use bicmath_core::envelope::ErrorResponse;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::fingerprint::Receipt;
use bicmath_engine::{
    BatchRequest, CallRequest, Engine, EngineConfig, EvaluateRequest, FunctionFilter,
};
use bicmath_mcp::{McpConfig, ToolProfile};

#[derive(Parser, Debug)]
#[command(
    name = "bicmath",
    version,
    about = "Deterministic computation engine for mathematics, statistics, science, and finance",
    long_about = "BicMath executes defined computational methods and returns inspectable results. \
                  Exact decimal, rational, and integer inputs stay exact; float64 results are \
                  approximate. It does not certify that the right business question or statistical \
                  design was chosen."
)]
struct Cli {
    /// Path to a JSON configuration file.
    #[arg(long, global = true, env = "BICMATH_CONFIG")]
    config: Option<PathBuf>,
    /// Output format.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
    /// Accepted for compatibility; JSON is already the default output format.
    #[arg(long, global = true)]
    json: bool,
    /// Increase log verbosity on stderr (error, warn, info, debug, trace).
    #[arg(
        long,
        global = true,
        default_value_t = LogLevel::Warn,
        value_enum,
        env = "BICMATH_LOG"
    )]
    log: LogLevel,
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the MCP server over stdio or Streamable HTTP.
    Serve {
        /// Transport to serve.
        #[arg(long, value_enum, default_value_t = Transport::Stdio)]
        transport: Transport,
        /// Bind address for the HTTP transport (loopback by default).
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
        /// Tool exposure profile: compact (six dispatcher tools) or expanded
        /// (one typed tool per function).
        #[arg(long, value_enum, env = "BICMATH_PROFILE")]
        profile: Option<ProfileArg>,
        /// Maximum concurrent calculations.
        #[arg(long, env = "BICMATH_MAX_IN_FLIGHT")]
        max_in_flight: Option<usize>,
        /// Comma-separated modules to disable at runtime.
        #[arg(long, env = "BICMATH_DISABLED_MODULES", value_delimiter = ',')]
        disable_module: Vec<String>,
    },
    /// List enabled modules.
    Modules,
    /// List functions with optional filtering.
    Functions {
        #[arg(long)]
        module: Option<String>,
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Describe one function completely.
    Describe {
        /// Qualified function id, e.g. statistics.median.
        function: String,
    },
    /// Execute one function from a JSON request (file or stdin).
    Calculate {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    /// Evaluate an expression from a JSON request (file or stdin).
    Evaluate {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    /// Execute a batch from a JSON request (file or stdin).
    Batch {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    /// Re-run a stored replay receipt and check the fingerprint.
    Replay {
        #[arg(long)]
        receipt: PathBuf,
    },
    /// Validate the installation and every documented example.
    Doctor,
    /// Print the effective configuration (never prints credentials).
    Config,
    /// Generate module/function reference documentation from the registry.
    Docs {
        /// Output directory; one Markdown file per module.
        #[arg(long, default_value = "docs/methods")]
        out: PathBuf,
    },
    /// Generate a typed client SDK from the registry (TypeScript or Python).
    Sdk {
        #[arg(long, value_enum, default_value_t = SdkLanguage::Typescript)]
        language: SdkLanguage,
        /// Output file; defaults to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Transport {
    Stdio,
    Http,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProfileArg {
    Compact,
    Expanded,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum SdkLanguage {
    Typescript,
    Python,
}

impl From<ProfileArg> for ToolProfile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::Compact => ToolProfile::Compact,
            ProfileArg::Expanded => ToolProfile::Expanded,
        }
    }
}

/// Validated configuration file. Unknown keys are rejected so misspellings
/// cannot silently change behaviour.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CliConfig {
    engine: EngineConfig,
    profile: Option<ProfileArg>,
    transport: TransportConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TransportConfig {
    bind: String,
    max_in_flight: usize,
    allowed_hosts: Vec<String>,
    allowed_origins: Vec<String>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        TransportConfig {
            bind: "127.0.0.1:8080".to_string(),
            max_in_flight: 4,
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
        }
    }
}

enum CliError {
    Calculation(EngineError),
    Configuration(String),
}

impl From<EngineError> for CliError {
    fn from(value: EngineError) -> Self {
        CliError::Calculation(value)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let level = cli.log.as_str();
    let filter = tracing_subscriber::EnvFilter::new(format!("bicmath={level},rmcp={level}"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
    let format = if cli.json {
        OutputFormat::Json
    } else {
        cli.format
    };
    match run(cli, format) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Calculation(error)) => {
            let response = ErrorResponse::new(bicmath_core::envelope::EngineInfo::current(), error);
            print_json(&response);
            ExitCode::from(1)
        }
        Err(CliError::Configuration(message)) => {
            eprintln!("configuration error: {message}");
            ExitCode::from(2)
        }
    }
}

fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("failed to serialize output: {error}"),
    }
}

fn run(cli: Cli, format: OutputFormat) -> Result<(), CliError> {
    let config = load_config(cli.config.as_ref())?;
    match cli.command {
        Command::Config => {
            print_json(&config);
            Ok(())
        }
        Command::Docs { out } => {
            let engine = build_engine(&config)?;
            generate_docs(&engine, &out)?;
            eprintln!("wrote generated function reference to {}", out.display());
            Ok(())
        }
        Command::Sdk { language, out } => {
            let engine = build_engine(&config)?;
            let generated = match language {
                SdkLanguage::Typescript => sdk::generate_typescript(&engine),
                SdkLanguage::Python => sdk::generate_python(&engine),
            };
            match out {
                Some(path) => {
                    fs::write(&path, generated).map_err(|error| {
                        CliError::Configuration(format!("cannot write {}: {error}", path.display()))
                    })?;
                    eprintln!("wrote SDK to {}", path.display());
                }
                None => print!("{generated}"),
            }
            Ok(())
        }
        Command::Doctor => {
            let engine = build_engine(&config)?;
            doctor(&engine, format)
        }
        Command::Modules => {
            let engine = build_engine(&config)?;
            let modules = engine.list_modules();
            if format == OutputFormat::Json {
                print_json(&serde_json::json!({
                    "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
                    "engine": engine.registry().engine_info(),
                    "modules": modules,
                }));
            } else {
                for module in modules {
                    println!(
                        "{:<16} {:<10} {} function(s)  {}",
                        module.id, module.version, module.function_count, module.description
                    );
                }
            }
            Ok(())
        }
        Command::Functions {
            module,
            query,
            offset,
            limit,
        } => {
            let engine = build_engine(&config)?;
            let filter = FunctionFilter {
                module,
                query,
                offset: Some(offset),
                limit: Some(limit),
            };
            let page = engine.list_functions(&filter);
            if format == OutputFormat::Json {
                print_json(&serde_json::json!({
                    "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
                    "page": page,
                }));
            } else {
                for function in &page.functions {
                    println!("{:<40} {}", function.id, function.summary);
                }
                println!(
                    "({} of {} shown; offset {})",
                    page.functions.len(),
                    page.total,
                    page.offset
                );
            }
            Ok(())
        }
        Command::Describe { function } => {
            let engine = build_engine(&config)?;
            let description = engine.describe(&function)?;
            if format == OutputFormat::Json {
                print_json(&description);
            } else {
                println!("{}", description["markdown"].as_str().unwrap_or_default());
            }
            Ok(())
        }
        Command::Calculate { request } => {
            let engine = build_engine(&config)?;
            let request: CallRequest = read_request(request)?;
            let envelope = engine.call(&request, &engine.base_context())?;
            emit_envelope(&envelope, format);
            Ok(())
        }
        Command::Evaluate { request } => {
            let engine = build_engine(&config)?;
            let request: EvaluateRequest = read_request(request)?;
            let envelope = engine.evaluate(&request, &engine.base_context())?;
            emit_envelope(&envelope, format);
            Ok(())
        }
        Command::Batch { request } => {
            let engine = build_engine(&config)?;
            let request: BatchRequest = read_request(request)?;
            let envelope = engine.batch(&request, &engine.base_context())?;
            if format == OutputFormat::Json {
                print_json(&envelope);
            } else {
                for node in &envelope.outputs {
                    match node.status {
                        bicmath_engine::NodeStatus::Ok => {
                            let result = node
                                .result
                                .as_ref()
                                .map(|envelope| {
                                    serde_json::to_string(&envelope.result).unwrap_or_default()
                                })
                                .unwrap_or_default();
                            println!("{:<16} ok       {result}", node.name);
                        }
                        bicmath_engine::NodeStatus::Error => {
                            println!(
                                "{:<16} error    {}",
                                node.name,
                                node.error
                                    .as_ref()
                                    .map(|error| error.message.clone())
                                    .unwrap_or_default()
                            );
                        }
                        bicmath_engine::NodeStatus::Skipped => {
                            println!("{:<16} skipped", node.name);
                        }
                    }
                }
            }
            Ok(())
        }
        Command::Replay { receipt } => {
            let engine = build_engine(&config)?;
            let text = fs::read_to_string(&receipt).map_err(|error| {
                CliError::Configuration(format!("cannot read receipt: {error}"))
            })?;
            let receipt: Receipt = serde_json::from_str(&text)
                .map_err(|error| CliError::Configuration(format!("invalid receipt: {error}")))?;
            let (replay_fingerprint, envelope) = replay(&engine, &receipt)?;
            let matches = replay_fingerprint == receipt.fingerprint;
            print_json(&serde_json::json!({
                "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
                "replayed": true,
                "fingerprint_matches": matches,
                "receipt_fingerprint": receipt.fingerprint,
                "replay_fingerprint": replay_fingerprint,
                "envelope": envelope,
            }));
            if !matches {
                eprintln!(
                    "warning: replay fingerprint differs; the request, context, or module \
                     versions changed"
                );
            }
            Ok(())
        }
        Command::Serve {
            transport,
            bind,
            profile,
            max_in_flight,
            disable_module,
        } => {
            let mut config = config;
            if let Some(profile) = profile {
                config.profile = Some(profile);
            }
            if let Some(max_in_flight) = max_in_flight {
                config.transport.max_in_flight = max_in_flight;
            }
            for module in disable_module {
                if !config.engine.disabled_modules.contains(&module) {
                    config.engine.disabled_modules.push(module);
                }
            }
            let engine = build_engine(&config)?;
            let profile = config.profile.unwrap_or(ProfileArg::Compact).into();
            let mcp_config = McpConfig {
                profile,
                max_in_flight: config.transport.max_in_flight,
                allowed_hosts: config.transport.allowed_hosts.clone(),
                allowed_origins: config.transport.allowed_origins.clone(),
                ..McpConfig::default()
            };
            match transport {
                Transport::Stdio => {
                    let runtime = tokio::runtime::Runtime::new()
                        .map_err(|error| CliError::Configuration(error.to_string()))?;
                    runtime
                        .block_on(bicmath_mcp::serve_stdio(engine, mcp_config))
                        .map_err(|error| CliError::Configuration(error.to_string()))
                }
                Transport::Http => serve_http(engine, mcp_config, &bind),
            }
        }
    }
}

#[cfg(feature = "http")]
fn serve_http(engine: Engine, config: McpConfig, bind: &str) -> Result<(), CliError> {
    let address: std::net::SocketAddr = bind.parse().map_err(|error| {
        CliError::Configuration(format!("invalid bind address {bind:?}: {error}"))
    })?;
    if !address.ip().is_loopback() {
        eprintln!(
            "warning: binding to {} exposes the server beyond loopback; configure \
             authentication and a trusted gateway before remote deployment",
            address
        );
    }
    let max_bytes = engine.config().limits.max_request_bytes;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| CliError::Configuration(error.to_string()))?;
    runtime
        .block_on(bicmath_mcp::http::serve(engine, config, address, max_bytes))
        .map_err(|error| CliError::Configuration(error.to_string()))
}

#[cfg(not(feature = "http"))]
fn serve_http(_engine: Engine, _config: McpConfig, _bind: &str) -> Result<(), CliError> {
    Err(CliError::Configuration(
        "this build was compiled without the \"http\" feature; rebuild with \
         `cargo build --features bicmath-cli/http`"
            .to_string(),
    ))
}

/// Generate one Markdown file per module from the live registry. Method
/// anchors referenced by descriptors are emitted so links resolve.
fn generate_docs(engine: &Engine, out_dir: &std::path::Path) -> Result<(), CliError> {
    use std::collections::BTreeMap;
    std::fs::create_dir_all(out_dir).map_err(|error| {
        CliError::Configuration(format!("cannot create {}: {error}", out_dir.display()))
    })?;
    let mut by_module: BTreeMap<String, Vec<&bicmath_core::contract::FunctionDescriptor>> =
        BTreeMap::new();
    for descriptor in engine.registry().function_descriptors() {
        by_module
            .entry(descriptor.module.clone())
            .or_default()
            .push(descriptor);
    }
    for module in engine.list_modules() {
        let descriptors = by_module.remove(&module.id).unwrap_or_default();
        let mut text = String::new();
        text.push_str(&format!("# {} module reference\n\n", module.title));
        text.push_str(&format!("{}\n\n", module.description));
        text.push_str(&format!(
            "- Module id: `{}`\n- Version: {}\n- Capabilities: {}\n- Supported modes: {}\n- Functions: {}\n\n",
            module.id,
            module.version,
            module.capabilities.join(", "),
            module
                .supported_modes
                .iter()
                .map(|mode| mode.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            descriptors.len()
        ));
        text.push_str(
            "This file is generated from the live registry by `bicmath docs`; the \
             function schemas, domains, and examples are the same ones the engine \
             validates against at runtime.\n\n",
        );
        let mut sorted = descriptors;
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        for descriptor in sorted {
            let short = descriptor
                .id
                .split_once('.')
                .map(|(_, name)| name)
                .unwrap_or(&descriptor.id);
            if let Some(anchor) = descriptor
                .method_ref
                .split_once('#')
                .map(|(_, anchor)| anchor)
            {
                text.push_str(&format!("<a id=\"{anchor}\"></a>\n\n"));
            }
            let rendered = bicmath_engine::render_function_markdown(descriptor);
            let body = rendered
                .strip_prefix(&format!("# {}\n", descriptor.id))
                .unwrap_or(&rendered)
                .trim_start_matches('\n');
            text.push_str(&format!("## {short}\n\n{body}\n"));
        }
        let path = out_dir.join(format!("{}.md", module.id));
        std::fs::write(&path, text).map_err(|error| {
            CliError::Configuration(format!("cannot write {}: {error}", path.display()))
        })?;
    }
    Ok(())
}

fn load_config(path: Option<&PathBuf>) -> Result<CliConfig, CliError> {
    let mut config = CliConfig::default();
    let path = path
        .cloned()
        .or_else(|| std::env::var("BICMATH_CONFIG").ok().map(PathBuf::from));
    if let Some(path) = path {
        let text = fs::read_to_string(&path).map_err(|error| {
            CliError::Configuration(format!("cannot read config {}: {error}", path.display()))
        })?;
        config = serde_json::from_str(&text).map_err(|error| {
            CliError::Configuration(format!("invalid config {}: {error}", path.display()))
        })?;
    }
    Ok(config)
}

fn build_engine(config: &CliConfig) -> Result<Engine, CliError> {
    Engine::full(config.engine.clone()).map_err(|errors| {
        CliError::Configuration(format!(
            "engine configuration is invalid: {}",
            errors
                .iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })
}

fn read_request<T: serde::de::DeserializeOwned>(path: Option<PathBuf>) -> Result<T, CliError> {
    let mut text = String::new();
    match path {
        Some(path) if path != *"-" => {
            text = fs::read_to_string(&path).map_err(|error| {
                CliError::Configuration(format!("cannot read request {}: {error}", path.display()))
            })?;
        }
        _ => {
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|error| CliError::Configuration(format!("cannot read stdin: {error}")))?;
        }
    }
    serde_json::from_str(&text)
        .map_err(|error| CliError::Configuration(format!("invalid request JSON: {error}")))
}

fn emit_envelope(envelope: &bicmath_core::envelope::ResultEnvelope, format: OutputFormat) {
    if format == OutputFormat::Json {
        print_json(envelope);
    } else {
        let result = serde_json::to_string(&envelope.result).unwrap_or_default();
        println!("result: {result}");
        println!("exactness: {}", envelope.exactness.as_str());
        println!("function: {:?}", envelope.function);
        println!("fingerprint: {}", envelope.fingerprint);
        for warning in &envelope.warnings {
            println!("warning [{}]: {}", warning.code, warning.message);
        }
        for assumption in &envelope.assumptions {
            println!("assumption: {}", assumption.statement);
        }
    }
}

fn replay(
    engine: &Engine,
    receipt: &Receipt,
) -> Result<(String, bicmath_core::envelope::ResultEnvelope), CliError> {
    let request = &receipt.request;
    if request.get("nodes").is_some() {
        let request: BatchRequest = serde_json::from_value(request.clone())
            .map_err(|error| CliError::Configuration(format!("invalid batch receipt: {error}")))?;
        let envelope = engine.batch(&request, &engine.base_context())?;
        let fingerprint = envelope.fingerprint.clone();
        let first = first_batch_envelope(engine, envelope)?;
        return Ok((fingerprint, first));
    }
    if let Some(function) = request.get("function").and_then(|value| value.as_str()) {
        let mut call = serde_json::Map::new();
        call.insert(
            "function".to_string(),
            serde_json::Value::String(function.to_string()),
        );
        call.insert(
            "arguments".to_string(),
            request
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default())),
        );
        call.insert("receipt".to_string(), serde_json::Value::Bool(true));
        let request: CallRequest = serde_json::from_value(serde_json::Value::Object(call))
            .map_err(|error| {
                CliError::Configuration(format!("invalid calculate receipt: {error}"))
            })?;
        let envelope = engine.call(&request, &engine.base_context())?;
        return Ok((envelope.fingerprint.clone(), envelope));
    }
    if let Some(expression) = request.get("expression") {
        let mut evaluate = serde_json::Map::new();
        evaluate.insert("expression".to_string(), expression.clone());
        if let Some(bindings) = request.get("bindings") {
            evaluate.insert("bindings".to_string(), bindings.clone());
        }
        evaluate.insert("receipt".to_string(), serde_json::Value::Bool(true));
        let request: EvaluateRequest = serde_json::from_value(serde_json::Value::Object(evaluate))
            .map_err(|error| {
                CliError::Configuration(format!("invalid evaluate receipt: {error}"))
            })?;
        let envelope = engine.evaluate(&request, &engine.base_context())?;
        return Ok((envelope.fingerprint.clone(), envelope));
    }
    Err(CliError::Configuration(
        "receipt request is not a calculate, evaluate, or batch request".to_string(),
    ))
}

fn first_batch_envelope(
    _engine: &Engine,
    batch: bicmath_engine::BatchEnvelope,
) -> Result<bicmath_core::envelope::ResultEnvelope, CliError> {
    batch
        .outputs
        .iter()
        .find_map(|node| node.result.clone())
        .ok_or_else(|| {
            CliError::Calculation(EngineError::new(
                ErrorCode::BatchDependencyFailed,
                "batch receipt produced no successful node",
            ))
        })
}

fn doctor(engine: &Engine, format: OutputFormat) -> Result<(), CliError> {
    let failures = engine.validate_examples();
    let modules = engine.list_modules();
    let function_count = engine.list_functions(&FunctionFilter::default()).total;
    let disabled: Vec<String> = modules
        .iter()
        .filter(|module| !module.enabled)
        .map(|module| module.id.clone())
        .collect();
    let report = serde_json::json!({
        "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
        "engine": engine.registry().engine_info(),
        "modules": modules.len(),
        "enabled_modules": modules.iter().filter(|module| module.enabled).count(),
        "disabled_modules": disabled,
        "functions": function_count,
        "examples_checked": true,
        "example_failures": failures,
        "ok": failures.is_empty(),
    });
    if format == OutputFormat::Json {
        print_json(&report);
    } else if failures.is_empty() {
        println!(
            "ok: {} modules, {} functions, all documented examples execute",
            modules.len(),
            function_count
        );
    } else {
        println!("doctor found {} example failure(s):", failures.len());
        for failure in &failures {
            println!("- {failure}");
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(CliError::Calculation(EngineError::internal(
            "one or more documented examples failed",
        )))
    }
}
