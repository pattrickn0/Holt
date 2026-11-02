//! The generic chat backend: any CLI that speaks the Agent Client Protocol (ACP, https://agentclientprotocol.com), a
//! JSON-RPC 2.0 protocol over stdio created by Zed.
//!
//! A reader of the terminal screen can never be generic, but ACP is the common layer many agent CLIs speak, natively or
//! through an adapter. One [`AcpBackend`] and one [`AcpProtocol`] serve all of them; what differs is a row of the table
//! [`CLIS`] (the command that starts the ACP mode). `memory/learnings/acp-protocol-traps.md` has what was checked.
//!
//! The dialogue, one long-lived process per tab:
//!
//! ```text
//! Holt                                       agent
//!  initialize {protocolVersion, clientCapabilities}  →
//!                                            ←  {agentCapabilities: {loadSession}, authMethods}
//!  session/new {cwd, mcpServers: []}          →      (or session/load {sessionId, ...} when resuming and the agent
//!                                            ←  {sessionId}   advertises loadSession; the history streams as updates)
//!  session/prompt {sessionId, prompt}         →      one per message, the next waits for the previous to end
//!                                            ←  session/update notifications (message and thought chunks, tool calls)
//!                                            ←  session/request_permission  → Holt answers {outcome}
//!  session/cancel {sessionId}                 →      the interrupt
//!                                            ←  {stopReason}: the end of the turn
//! ```
//!
//! Holt advertises no `fs` and no `terminal` capability: it implements neither, so the agent does its own file and shell
//! work and a request for them is answered with "method not found". How the updates become the common chat events:
//!
//! | ACP update | chat |
//! |---|---|
//! | `agent_message_chunk` | assistant text, streamed; a tool call or a thought ends the message |
//! | `agent_thought_chunk` | a thinking block |
//! | `tool_call` / `tool_call_update` | a call named after its `kind` (`Bash`, `Edit`, `Read`, `Grep`, `WebFetch`...) and its result; a `diff` content becomes the patch hunks |
//! | `session/request_permission` | the permission card: `allow_once` → Allow, `allow_always` → "for this session", `reject_once` → Deny |
//! | `available_commands_update` | the slash commands of the composer |
//! | `usage_update` | the context use |
//! | `plan`, `current_mode_update`, `user_message_chunk` (outside a resume)... | ignored: the view has no screen for them |

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use serde_json::{json, Value};

use super::backend::{Capability, ChatBackend, Interrupt, Parsed, Protocol};
use super::{clip, patch_from_diff, str_of, Block, ChatEvent, Item, PermissionRequest, ToolOutcome, Transcript, TurnResult, RESULT_MAX_CHARS};
use crate::agents::SpawnSpec;
use crate::models::Effort;
use crate::t;

/// How the Holt instructions reach a CLI in ACP mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Instructions {
    /// The CLI has a channel of its own that `agents.rs` already feeds (OpenCode's configuration, the `AGENTS.md` that Goose
    /// and Copilot read).
    Native,
    /// No channel known for this CLI: they go in front of the first prompt of a new conversation
    /// (`<holt-instructions>...</holt-instructions>`, left out of a replayed history).
    FirstPrompt,
}

/// One CLI that speaks ACP: the row of [`CLIS`].
pub(crate) struct AcpBackend {
    /// The agent id (`opencode`, `goose`, `gemini`...).
    pub(crate) kind: &'static str,
    /// Arguments before every other one: the subcommand that starts the ACP mode (`opencode acp`).
    leading: &'static [&'static str],
    /// Arguments that switch the CLI to its ACP mode (`gemini --acp`), after the others.
    mode: &'static [&'static str],
    pub(crate) instructions: Instructions,
}

/// The CLIs with an ACP mode (flags checked against the ACP registry, https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json,
/// and each CLI's documentation on 2026-11-02). A CLI without a row has no chat view unless it has a backend of its own.
pub(crate) static CLIS: [AcpBackend; 5] = [
    AcpBackend { kind: "opencode", leading: &["acp"], mode: &[], instructions: Instructions::Native },
    AcpBackend { kind: "goose", leading: &["acp"], mode: &[], instructions: Instructions::Native },
    // `gemini --acp` (registry); `--experimental-acp` is the old name of the same flag.
    AcpBackend { kind: "gemini", leading: &[], mode: &["--acp"], instructions: Instructions::FirstPrompt },
    // GitHub's docs: `copilot --acp` runs on stdio; `--stdio` is optional, kept so the transport is explicit.
    AcpBackend { kind: "copilot", leading: &[], mode: &["--acp", "--stdio"], instructions: Instructions::Native },
    AcpBackend { kind: "qwen", leading: &[], mode: &["--acp"], instructions: Instructions::FirstPrompt },
];

