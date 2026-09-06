//! One ephemeral, text-only Codex app-server turn over stdio (CLI 0.153.4).
//! The exec codec remains independent. JSON-RPC is deliberately private to
//! this provider: one ordered handshake, one turn, no reusable RPC layer.

use jinn_engine::{Event, Usage};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_LINE: usize = 512 * 1024;
const DENIED_FEATURES: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "code_mode",
    "code_mode_host",
    "code_mode_only",
    "apps",
    "plugins",
    "hooks",
    "multi_agent",
    "image_generation",
    "view_image",
    "browser_use",
    "computer_use",
    "goals",
    "sleep_tool",
    "skill_search",
    "skill_mcp_dependency_install",
    "tool_suggest",
    "workspace_dependencies",
    "memory_tool",
    "memories",
    "request_permissions_tool",
    "default_mode_request_user_input",
    "standalone_web_search",
    "shell_snapshot",
];

/// Startup controls are enforced again on the server's effective config.
#[must_use]
pub fn argv() -> Vec<String> {
    let mut args = vec![
        "app-server".into(),
        "--stdio".into(),
        "--strict-config".into(),
    ];
    for value in [
        "approval_policy=\"never\"",
        "sandbox_mode=\"read-only\"",
        "web_search=\"disabled\"",
        "project_doc_max_bytes=0",
        "notify=[]",
        "features.skip_host_skill_discovery=true",
        "tools.update_plan.enabled=false",
    ] {
        args.extend(["-c".into(), value.into()]);
    }
    for feature in DENIED_FEATURES {
        args.extend(["-c".into(), format!("features.{feature}=false")]);
    }
    args
}

pub struct AppServer {
    buffer: Vec<u8>,
    outgoing: Vec<u8>,
    expected: u64,
    model: String,
    effort: String,
    cwd: String,
    prompt: String,
    thread: Option<String>,
    turn: Option<String>,
    messages: BTreeMap<String, String>,
    completed: BTreeMap<String, String>,
    terminal: bool,
    errors: Vec<String>,
    usage: Usage,
}

impl AppServer {
    #[must_use]
    pub fn new(prompt: String, model: String, effort: String, cwd: String) -> Self {
        let mut wire = Self {
            buffer: Vec::new(),
            outgoing: Vec::new(),
            expected: 1,
            model,
            effort,
            cwd,
            prompt,
            thread: None,
            turn: None,
            messages: BTreeMap::new(),
            completed: BTreeMap::new(),
            terminal: false,
            errors: Vec::new(),
            usage: Usage::default(),
        };
        wire.request(
            1,
            "initialize",
            json!({"clientInfo":{"name":"jinn_text_chat","version":"1"},
            "capabilities":{"experimentalApi":true}}),
        );
        wire
    }

    fn request(&mut self, id: u64, method: &str, params: Value) {
        self.expected = id;
        self.write(json!({"id":id,"method":method,"params":params}));
    }

    fn write(&mut self, value: Value) {
        self.outgoing.extend(value.to_string().bytes());
        self.outgoing.push(b'\n');
    }

    fn fail(&mut self, reason: impl Into<String>) {
        if self.errors.is_empty() {
            self.errors.push(reason.into());
        }
        self.terminal = true;
    }

