//! MCP server over stdio: `stepshots mcp` exposes the record / verify /
//! upload workflow as tools for AI agents (Claude Code, Cursor, etc.).
//!
//! stdout carries the JSON-RPC stream, so every tool goes through the
//! print-free internals (`record_tutorial`, `verify::collect`,
//! `upload::upload_one`) — never the printing `run()` wrappers.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::ServiceExt;
use rmcp::handler::server::ServerHandler;
use rmcp::handler::server::tool::{ToolCallContext, ToolRouter};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    InitializeRequestParams, InitializeResult, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use manifest::{RecordingTarget, resolve_viewport};

use crate::auth;
use crate::config::{self, StepshotsConfig};
use crate::error::CliError;
use crate::output::{
    ErrorOutput, ListOutput, ListTutorial, TutorialOutput, UploadOutput, UploadedDemo,
};

use super::record;
use super::schema;
use super::upload;
use super::verify::{self, FailOn};

/// Serve the MCP server on stdin/stdout until the client disconnects.
pub async fn run(default_config: Option<PathBuf>) -> Result<(), CliError> {
    let service = StepshotsMcp::new(default_config)
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| CliError::Other(format!("MCP server failed to initialize: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| CliError::Other(format!("MCP server task failed: {e}")))?;
    Ok(())
}

fn to_mcp_error(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// Serialize a value as pretty JSON into a successful tool result.
fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_string_pretty(value).map_err(to_mcp_error)?;
    Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
}

// ============================================================================
// Tool input types
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTutorialsInput {
    /// Path to stepshots.config.json (default: auto-detect from the working directory)
    #[serde(default)]
    pub config: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordInput {
    /// Tutorial keys to record; records every tutorial in the config when omitted
    #[serde(default)]
    pub tutorials: Vec<String>,
    /// Output directory for .stepshot bundles (default: "output")
    #[serde(default)]
    pub output_dir: Option<String>,
    /// Persistent browser profile directory for authenticated recordings
    #[serde(default)]
    pub profile_dir: Option<String>,
    /// Path to stepshots.config.json (default: auto-detect from the working directory)
    #[serde(default)]
    pub config: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VerifyInput {
    /// Tutorial keys to verify; verifies every tutorial when omitted
    #[serde(default)]
    pub tutorials: Vec<String>,
    /// Directory for failure screenshots (default: "output")
    #[serde(default)]
    pub save_failures: Option<String>,
    /// Failure threshold: "fail" (broken steps only, default) or "warn" (annotation drift also fails)
    #[serde(default)]
    pub fail_on: Option<String>,
    /// Persistent browser profile directory for authenticated flows
    #[serde(default)]
    pub profile_dir: Option<String>,
    /// Path to stepshots.config.json (default: auto-detect from the working directory)
    #[serde(default)]
    pub config: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UploadInput {
    /// Paths of .stepshot bundles to upload
    pub files: Vec<String>,
    /// Demo title override (default: derived from the bundle or filename)
    #[serde(default)]
    pub title: Option<String>,
    /// Replace this existing demo in place instead of creating a new one
    #[serde(default)]
    pub demo_id: Option<String>,
    /// Make newly created demos publicly viewable immediately (default: false)
    #[serde(default)]
    pub public: Option<bool>,
    /// Stepshots server URL (default: $STEPSHOTS_SERVER or https://stepshots.com)
    #[serde(default)]
    pub server: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSchemaInput {
    /// Which schema: "config" for stepshots.config.json (default) or "tour" for *.tour.json
    #[serde(default)]
    pub kind: Option<String>,
}

/// JSON shape returned by the `record` tool — the CLI's `record --json`
/// output, with every failed tutorial reported instead of only the first.
#[derive(Serialize)]
struct McpRecordOutput {
    success: bool,
    command: &'static str,
    tutorials: Vec<TutorialOutput>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failures: Vec<ErrorOutput>,
}

// ============================================================================
// Server
// ============================================================================

pub struct StepshotsMcp {
    /// `--config` passed to `stepshots mcp`; per-tool `config` params override it.
    default_config: Option<PathBuf>,
    tool_router: ToolRouter<Self>,
}

impl StepshotsMcp {
    pub fn new(default_config: Option<PathBuf>) -> Self {
        Self {
            default_config,
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve and load the config: explicit tool param > server `--config` > auto-detect.
    fn load(&self, override_path: Option<&str>) -> Result<(StepshotsConfig, PathBuf), McpError> {
        let flag = override_path
            .map(PathBuf::from)
            .or_else(|| self.default_config.clone());
        let path = config::find_config(flag.as_deref()).map_err(to_mcp_error)?;
        let cfg = config::load_config(&path).map_err(to_mcp_error)?;
        Ok((cfg, path))
    }
}

#[tool_router]
impl StepshotsMcp {
    #[tool(
        description = "List the tutorials defined in stepshots.config.json: key, title, description, and step count. Start here to see what can be recorded or verified."
    )]
    pub async fn list_tutorials(
        &self,
        params: Parameters<ListTutorialsInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let (cfg, path) = self.load(input.config.as_deref())?;

        let mut keys: Vec<&String> = cfg.tutorials.keys().collect();
        keys.sort();
        let tutorials: Vec<ListTutorial> = keys
            .iter()
            .map(|key| {
                let t = &cfg.tutorials[*key];
                ListTutorial {
                    key: (*key).clone(),
                    title: t.title.clone(),
                    description: t.description.clone(),
                    steps: t.steps.len(),
                }
            })
            .collect();

        json_result(&ListOutput {
            success: true,
            command: "list",
            config: path.display().to_string(),
            tutorials,
        })
    }

    #[tool(
        description = "Record tutorials from stepshots.config.json into .stepshot bundles by driving a headless browser. Records every tutorial when 'tutorials' is omitted. Requires Chrome/Chromium and a reachable app at the config's baseUrl. Returns per-tutorial results with bundle paths; failed tutorials are reported in 'failures' with the failing step."
    )]
    pub async fn record(
        &self,
        params: Parameters<RecordInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let (cfg, _path) = self.load(input.config.as_deref())?;
        let selected = config::select_tutorials(&cfg, &input.tutorials).map_err(to_mcp_error)?;
        let viewport = resolve_viewport(cfg.format.as_ref(), &cfg.viewport);

        let output_dir = PathBuf::from(input.output_dir.as_deref().unwrap_or("output"));
        std::fs::create_dir_all(&output_dir).map_err(to_mcp_error)?;
        let profile_dir = input.profile_dir.map(PathBuf::from);

        let mut tutorials_out = Vec::new();
        let mut failures = Vec::new();
        for (key, tutorial) in &selected {
            let output_path = output_dir.join(format!("{key}.stepshot"));
            match record::record_tutorial(
                &cfg,
                tutorial,
                &viewport,
                &output_path,
                true,
                crate::browser::SessionSource {
                    profile_dir: profile_dir.as_deref(),
                    storage_state: None,
                },
            )
            .await
            {
                Ok((step_results, None)) => {
                    if tutorial.target == Some(RecordingTarget::Tour)
                        && let Err(e) = super::tour::post_record(key, &output_path, true)
                    {
                        eprintln!("tour warning: could not scaffold tour file: {e}");
                    }
                    tutorials_out.push(TutorialOutput {
                        key: key.to_string(),
                        title: tutorial.title.clone(),
                        output: Some(output_path.display().to_string()),
                        steps_total: tutorial.steps.len(),
                        steps_completed: Some(step_results.len()),
                        steps: Some(step_results),
                    });
                }
                Ok((step_results, Some(failure))) => {
                    failures.push(ErrorOutput {
                        category: failure.category.to_string(),
                        message: failure.message.clone(),
                        step_index: Some(failure.step_index),
                        tutorial: Some(key.to_string()),
                    });
                    tutorials_out.push(TutorialOutput {
                        key: key.to_string(),
                        title: tutorial.title.clone(),
                        output: None,
                        steps_total: tutorial.steps.len(),
                        steps_completed: Some(step_results.len()),
                        steps: Some(step_results),
                    });
                }
                Err(e) => {
                    failures.push(ErrorOutput {
                        category: e.error_category().to_string(),
                        message: e.to_string(),
                        step_index: None,
                        tutorial: Some(key.to_string()),
                    });
                }
            }
        }

        json_result(&McpRecordOutput {
            success: failures.is_empty(),
            command: "record",
            tutorials: tutorials_out,
            failures,
        })
    }

    #[tool(
        description = "Verify that tutorials still replay against the live app without writing bundles. Returns a per-step drift report — orphaned selectors, failed navigations, annotation drift — with a repair hint per failure. Run this after UI changes to find broken demos."
    )]
    pub async fn verify(
        &self,
        params: Parameters<VerifyInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let (cfg, config_path) = self.load(input.config.as_deref())?;

        let fail_on = match input.fail_on.as_deref() {
            None | Some("fail") => FailOn::Fail,
            Some("warn") => FailOn::Warn,
            Some(other) => {
                return Err(McpError::invalid_params(
                    format!("fail_on must be \"fail\" or \"warn\", got \"{other}\""),
                    None,
                ));
            }
        };
        let save_failures = PathBuf::from(input.save_failures.as_deref().unwrap_or("output"));
        let profile_dir = input.profile_dir.map(PathBuf::from);

        let report = verify::collect(
            &cfg,
            &config_path,
            &input.tutorials,
            &save_failures,
            fail_on,
            profile_dir.as_deref(),
        )
        .await
        .map_err(to_mcp_error)?;

        json_result(&report)
    }

    #[tool(
        description = "Upload .stepshot bundles to the Stepshots dashboard (or replace an existing demo in place via demo_id, keeping its URL and embeds). Requires a stored login (`stepshots login`) or STEPSHOTS_TOKEN. Returns demo IDs and dashboard URLs."
    )]
    pub async fn upload(
        &self,
        params: Parameters<UploadInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        if input.files.is_empty() {
            return Err(McpError::invalid_params("files must not be empty", None));
        }

        let server = input
            .server
            .or_else(|| std::env::var("STEPSHOTS_SERVER").ok())
            .unwrap_or_else(|| "https://stepshots.com".to_string());
        let token = std::env::var("STEPSHOTS_TOKEN")
            .ok()
            .or_else(|| auth::stored_token_for(&server))
            .ok_or_else(|| {
                McpError::invalid_request(
                    "No API token. Run `stepshots login` in a terminal first, or set STEPSHOTS_TOKEN.",
                    None,
                )
            })?;

        let client = reqwest::Client::new();
        let mut demos = Vec::new();
        for file in &input.files {
            let result = upload::upload_one(
                &client,
                file,
                input.title.as_deref(),
                input.demo_id.as_deref(),
                input.public.unwrap_or(false),
                true,
                &server,
                &token,
            )
            .await
            .map_err(to_mcp_error)?;
            demos.push(UploadedDemo {
                demo_id: result.demo_id,
                view_url: result.view_url,
                replaced: input.demo_id.is_some(),
            });
        }

        json_result(&UploadOutput {
            success: true,
            command: "upload",
            demos,
        })
    }

    #[tool(
        description = "Get the JSON Schema for stepshots.config.json (kind=\"config\", default) or for *.tour.json guided-tour files (kind=\"tour\") — use it to write or validate configs before recording."
    )]
    pub async fn get_schema(
        &self,
        params: Parameters<GetSchemaInput>,
    ) -> Result<CallToolResult, McpError> {
        let schema_json = match params.0.kind.as_deref() {
            None | Some("config") => schema::generate().map_err(to_mcp_error)?,
            Some("tour") => schema::generate_tour().map_err(to_mcp_error)?,
            Some(other) => {
                return Err(McpError::invalid_params(
                    format!("kind must be \"config\" or \"tour\", got \"{other}\""),
                    None,
                ));
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(
            schema_json,
        )]))
    }
}