/// The ACP row of `kind`, if the CLI speaks ACP.
pub(crate) fn backend_for(kind: &str) -> Option<&'static AcpBackend> {
    CLIS.iter().find(|b| b.kind == kind)
}

impl ChatBackend for AcpBackend {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn capability(&self) -> Capability {
        Capability::Chat
    }

    fn leading_args(&self) -> Vec<String> {
        self.leading.iter().map(|s| (*s).to_owned()).collect()
    }

    fn mode_args(&self) -> Vec<String> {
        self.mode.iter().map(|s| (*s).to_owned()).collect()
    }

    /// The model is not a launch option here: an ACP agent says which models it has in `session/new` and the view shows the
    /// one in use. (OpenCode gets the tab's model through its configuration, see `agents.rs`.)
    fn model_args(&self, _model: &str) -> Vec<String> {
        Vec::new()
    }

    fn effort_args(&self, _effort: Effort) -> Vec<String> {
        Vec::new()
    }

    /// `session/load` resumes the conversation, not an argument.
    fn resumes_by_args(&self) -> bool {
        false
    }

    fn prompt_instructions(&self) -> bool {
        self.instructions == Instructions::FirstPrompt
    }

    fn protocol(&self, spec: &SpawnSpec) -> Box<dyn Protocol> {
        Box::new(AcpProtocol::new(spec.session.clone(), &spec.cwd, spec.instructions.clone()))
    }

    /// The agent replays the conversation itself (`session/load`): nothing to read here but that the tab has one, so the chat
    /// waits for it instead of showing an empty view.
    fn history(&self, _home: &Path, _session: &str) -> Option<Transcript> {
        Some(Transcript { history_pending: true, ..Transcript::default() })
    }
}

const INSTRUCTIONS_OPEN: &str = "<holt-instructions>";
const INSTRUCTIONS_CLOSE: &str = "</holt-instructions>";

/// What a request Holt sent is waiting for.
#[derive(Debug, Clone, Copy)]
enum Awaiting {
    Initialize,
    /// `session/new`, or `session/load` (`true`).
    Session(bool),
    Prompt,
}

/// The options of a `session/request_permission` by kind, and the id of the request to answer.
struct Permission {
    rpc_id: Value,
    allow_once: Option<String>,
    allow_always: Option<String>,
    reject_once: Option<String>,
    reject_always: Option<String>,
}

/// One diff of a tool call (`{type: "diff", path, oldText, newText}`).
#[derive(Debug, Clone, PartialEq)]
struct Diff {
    path: String,
    /// `None` = a new file.
    old: Option<String>,
    new: String,
}

/// What `tool_call` and its updates said about one call, merged: every update may carry only what changed.
#[derive(Debug, Clone, Default)]
struct Tool {
    kind: String,
    title: String,
    raw_input: Value,
    locations: Vec<String>,
    diffs: Vec<Diff>,
    text: String,
    status: String,
    /// The input last handed to the chat (a later update may change it: the diff arrives after the call).
    shown: Option<Value>,
    done: bool,
}

impl Tool {
    /// Takes what `update` (a `tool_call`, a `tool_call_update` or the `toolCall` of a permission request) carries.
    fn merge(&mut self, update: &Value) {
        let text = |key: &str| update.get(key).and_then(Value::as_str).map(str::to_owned);
        if let Some(kind) = text("kind") {
            self.kind = kind;
        }
        if let Some(title) = text("title") {
            self.title = title;
        }
        if let Some(status) = text("status") {
            self.status = status;
        }
        if let Some(raw) = update.get("rawInput").filter(|v| !v.is_null()) {
            self.raw_input = raw.clone();
        }
        if let Some(locations) = update.get("locations").and_then(Value::as_array) {
            self.locations = locations.iter().map(|l| str_of(l, "path").to_owned()).filter(|p| !p.is_empty()).collect();
        }
        if let Some(content) = update.get("content").and_then(Value::as_array) {
            self.diffs.clear();
            let mut texts = Vec::new();
            for c in content {
                match str_of(c, "type") {
                    "diff" => self.diffs.push(Diff {
                        path: str_of(c, "path").to_owned(),
                        old: c.get("oldText").and_then(Value::as_str).map(str::to_owned),
                        new: str_of(c, "newText").to_owned(),
                    }),
                    "content" => {
                        let t = str_of(c.get("content").unwrap_or(&Value::Null), "text");
                        if !t.is_empty() {
                            texts.push(t.to_owned());
                        }
                    }
                    _ => {}
                }
            }
            self.text = texts.join("\n");
        }
        // `rawOutput` (an extension most agents send) is the result when the content has no text.
        if self.text.is_empty() {
            if let Some(out) = update.get("rawOutput") {
                self.text = match out {
                    Value::String(s) => s.clone(),
                    Value::Object(o) => ["output", "stdout", "text", "content"]
                        .iter()
                        .find_map(|k| o.get(*k).and_then(Value::as_str))
                        .unwrap_or_default()
                        .to_owned(),
                    _ => String::new(),
                };
            }
        }
    }