    #[must_use]
    pub fn take_outgoing(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outgoing)
    }
    #[must_use]
    pub fn terminal(&self) -> bool {
        self.terminal
    }
    #[must_use]
    pub fn errors(&self) -> &[String] {
        &self.errors
    }
    #[must_use]
    pub fn usage(&self) -> Usage {
        self.usage.clone()
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        for byte in bytes {
            if *byte == b'\n' {
                let line = std::mem::take(&mut self.buffer);
                match serde_json::from_slice(&line) {
                    Ok(value) => events.extend(self.frame(value)),
                    Err(_) => self.fail("Codex sent malformed protocol data"),
                }
            } else if self.buffer.len() < MAX_LINE {
                self.buffer.push(*byte);
            } else {
                self.fail("Codex protocol frame exceeded its byte limit");
                self.buffer.clear();
                break;
            }
        }
        events
    }

    pub fn flush(&mut self) -> Vec<Event> {
        if !self.buffer.is_empty() || !self.terminal {
            self.fail("Codex disconnected without a complete terminal response");
        }
        Vec::new()
    }

    fn frame(&mut self, value: Value) -> Vec<Event> {
        if self.terminal {
            return Vec::new();
        }
        if value.get("id").is_some() && value.get("method").is_some() {
            self.write(json!({"id":value["id"],"error":{"code":-32601,
                "message":"Text Chat does not permit tools or approval requests"}}));
            self.fail("Codex requested a tool or approval unavailable in text Chat");
            return Vec::new();
        }
        if let Some(id) = value.get("id") {
            if id.as_u64() != Some(self.expected) {
                self.fail("Codex returned an unexpected response id");
            } else if value.get("error").is_some() {
                self.fail(format!(
                    "Codex refused the request: {}",
                    value["error"]["message"]
                ));
            } else if let Some(result) = value.get("result") {
                self.response(result);
            } else {
                self.fail("Codex returned a response without a result");
            }
            return Vec::new();
        }
        let Some(method) = value["method"].as_str() else {
            self.fail("Codex sent a frame without a method");
            return Vec::new();
        };
        let params = &value["params"];
        if method == "error" {
            self.fail(format!("Codex failed: {}", params["error"]["message"]));
            return Vec::new();
        }
        if let Some(thread) = params["threadId"].as_str() {
            if self.thread.as_deref() != Some(thread) {
                self.fail("Codex changed thread identity");
            }
        }
        if let Some(turn) = params["turnId"].as_str() {
            if let Some(expected) = &self.turn {
                if expected != turn {
                    self.fail("Codex changed turn identity");
                }
            }
        }
        if self.terminal {
            return Vec::new();
        }
        match method {
            "item/agentMessage/delta" => {
                let (Some(id), Some(delta)) = (params["itemId"].as_str(), params["delta"].as_str())
                else {
                    self.fail("Codex sent an invalid text delta");
                    return Vec::new();
                };
                if self.completed.contains_key(id) {
                    self.fail("Codex sent text after message completion");
                } else if self.messages.len() >= 128 && !self.messages.contains_key(id)
                    || self.messages.values().map(String::len).sum::<usize>() + delta.len()
                        > MAX_LINE
                {
                    self.fail("Codex text exceeded the protocol memory limit");
                } else {
                    self.messages.entry(id.into()).or_default().push_str(delta);
                    return vec![Event::delta(delta)];
                }
            }
            "item/completed" => {
                let item = &params["item"];
                if item["type"] == "agentMessage" {
                    self.check_message(item);
                } else if item["type"] != "userMessage" && item["type"] != "reasoning" {
                    self.fail("Codex used an unsupported item in text Chat");
                }
            }
            "rawResponseItem/completed" => {
                let kind = params["item"]["type"].as_str().unwrap_or_default();
                if kind.contains("tool") || kind == "function_call" {
                    self.fail("Tools are unavailable in text Chat");
                }
            }
            "thread/tokenUsage/updated" => {
                let usage = &params["tokenUsage"]["last"];
                self.usage.input_tokens = usage["inputTokens"].as_u64().unwrap_or_default();
                self.usage.output_tokens = usage["outputTokens"].as_u64().unwrap_or_default();
            }
            "turn/completed" => {
                let turn = &params["turn"];
                if self.turn.as_deref() != turn["id"].as_str() {
                    self.fail("Codex terminal response changed turn identity");
                } else if turn["status"] != "completed" {
                    self.fail(format!("Codex turn {}: {}", turn["status"], turn["error"]));
                } else if self.messages.is_empty() || self.messages != self.completed {
                    self.fail("Codex ended without confirming the streamed answer");
                } else {
                    self.terminal = true;
                    return vec![Event::turn_end(None)];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn check_message(&mut self, item: &Value) {
        let (Some(id), Some(text)) = (item["id"].as_str(), item["text"].as_str()) else {
            self.fail("Codex completed an invalid message");
            return;
        };
        if self.messages.get(id).map(String::as_str) != Some(text) {
            self.fail("Codex completed text that differs from its streamed answer");
        } else {
            self.completed.insert(id.into(), text.into());
        }
    }

    fn response(&mut self, result: &Value) {
        match self.expected {
            1 => {
                self.write(json!({"method":"initialized","params":{}}));
                self.request(
                    2,
                    "config/read",
                    json!({"includeLayers":false,"cwd":self.cwd}),
                );
            }
            2 => {
                let config = &result["config"];
                let empty = |key: &str| {
                    config[key].is_null()
                        || config[key]
                            .as_object()
                            .is_some_and(serde_json::Map::is_empty)
                };
                let disabled = DENIED_FEATURES.iter().all(|key| {
                    let v = &config["features"][key];
                    v == false || v["enabled"] == false
                });
                if !disabled
                    || !empty("mcp_servers")
                    || !empty("hooks")
                    || !empty("plugins")
                    || config["notify"].as_array().is_none_or(|a| !a.is_empty())
                    || config["project_doc_max_bytes"] != 0
                    || config["web_search"] != "disabled"
                    || config["features"]["skip_host_skill_discovery"] != true
                    || !config["developer_instructions"].is_null()
                    || !config["instructions"].is_null()
                    || !config["model_instructions_file"].is_null()
                {
                    self.fail("Codex configuration is not isolated for text Chat; use a dedicated Codex home");
                } else {
                    self.request(3, "mcpServerStatus/list", json!({}));
                }
            }
            3 => {
                if result["data"]
                    .as_array()
                    .is_none_or(|data| !data.is_empty())
                    || !result["nextCursor"].is_null()
                {
                    self.fail("Text Chat refuses an inherited MCP server");
                } else {
                    self.request(4, "thread/start", json!({"model":self.model,"allowProviderModelFallback":false,
                        "cwd":self.cwd,"approvalPolicy":"never","sandbox":"read-only","environments":[],
                        "dynamicTools":[],"ephemeral":true,"experimentalRawEvents":true,
                        "baseInstructions":"You are a helpful text chat assistant. Answer directly using text or Markdown. Tools are unavailable.",
                        "config":{"model_reasoning_effort":self.effort}}));
                }
            }
            4 => {
                if result["model"] != self.model
                    || result["reasoningEffort"] != self.effort
                    || result["approvalPolicy"] != "never"
                    || result["instructionSources"]
                        .as_array()
                        .is_none_or(|a| !a.is_empty())
                {
                    self.fail("Codex did not confirm the requested model, effort, or isolated instructions");
                } else if let Some(thread) = result["thread"]["id"].as_str() {
                    self.thread = Some(thread.into());
                    self.request(
                        5,
                        "turn/start",
                        json!({"threadId":thread,"model":self.model,"effort":self.effort,
                        "environments":[],"input":[{"type":"text","text":self.prompt}]}),
                    );
                    self.prompt.clear();
                } else {
                    self.fail("Codex did not name its thread");
                }
            }
            5 => {
                if let Some(turn) = result["turn"]["id"].as_str() {
                    self.turn = Some(turn.into());
                } else {
                    self.fail("Codex did not name its turn");
                }
                self.expected = 0;
            }
            _ => self.fail("Codex returned an unexpected handshake response"),
        }
    }
}

#[cfg(test)]
mod tests;
