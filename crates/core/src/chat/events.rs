//! A stdout line of the CLI, turned into typed events (and the JSON that goes back on stdin).

use serde_json::Value;

use super::*;

// ---------- Events ----------

/// A block of a complete assistant message.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    Thinking(String),
    ToolUse { id: String, name: String, input: Value },
}

/// The outcome of a tool call, from a `tool_result` block.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub tool_use_id: String,
    pub text: String,
    pub is_error: bool,
    /// What an edit changed, with the line numbers (`structuredPatch` of the result); empty for other tools.
    pub patch: Vec<PatchHunk>,
}

/// One hunk of an edit: `lines` start with ` ` (unchanged), `-` or `+`, like a unified diff.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<String>,
}

/// The `tool_result` blocks of a user line. `extra` is the line's `tool_use_result` (`toolUseResult` in the CLI's file): it
/// belongs to the call when the line has a single result.
pub(crate) fn tool_results(content: &Value, extra: &Value) -> Vec<ToolOutcome> {
    let mut results: Vec<ToolOutcome> = content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| str_of(b, "type") == "tool_result")
        .map(|b| ToolOutcome {
            tool_use_id: str_of(b, "tool_use_id").to_owned(),
            text: result_text(b.get("content").unwrap_or(&Value::Null)),
            is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            patch: Vec::new(),
        })
        .collect();
    if let [only] = results.as_mut_slice() {
        only.patch = patch_of(extra);
    }
    results
}

pub(crate) fn patch_of(result: &Value) -> Vec<PatchHunk> {
    let num = |h: &Value, k: &str| h.get(k).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).unwrap_or(1);
    result
        .get("structuredPatch")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|h| PatchHunk {
            old_start: num(h, "oldStart"),
            new_start: num(h, "newStart"),
            // "\\ No newline at end of file" is not a line of the file.
            lines: strings(h.get("lines")).into_iter().filter(|l| !l.starts_with('\\')).collect(),
        })
        .collect()
}

/// The CLI asks whether a tool may run (`control_request` / `can_use_tool`).
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    pub request_id: String,
    pub tool_use_id: String,
    pub tool: String,
    pub input: Value,
    pub description: String,
    /// What "allow for this session" would add (`permission_suggestions` of the request); empty = the CLI offers none.
    pub suggestions: Vec<Value>,
}

/// End of a turn (`result`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TurnResult {
    pub is_error: bool,
    /// The error text, when `is_error`.
    pub error: String,
    pub cost_usd: f64,
    pub duration_ms: u64,
    pub session: String,
    /// The model's context window in tokens (`modelUsage`), 0 = unknown.
    pub context_window: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// `system` / `init`, sent at the start of every turn.
    Init {
        session: String,
        model: String,
        commands: Vec<String>,
        terminal_commands: Vec<String>,
    },
    /// `system` / `status` `requesting`: the CLI is calling the model.
    Requesting,
    TextDelta {
        message: String,
        text: String,
    },
    /// A complete assistant message (or the part of one that just finished).
    Assistant {
        message: String,
        blocks: Vec<Block>,
        context_tokens: u64,
    },
    ToolResults(Vec<ToolOutcome>),
    Permission(PermissionRequest),
    /// The CLI withdrew a permission request (`control_cancel_request`).
    PermissionCancelled(String),
    /// The turn was cut by an interrupt.
    Interrupted,
    Result(TurnResult),
    /// A control request this module does not know: it has to be answered, or the CLI waits for it.
    UnknownRequest {
        request_id: String,
    },
    /// Something the backend says in the conversation (a resume that fell back to a new conversation, a warning).
    Notice {
        text: String,
        error: bool,
    },
    /// What the CLI saved of the conversation it just resumed (a backend that reads it from the process itself, not
    /// from a file: Codex's `thread/resume`). Applied only where the chat asked for it ([`Transcript::history_pending`]).
    Replay(Vec<Item>),
    /// The plan's usage (Claude's `rate_limit_event`): it goes to the usage bar, not to the conversation.
    Usage(crate::usage::AgentUsage),
    /// A line with nothing to show (hooks, partial tool input...).
    Ignored,
}