    fn raw_str(&self, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|k| match self.raw_input.get(*k) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Array(a)) => {
                let parts: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
                (!parts.is_empty()).then(|| parts.join(" "))
            }
            _ => None,
        })
    }

    fn file(&self) -> String {
        self.diffs
            .first()
            .map(|d| d.path.clone())
            .filter(|p| !p.is_empty())
            .or_else(|| self.locations.first().cloned())
            .or_else(|| self.raw_str(&["filePath", "file_path", "path", "file"]))
            .unwrap_or_else(|| self.title.clone())
    }

    /// The name the chat knows the call by and its input, from the call's `kind`. `None` = a call the view has no use for
    /// (the agent "thinking" as a tool, a mode switch).
    fn view(&self) -> Option<(String, Value)> {
        Some(match self.kind.as_str() {
            "execute" => ("Bash".into(), json!({ "command": self.raw_str(&["command", "cmd"]).unwrap_or_else(|| self.title.clone()) })),
            "edit" => {
                let first = self.file();
                let path = if self.diffs.len() > 1 { format!("{first} (+{} more)", self.diffs.len() - 1) } else { first };
                let mut input = json!({ "file_path": path });
                if !self.diffs.is_empty() {
                    input["diff"] = Value::String(self.diffs.iter().map(diff_text).collect::<Vec<_>>().join("\n"));
                }
                ("Edit".into(), input)
            }
            "delete" => ("Delete".into(), json!({ "file_path": self.file() })),
            "move" => ("Move".into(), json!({ "title": self.title })),
            "read" => ("Read".into(), json!({ "file_path": self.file() })),
            "search" => {
                let pattern = self.raw_str(&["pattern", "query", "regex"]).unwrap_or_else(|| self.title.clone());
                let mut input = json!({ "pattern": pattern });
                if let Some(path) = self.raw_str(&["path", "directory", "dir"]) {
                    input["path"] = Value::String(path);
                }
                ("Grep".into(), input)
            }
            "fetch" => ("WebFetch".into(), json!({ "url": self.raw_str(&["url", "uri"]).unwrap_or_else(|| self.title.clone()) })),
            "think" | "switch_mode" => return None,
            _ => self.generic(),
        })
    }

    /// A call of an unknown kind: named `Tool`, with the title the agent gave it.
    fn generic(&self) -> (String, Value) {
        ("Tool".into(), json!({ "title": self.title }))
    }
}

/// A diff as unified-diff text, for the permission card and the call.
fn diff_text(d: &Diff) -> String {
    unified_diff(d.old.as_deref().unwrap_or(""), &d.new, d.old.is_none())
}

/// What `session/load` streams while it runs: the conversation, built apart so it can replace the view's history.
struct Replay {
    transcript: Transcript,
    /// A user message arrives in chunks; it is closed by the next update of another kind.
    user: String,
}

pub(super) struct AcpProtocol {
    resume: Option<String>,
    cwd: String,
    /// The Holt instructions to put in front of the first prompt of a new conversation (see [`Instructions::FirstPrompt`]).
    instructions: Option<String>,
    preamble: Option<String>,
    next_id: u64,
    awaiting: HashMap<u64, Awaiting>,
    can_load: bool,
    has_auth: bool,
    session: Option<String>,
    model: String,
    commands: Vec<String>,
    /// A `session/prompt` went out and its answer has not come: the next message waits in `queue`, so each one has its result.
    turn_busy: bool,
    queue: VecDeque<String>,
    /// The id of the assistant message being streamed, and the text and thought collected since the last tool call.
    message: u64,
    text: String,
    thought: String,
    tools: HashMap<String, Tool>,
    permissions: HashMap<String, Permission>,
    replay: Option<Replay>,
    /// The conversation `session/load` is replaying.
    loading: Option<String>,
    context_tokens: u64,
    context_window: u64,
}