#[allow(clippy::manual_async_fn)]
impl ServerHandler for StepshotsMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("stepshots", env!("CARGO_PKG_VERSION"))
                    .with_title("Stepshots MCP Server")
                    .with_website_url("https://stepshots.com"),
            )
            .with_instructions(
                "Stepshots — record, verify, and publish interactive product demos from \
                 stepshots.config.json in the working directory. Workflow: get_schema to write \
                 or edit the config, list_tutorials to see what's defined, record to produce \
                 .stepshot bundles headlessly, verify to replay demos against the live app and \
                 get a drift report with repair hints (run it after UI changes), upload to \
                 publish bundles to the dashboard (pass demo_id to update an existing demo \
                 without changing its URL or embeds). Uploading requires `stepshots login` \
                 once, or STEPSHOTS_TOKEN.",
            )
    }

    fn initialize(
        &self,
        _request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<InitializeResult, McpError>> + Send + '_ {
        async { Ok(self.get_info()) }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async {
            let tools: Vec<Tool> = self.tool_router.list_all();
            Ok(ListToolsResult::with_all_items(tools))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResponse, McpError>> + Send + '_ {
        let tool_context = ToolCallContext::new(self, request, context);
        self.tool_router.call(tool_context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks in that the rmcp `#[tool_router]` macro still registers every
    /// `#[tool]` method after a crate upgrade — a silently shrunken router
    /// would otherwise only surface as missing tools in connected clients.
    #[test]
    fn tool_router_registers_all_tools() {
        let tools = StepshotsMcp::tool_router().list_all();
        assert_eq!(tools.len(), 5);
        for tool in &tools {
            assert!(!tool.name.is_empty());
            assert!(
                !tool.input_schema.is_empty(),
                "tool {} lost its input schema",
                tool.name
            );
        }
    }
}