pub(crate) fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

pub(crate) fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

/// The text of a `tool_result` content: a string, or a list of `{type:"text", text}` blocks.
pub(crate) fn result_text(content: &Value) -> String {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p.get("type").and_then(Value::as_str) {
                Some("text") => str_of(p, "text").to_owned(),
                Some("image") => "[image]".to_owned(),
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    clip(&text, RESULT_MAX_CHARS)
}

pub(crate) fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}\n… ({} more characters)", &text[..at], text[at..].chars().count()),
        None => text.to_owned(),
    }
}

/// Tokens the conversation takes in the model's context, from the `usage` of an assistant message.
pub(crate) fn context_tokens(usage: &Value) -> u64 {
    ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
        .iter()
        .filter_map(|k| usage.get(k).and_then(Value::as_u64))
        .sum()
}

pub(crate) fn blocks_of(content: &Value) -> Vec<Block> {
    let Some(list) = content.as_array() else { return Vec::new() };
    list.iter()
        .filter_map(|b| match b.get("type").and_then(Value::as_str)? {
            "text" => Some(Block::Text(str_of(b, "text").to_owned())),
            "thinking" => Some(Block::Thinking(str_of(b, "thinking").to_owned())),
            "tool_use" => Some(Block::ToolUse {
                id: str_of(b, "id").to_owned(),
                name: str_of(b, "name").to_owned(),
                input: b.get("input").cloned().unwrap_or(Value::Null),
            }),
            _ => None,
        })
        .collect()
}

/// Turns one line of the stdout of Claude Code or Antigravity into events (the shape tells which: Antigravity's has a
/// top-level `event` and no `type`). A line that is not JSON (a stray warning) yields nothing. Each backend's
/// [`Protocol::parse`] is the general entry point; this one is kept for the recordings of the tests.
pub fn parse_line(line: &str) -> Vec<ChatEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return Vec::new() };
    if v.get("type").is_none() && v.get("event").is_some() {
        return parse_agy_line(&v);
    }
    parse_claude_line(&v)
}