fn rpc(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn rpc_error(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

impl AcpProtocol {
    pub(super) fn new(resume: Option<String>, cwd: &Path, instructions: Option<String>) -> Self {
        Self {
            resume,
            cwd: cwd.to_string_lossy().into_owned(),
            instructions,
            preamble: None,
            next_id: 0,
            awaiting: HashMap::new(),
            can_load: false,
            has_auth: false,
            session: None,
            model: String::new(),
            commands: Vec::new(),
            turn_busy: false,
            queue: VecDeque::new(),
            message: 0,
            text: String::new(),
            thought: String::new(),
            tools: HashMap::new(),
            permissions: HashMap::new(),
            replay: None,
            loading: None,
            context_tokens: 0,
            context_window: 0,
        }
    }

    fn request(&mut self, method: &str, params: Value, awaiting: Awaiting) -> String {
        self.next_id += 1;
        self.awaiting.insert(self.next_id, awaiting);
        json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params }).to_string()
    }

    fn session_request(&mut self, resume: Option<String>) -> String {
        match resume {
            Some(id) => {
                self.loading = Some(id.clone());
                self.replay = Some(Replay { transcript: Transcript::default(), user: String::new() });
                self.request("session/load", json!({ "sessionId": id, "cwd": self.cwd, "mcpServers": [] }), Awaiting::Session(true))
            }
            None => self.request("session/new", json!({ "cwd": self.cwd, "mcpServers": [] }), Awaiting::Session(false)),
        }
    }

    /// Starts the next waiting message, if the session is ready and no turn is running.
    fn pump(&mut self) -> Vec<String> {
        let Some(session) = self.session.clone() else { return Vec::new() };
        if self.turn_busy {
            return Vec::new();
        }
        let Some(text) = self.queue.pop_front() else { return Vec::new() };
        self.turn_busy = true;
        let text = match self.preamble.take() {
            Some(pre) => format!("{INSTRUCTIONS_OPEN}\n{pre}\n{INSTRUCTIONS_CLOSE}\n\n{text}"),
            None => text,
        };
        let params = json!({ "sessionId": session, "prompt": [{ "type": "text", "text": text }] });
        vec![self.request("session/prompt", params, Awaiting::Prompt)]
    }

    // ----- Events -----

    /// Hands `ev` to the chat, or, while a conversation is being replayed, to the history that will replace the view's.
    fn emit(&mut self, out: &mut Parsed, ev: ChatEvent) {
        match (&mut self.replay, &ev) {
            (Some(replay), ChatEvent::TextDelta { .. } | ChatEvent::Assistant { .. } | ChatEvent::ToolResults(_)) => {
                replay.transcript.apply(&ev);
            }
            _ => out.events.push(ev),
        }
    }

    /// Ends the message and the thought being collected: the complete text replaces what the chunks streamed.
    fn flush(&mut self, out: &mut Parsed) {
        if !self.thought.trim().is_empty() {
            let blocks = vec![Block::Thinking(std::mem::take(&mut self.thought))];
            let ev = ChatEvent::Assistant { message: format!("acp-thought-{}", self.message), blocks, context_tokens: 0 };
            self.emit(out, ev);
        }
        self.thought.clear();
        if !self.text.is_empty() {
            let blocks = vec![Block::Text(std::mem::take(&mut self.text))];
            let ev = ChatEvent::Assistant { message: format!("acp-{}", self.message), blocks, context_tokens: 0 };
            self.emit(out, ev);
        }
        self.message += 1;
    }

    /// A user message of a replayed history is complete.
    fn close_user(&mut self) {
        if let Some(replay) = self.replay.as_mut() {
            let text = std::mem::take(&mut replay.user);
            let text = text.split_once(INSTRUCTIONS_CLOSE).map_or(text.as_str(), |(_, rest)| rest).trim();
            if !text.is_empty() {
                replay.transcript.push(Item::User { text: text.to_owned(), from: None });
            }
        }
    }

    fn init_event(&self) -> ChatEvent {
        ChatEvent::Init {
            session: self.session.clone().unwrap_or_default(),
            model: self.model.clone(),
            commands: self.commands.clone(),
            terminal_commands: Vec::new(),
        }
    }

    // ----- Responses to Holt's requests -----

    fn response(&mut self, id: u64, v: &Value) -> Parsed {
        let Some(awaiting) = self.awaiting.remove(&id) else { return Parsed::default() };
        let error = v.get("error").map(|e| str_of(e, "message").to_owned());
        let result = v.get("result").unwrap_or(&Value::Null);
        let mut out = Parsed::default();
        match awaiting {
            Awaiting::Initialize => match error {
                Some(msg) => out.events.push(ChatEvent::Notice { text: msg, error: true }),
                None => {
                    let caps = result.get("agentCapabilities").unwrap_or(&Value::Null);
                    self.can_load = caps.get("loadSession").and_then(Value::as_bool).unwrap_or(false);
                    self.has_auth = result.get("authMethods").and_then(Value::as_array).is_some_and(|m| !m.is_empty());
                    let resume = self.resume.take();
                    let resume = match resume {
                        Some(_) if !self.can_load => {
                            out.events.push(ChatEvent::Notice {
                                text: t!("This agent cannot resume a saved conversation. A new one was started.").to_owned(),
                                error: false,
                            });
                            None
                        }
                        other => other,
                    };
                    let line = self.session_request(resume);
                    out.reply.push(line);
                }
            },
            Awaiting::Session(loaded) => {
                if let Some(msg) = error {
                    self.session_failed(loaded, &msg, &mut out);
                } else {
                    self.session_ready(loaded, result, &mut out);
                }
            }
            Awaiting::Prompt => self.prompt_done(error, result, &mut out),
        }
        out
    }

    fn session_failed(&mut self, loaded: bool, msg: &str, out: &mut Parsed) {
        self.replay = None;
        self.loading = None;
        if loaded {
            // The conversation is gone (or refused): open a new one instead of a dead tab, and say so.
            out.events.push(ChatEvent::Notice {
                text: t!("Could not resume the conversation ({reason}). A new one was started.", reason = msg),
                error: false,
            });
            let line = self.session_request(None);
            out.reply.push(line);
            return;
        }
        let text = if self.has_auth {
            t!("{reason}. This agent may need you to sign in: do it in the Terminal view of this tab.", reason = msg)
        } else {
            msg.to_owned()
        };
        out.events.push(ChatEvent::Notice { text, error: true });
        // Nothing will answer the messages that waited: end each one as a failed turn.
        for _ in self.queue.drain(..) {
            out.events.push(ChatEvent::Result(TurnResult { is_error: true, error: msg.to_owned(), ..TurnResult::default() }));
        }
    }

    fn session_ready(&mut self, loaded: bool, result: &Value, out: &mut Parsed) {
        if !loaded {
            self.session = result.get("sessionId").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
            self.preamble = self.instructions.clone();
        } else {
            self.session = self.loading.take();
        }
        if let Some(models) = result.get("models") {
            let current = str_of(models, "currentModelId");
            self.model = models
                .get("availableModels")
                .and_then(Value::as_array)
                .and_then(|list| list.iter().find(|m| str_of(m, "modelId") == current))
                .map(|m| str_of(m, "name"))
                .filter(|n| !n.is_empty())
                .unwrap_or(current)
                .to_owned();
        }
        out.events.push(self.init_event());
        if loaded {
            self.flush(out);
            self.close_user();
            let items = self.replay.take().map(|r| r.transcript.items).unwrap_or_default();
            out.events.push(ChatEvent::Replay(items));
        } else {
            // A new conversation has nothing to replay: the chat that waited for a history stops waiting.
            out.events.push(ChatEvent::Replay(Vec::new()));
        }
        out.reply = self.pump();
    }

    fn prompt_done(&mut self, error: Option<String>, result: &Value, out: &mut Parsed) {
        self.turn_busy = false;
        self.flush(out);
        // A call the turn left unfinished (cut by an interrupt): close it, or its card would look busy forever.
        let mut open: Vec<String> = self.tools.iter().filter(|(_, t)| t.shown.is_some() && !t.done).map(|(id, _)| id.clone()).collect();
        open.sort();
        if !open.is_empty() {
            let outcomes = open
                .into_iter()
                .map(|id| ToolOutcome { tool_use_id: id, text: t!("Cancelled.").to_owned(), is_error: true, patch: Vec::new() })
                .collect();
            out.events.push(ChatEvent::ToolResults(outcomes));
        }
        self.tools.clear();
        self.permissions.clear();
        let stop = str_of(result, "stopReason");
        let (is_error, message) = match (&error, stop) {
            (Some(msg), _) => (true, msg.clone()),
            (None, "max_tokens") => (true, t!("The agent stopped: it reached its limit of output tokens.").to_owned()),
            (None, "max_turn_requests") => (true, t!("The agent stopped: it reached its limit of requests in one turn.").to_owned()),
            (None, "refusal") => (true, t!("The agent refused to continue.").to_owned()),
            _ => (false, String::new()),
        };
        if stop == "cancelled" {
            out.events.push(ChatEvent::Interrupted);
        }
        out.events.push(ChatEvent::Result(TurnResult {
            is_error,
            error: message,
            cost_usd: 0.0,
            duration_ms: 0,
            session: self.session.clone().unwrap_or_default(),
            context_window: self.context_window,
        }));
        out.reply = self.pump();
    }

    // ----- Requests from the agent -----

    fn agent_request(&mut self, id: &Value, method: &str, params: &Value) -> Parsed {
        match method {
            "session/request_permission" => self.permission_request(id, params),
            // `fs/*` and `terminal/*` were not advertised; anything else is unknown. An error answer lets the turn go on
            // (the call fails) instead of waiting forever.
            _ => Parsed { events: Vec::new(), reply: vec![rpc_error(id, -32601, "not supported by Holt")] },
        }
    }

    fn permission_request(&mut self, id: &Value, p: &Value) -> Parsed {
        let mut out = Parsed::default();
        let request_id = match id {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let mut perm = Permission { rpc_id: id.clone(), allow_once: None, allow_always: None, reject_once: None, reject_always: None };
        for option in p.get("options").and_then(Value::as_array).into_iter().flatten() {
            let option_id = Some(str_of(option, "optionId").to_owned()).filter(|s| !s.is_empty());
            let slot = match str_of(option, "kind") {
                "allow_once" => &mut perm.allow_once,
                "allow_always" => &mut perm.allow_always,
                "reject_once" => &mut perm.reject_once,
                "reject_always" => &mut perm.reject_always,
                _ => continue,
            };
            slot.get_or_insert_with(|| option_id.unwrap_or_default());
        }
        let call = p.get("toolCall").unwrap_or(&Value::Null);
        let call_id = str_of(call, "toolCallId").to_owned();
        self.flush(&mut out);
        let tool = self.tools.entry(call_id.clone()).or_default();
        tool.merge(call);
        let (name, input) = tool.view().unwrap_or_else(|| tool.generic());
        let description = tool.title.clone();
        self.show_tool(&call_id, &mut out);
        let suggestions = perm.allow_always.iter().map(|o| json!(o)).collect();
        self.permissions.insert(request_id.clone(), perm);
        let ev = ChatEvent::Permission(PermissionRequest { request_id, tool_use_id: call_id, tool: name, input, description, suggestions });
        out.events.push(ev);
        out
    }

    // ----- Notifications -----

    fn notification(&mut self, method: &str, p: &Value) -> Parsed {
        let mut out = Parsed::default();
        if method != "session/update" {
            return out;
        }
        let u = p.get("update").unwrap_or(&Value::Null);
        let kind = str_of(u, "sessionUpdate");
        if kind == "user_message_chunk" {
            // A user message after the agent's answer (a replayed history): the answer is complete.
            if !self.text.is_empty() || !self.thought.is_empty() {
                self.flush(&mut out);
            }
        } else {
            self.close_user();
        }
        let chunk = || str_of(u.get("content").unwrap_or(&Value::Null), "text").to_owned();
        match kind {
            "agent_message_chunk" => {
                let text = chunk();
                if !self.thought.is_empty() {
                    self.flush(&mut out);
                }
                if !text.is_empty() {
                    self.text.push_str(&text);
                    let ev = ChatEvent::TextDelta { message: format!("acp-{}", self.message), text };
                    self.emit(&mut out, ev);
                }
            }
            "agent_thought_chunk" => {
                if !self.text.is_empty() {
                    self.flush(&mut out);
                }
                self.thought.push_str(&chunk());
            }
            "user_message_chunk" => {
                if let Some(replay) = self.replay.as_mut() {
                    replay.user.push_str(&chunk());
                }
            }
            "tool_call" | "tool_call_update" => {
                let id = str_of(u, "toolCallId").to_owned();
                if id.is_empty() {
                    return out;
                }
                self.flush(&mut out);
                self.tools.entry(id.clone()).or_default().merge(u);
                self.show_tool(&id, &mut out);
            }
            "available_commands_update" => {
                self.commands = u
                    .get("availableCommands")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|c| str_of(c, "name").trim_start_matches('/').to_owned())
                    .filter(|n| !n.is_empty())
                    .collect();
                out.events.push(self.init_event());
            }
            "usage_update" => {
                self.context_tokens = u.get("used").and_then(Value::as_u64).unwrap_or(self.context_tokens);
                self.context_window = u.get("size").and_then(Value::as_u64).unwrap_or(self.context_window);
                if self.replay.is_none() {
                    out.events.push(ChatEvent::Assistant { message: String::new(), blocks: Vec::new(), context_tokens: self.context_tokens });
                }
            }
            _ => {}
        }
        out
    }

    /// Hands the call `id` to the chat: its header when it first shows or its input changed, its result when it finished.
    fn show_tool(&mut self, id: &str, out: &mut Parsed) {
        let Some(tool) = self.tools.get_mut(id) else { return };
        let Some((name, input)) = tool.view() else { return };
        let mut events = Vec::new();
        if tool.shown.as_ref() != Some(&input) {
            tool.shown = Some(input.clone());
            events.push(ChatEvent::Assistant {
                message: id.to_owned(),
                blocks: vec![Block::ToolUse { id: id.to_owned(), name: name.clone(), input }],
                context_tokens: 0,
            });
        }
        if !tool.done && matches!(tool.status.as_str(), "completed" | "failed") {
            tool.done = true;
            let failed = tool.status == "failed";
            let patch = tool.diffs.iter().flat_map(|d| patch_from_diff(&diff_text(d))).collect();
            let mut text = clip(&tool.text, RESULT_MAX_CHARS);
            if failed && text.trim().is_empty() {
                text = t!("The call failed.").to_owned();
            }
            events.push(ChatEvent::ToolResults(vec![ToolOutcome { tool_use_id: id.to_owned(), text, is_error: failed, patch }]));
        }
        for ev in events {
            self.emit(out, ev);
        }
    }
}

impl Protocol for AcpProtocol {
    fn open(&mut self) -> Vec<String> {
        let params = json!({
            "protocolVersion": 1,
            // Honest: Holt implements neither the file system nor the terminal methods, so the agent uses its own.
            "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
            "clientInfo": { "name": "holt", "title": "Holt", "version": env!("CARGO_PKG_VERSION") },
        });
        vec![self.request("initialize", params, Awaiting::Initialize)]
    }

    fn parse(&mut self, line: &str) -> Parsed {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return Parsed::default() };
        let method = v.get("method").and_then(Value::as_str);
        let params = v.get("params").unwrap_or(&Value::Null);
        match (v.get("id").filter(|i| !i.is_null()), method) {
            (Some(id), Some(method)) => self.agent_request(id, method, params),
            (Some(id), None) => id.as_u64().map_or_else(Parsed::default, |id| self.response(id, &v)),
            (None, Some(method)) => self.notification(method, params),
            (None, None) => Parsed::default(),
        }
    }

    fn user_message(&mut self, text: &str) -> Vec<String> {
        self.queue.push_back(text.to_owned());
        self.pump()
    }

    fn answer_permission(&mut self, request_id: &str, allow: bool, _input: &Value, _deny_message: &str, rules: &[Value]) -> Vec<String> {
        let Some(perm) = self.permissions.remove(request_id) else { return Vec::new() };
        let option = if allow {
            // "For this session" hands back the id of the `allow_always` option the card carried as its suggestion.
            rules.first().and_then(Value::as_str).map(str::to_owned).or(perm.allow_once).or(perm.allow_always)
        } else {
            perm.reject_once.or(perm.reject_always)
        };
        let outcome = match option {
            Some(option_id) => json!({ "outcome": "selected", "optionId": option_id }),
            None => json!({ "outcome": "cancelled" }),
        };
        vec![rpc(&perm.rpc_id, json!({ "outcome": outcome }))]
    }

    fn interrupt(&mut self) -> Interrupt {
        let Some(session) = self.session.clone().filter(|_| self.turn_busy) else { return Interrupt::Lines(Vec::new()) };
        // The agent must hear that the requests it waits for are cancelled too, then it ends the prompt with "cancelled".
        let mut lines: Vec<String> =
            self.permissions.drain().map(|(_, p)| rpc(&p.rpc_id, json!({ "outcome": { "outcome": "cancelled" } }))).collect();
        lines.push(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": session } }).to_string());
        Interrupt::Lines(lines)
    }
}