/// One line of Claude Code's stream.
pub(crate) fn parse_claude_line(v: &Value) -> Vec<ChatEvent> {
    let event = match str_of(v, "type") {
        "system" => match str_of(v, "subtype") {
            "init" => ChatEvent::Init {
                session: str_of(v, "session_id").to_owned(),
                model: str_of(v, "model").to_owned(),
                commands: strings(v.get("slash_commands")),
                terminal_commands: strings(v.get("terminal_slash_commands")),
            },
            "status" if str_of(v, "status") == "requesting" => ChatEvent::Requesting,
            _ => ChatEvent::Ignored,
        },
        "rate_limit_event" => {
            let usage = crate::usage::from_claude_rate_limits(v.get("rate_limit_info").unwrap_or(&Value::Null));
            if usage.session.is_some() || usage.weekly.is_some() {
                ChatEvent::Usage(usage)
            } else {
                ChatEvent::Ignored
            }
        }
        "stream_event" => {
            let ev = v.get("event").unwrap_or(&Value::Null);
            let delta = ev.get("delta").unwrap_or(&Value::Null);
            // A sub-agent's stream is not the conversation's.
            if !v.get("parent_tool_use_id").is_none_or(Value::is_null) {
                return Vec::new();
            }
            if str_of(ev, "type") == "content_block_delta" && str_of(delta, "type") == "text_delta" {
                ChatEvent::TextDelta { message: str_of(v, "api_message_id").to_owned(), text: str_of(delta, "text").to_owned() }
            } else {
                ChatEvent::Ignored
            }
        }
        "assistant" => {
            if !v.get("parent_tool_use_id").is_none_or(Value::is_null) {
                return Vec::new();
            }
            let msg = v.get("message").unwrap_or(&Value::Null);
            ChatEvent::Assistant {
                message: str_of(msg, "id").to_owned(),
                blocks: blocks_of(msg.get("content").unwrap_or(&Value::Null)),
                context_tokens: context_tokens(msg.get("usage").unwrap_or(&Value::Null)),
            }
        }
        "user" => {
            if !v.get("parent_tool_use_id").is_none_or(Value::is_null) {
                return Vec::new();
            }
            let content = v.get("message").and_then(|m| m.get("content")).unwrap_or(&Value::Null);
            let results = tool_results(content, v.get("tool_use_result").unwrap_or(&Value::Null));
            if !results.is_empty() {
                ChatEvent::ToolResults(results)
            } else if user_text(content).starts_with(INTERRUPTED) {
                ChatEvent::Interrupted
            } else {
                ChatEvent::Ignored
            }
        }
        "control_request" => {
            let req = v.get("request").unwrap_or(&Value::Null);
            let request_id = str_of(v, "request_id").to_owned();
            if str_of(req, "subtype") == "can_use_tool" {
                ChatEvent::Permission(PermissionRequest {
                    request_id,
                    tool_use_id: str_of(req, "tool_use_id").to_owned(),
                    tool: str_of(req, "tool_name").to_owned(),
                    input: req.get("input").cloned().unwrap_or(Value::Null),
                    description: str_of(req, "description").to_owned(),
                    suggestions: req.get("permission_suggestions").and_then(Value::as_array).cloned().unwrap_or_default(),
                })
            } else {
                ChatEvent::UnknownRequest { request_id }
            }
        }
        "control_cancel_request" => ChatEvent::PermissionCancelled(str_of(v, "request_id").to_owned()),
        "result" => {
            let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            ChatEvent::Result(TurnResult {
                is_error,
                error: if is_error { str_of(v, "result").to_owned() } else { String::new() },
                cost_usd: v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
                duration_ms: v.get("duration_ms").and_then(Value::as_u64).unwrap_or(0),
                session: str_of(v, "session_id").to_owned(),
                context_window: v
                    .get("modelUsage")
                    .and_then(Value::as_object)
                    .and_then(|m| m.values().find_map(|u| u.get("contextWindow").and_then(Value::as_u64)))
                    .unwrap_or(0),
            })
        }
        _ => ChatEvent::Ignored,
    };
    if event == ChatEvent::Ignored {
        Vec::new()
    } else {
        vec![event]
    }
}

pub(crate) fn parse_agy_line(v: &Value) -> Vec<ChatEvent> {
    match str_of(v, "event") {
        "init" => {
            let session = str_of(v, "conversation_id").to_owned();
            vec![ChatEvent::Init { session, model: "antigravity".to_owned(), commands: Vec::new(), terminal_commands: Vec::new() }]
        }
        "step_update" => {
            let Some(step) = v.get("step_update") else { return Vec::new() };
            let step_index = step.get("step_index").and_then(Value::as_u64).unwrap_or(0);
            let step_type = str_of(step, "step_type");
            let state = str_of(step, "state");
            match step_type {
                "agent_response" => {
                    let text_delta = str_of(step, "text_delta");
                    let mut evs = Vec::new();
                    if !text_delta.is_empty() {
                        evs.push(ChatEvent::TextDelta { message: "agy_resp".to_owned(), text: text_delta.to_owned() });
                    }
                    if state == "DONE" {
                        let usage = step.get("usage");
                        let context_tokens = usage.map_or(0, |u| {
                            u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0)
                                + u.get("cache_read_tokens").and_then(Value::as_u64).unwrap_or(0)
                        });
                        evs.push(ChatEvent::Assistant { message: "agy_resp".to_owned(), blocks: Vec::new(), context_tokens });
                    }
                    evs
                }
                "tool" => {
                    let tool_name = str_of(step, "tool_name");
                    let tool_info = step.get("tool_info").unwrap_or(&Value::Null);
                    let tool_id = format!("step_{step_index}");
                    if state == "ACTIVE" {
                        let params = tool_info.get("parameters").cloned().unwrap_or(Value::Null);
                        vec![ChatEvent::Assistant {
                            message: format!("call_{step_index}"),
                            blocks: vec![Block::ToolUse { id: tool_id, name: tool_name.to_owned(), input: params }],
                            context_tokens: 0,
                        }]
                    } else if state == "DONE" {
                        let output = tool_info.get("output").and_then(Value::as_str).unwrap_or("");
                        let params = tool_info.get("parameters");
                        let patch = agy_patch_of(params);
                        vec![ChatEvent::ToolResults(vec![ToolOutcome {
                            tool_use_id: tool_id,
                            text: output.to_owned(),
                            is_error: false,
                            patch,
                        }])]
                    } else if state == "ERROR" {
                        let err_msg = tool_info.get("error").and_then(|e| e.get("message")).and_then(Value::as_str).unwrap_or("tool error");
                        vec![ChatEvent::ToolResults(vec![ToolOutcome {
                            tool_use_id: tool_id,
                            text: err_msg.to_owned(),
                            is_error: true,
                            patch: Vec::new(),
                        }])]
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            }
        }
        "result" => {
            let Some(res) = v.get("result") else { return Vec::new() };
            let status = str_of(res, "status");
            let is_error = status != "SUCCESS";
            let error = if is_error { str_of(res, "error").to_owned() } else { String::new() };
            let session = str_of(res, "conversation_id").to_owned();
            let duration_ms = (res.get("duration_seconds").and_then(Value::as_f64).unwrap_or(0.0) * 1000.0) as u64;
            let response = str_of(res, "response");
            let usage = res.get("usage");
            let context_tokens = usage.map_or(0, |u| {
                u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0) + u.get("cache_read_tokens").and_then(Value::as_u64).unwrap_or(0)
            });
            if is_error && error == "interrupted" {
                return vec![
                    ChatEvent::Interrupted,
                    ChatEvent::Result(TurnResult {
                        is_error: false,
                        error: String::new(),
                        cost_usd: 0.0,
                        duration_ms,
                        session,
                        context_window: 0,
                    }),
                ];
            }
            let mut evs = Vec::new();
            if !response.is_empty() {
                evs.push(ChatEvent::Assistant {
                    message: "agy_resp".to_owned(),
                    blocks: vec![Block::Text(response.to_owned())],
                    context_tokens,
                });
            }
            evs.push(ChatEvent::Result(TurnResult { is_error, error, cost_usd: 0.0, duration_ms, session, context_window: 0 }));
            evs
        }
        _ => Vec::new(),
    }
}

pub(crate) fn agy_unquote_str(s: &str) -> String {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        if let Ok(v) = serde_json::from_str::<String>(s) {
            return v;
        }
        return s[1..s.len() - 1].to_owned();
    }
    s.to_owned()
}

pub(crate) fn agy_patch_of(params: Option<&Value>) -> Vec<PatchHunk> {
    let Some(params) = params else { return Vec::new() };
    let target = params.get("Target").or_else(|| params.get("target")).and_then(Value::as_str);
    let replacement = params.get("Replacement").or_else(|| params.get("replacement")).and_then(Value::as_str);
    match (target, replacement) {
        (Some(t), Some(r)) => {
            let t = agy_unquote_str(t);
            let r = agy_unquote_str(r);
            let mut lines = Vec::new();
            for l in t.lines() {
                lines.push(format!("-{l}"));
            }
            for l in r.lines() {
                lines.push(format!("+{l}"));
            }
            vec![PatchHunk { old_start: 1, new_start: 1, lines }]
        }
        _ => Vec::new(),
    }
}

/// The text of a user message's content (a string, or the text blocks of a list).
pub(crate) fn user_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => {
            parts.iter().filter(|p| str_of(p, "type") == "text").map(|p| str_of(p, "text")).collect::<Vec<_>>().join("\n")
        }
        _ => String::new(),
    }
}