// ---------- Diffs ----------

/// The longest common subsequence is only worked out for files up to this many line pairs; a bigger change is shown as
/// everything removed and everything added.
const DIFF_CELLS_MAX: usize = 4_000_000;
const DIFF_CONTEXT: usize = 3;

/// A unified diff of `old` → `new` (`@@ -a,b +c,d @@` hunks with three lines of context). A new file (`created`) is one hunk of
/// added lines.
fn unified_diff(old: &str, new: &str, created: bool) -> String {
    let a: Vec<&str> = if created { Vec::new() } else { old.lines().collect() };
    let b: Vec<&str> = new.lines().collect();
    // Each line of the result: ' ' (both), '-' (only old), '+' (only new).
    let mut ops: Vec<(char, &str)> = Vec::new();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    ops.extend(a[..prefix].iter().map(|l| (' ', *l)));
    let (ma, mb) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    if ma.len().saturating_mul(mb.len()) > DIFF_CELLS_MAX {
        ops.extend(ma.iter().map(|l| ('-', *l)));
        ops.extend(mb.iter().map(|l| ('+', *l)));
    } else {
        // lcs[i][j] = the common subsequence length of ma[i..] and mb[j..].
        let mut lcs = vec![vec![0u32; mb.len() + 1]; ma.len() + 1];
        for i in (0..ma.len()).rev() {
            for j in (0..mb.len()).rev() {
                lcs[i][j] = if ma[i] == mb[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < ma.len() || j < mb.len() {
            if i < ma.len() && j < mb.len() && ma[i] == mb[j] {
                ops.push((' ', ma[i]));
                i += 1;
                j += 1;
            } else if i < ma.len() && (j == mb.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
                // A removed line before the added one that replaces it, as `diff` writes them.
                ops.push(('-', ma[i]));
                i += 1;
            } else {
                ops.push(('+', mb[j]));
                j += 1;
            }
        }
    }
    ops.extend(a[a.len() - suffix..].iter().map(|l| (' ', *l)));

    // Hunks: every change with its context, merged when the contexts touch.
    let changes: Vec<usize> = ops.iter().enumerate().filter(|(_, (c, _))| *c != ' ').map(|(n, _)| n).collect();
    let mut text = String::new();
    let mut k = 0;
    while k < changes.len() {
        let start = changes[k].saturating_sub(DIFF_CONTEXT);
        let mut end = changes[k];
        while k + 1 < changes.len() && changes[k + 1] <= end + 2 * DIFF_CONTEXT + 1 {
            k += 1;
            end = changes[k];
        }
        k += 1;
        let end = (end + DIFF_CONTEXT + 1).min(ops.len());
        let before = ops[..start].iter().filter(|(c, _)| *c != '+').count();
        let after = ops[..start].iter().filter(|(c, _)| *c != '-').count();
        let slice = &ops[start..end];
        let (old_n, new_n) = (slice.iter().filter(|(c, _)| *c != '+').count(), slice.iter().filter(|(c, _)| *c != '-').count());
        let first = |n: usize, len: usize| if len == 0 { n } else { n + 1 };
        text.push_str(&format!("@@ -{},{} +{},{} @@\n", first(before, old_n), old_n, first(after, new_n), new_n));
        for (c, line) in slice {
            text.push(*c);
            text.push_str(line);
            text.push('\n');
        }
    }
    text.trim_end_matches('\n').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_has_the_numbers_of_both_files_and_three_lines_of_context() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
        let new = "1\n2\n3\n4\nFIVE\n6\n7\n8\n9\n10\n";
        assert_eq!(unified_diff(old, new, false), "@@ -2,7 +2,7 @@\n 2\n 3\n 4\n-5\n+FIVE\n 6\n 7\n 8");
        assert_eq!(patch_from_diff(&unified_diff(old, new, false))[0].old_start, 2);
    }

    #[test]
    fn the_flags_of_each_cli_are_the_ones_in_its_docs() {
        let args = |kind: &str| {
            let b = backend_for(kind).expect("a row");
            (b.leading_args(), b.mode_args())
        };
        let v = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(args("opencode"), (v(&["acp"]), v(&[])));
        assert_eq!(args("goose"), (v(&["acp"]), v(&[])));
        assert_eq!(args("gemini"), (v(&[]), v(&["--acp"])));
        assert_eq!(args("copilot"), (v(&[]), v(&["--acp", "--stdio"])));
        assert_eq!(args("qwen"), (v(&[]), v(&["--acp"])));
        assert!(backend_for("aider").is_none() && backend_for("q").is_none() && backend_for("interpreter").is_none());
        // The model and the effort are not launch options: the agent says its model in `session/new`.
        let b = backend_for("gemini").expect("row");
        assert!(b.spawn_args(&super::super::Launch { model: Some("m"), effort: Some(Effort::High) }) == v(&["--acp"]));
        assert!(!b.resumes_by_args());
    }

    #[test]
    fn a_new_file_is_one_hunk_of_added_lines_and_two_far_changes_are_two_hunks() {
        assert_eq!(unified_diff("", "a\nb\n", true), "@@ -0,0 +1,2 @@\n+a\n+b");
        let old = (1..=30).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
        let new = old.replace("\n2\n", "\ntwo\n").replace("\n28\n", "\ntwenty-eight\n");
        let hunks = patch_from_diff(&unified_diff(&old, &new, false));
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[1].old_start, 25);
        assert_eq!(unified_diff("same\n", "same\n", false), "");
    }
}
