//! The conversation as a list of items, built from the events or from the CLI's own transcript file.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::t;

use super::*;

// ---------- Transcript ----------

/// What an agent is doing during a turn, derived from its CLI's reports (never self-declared).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Editing,
    Testing,
    Building,
    RunningCommand,
    Reading,
    SearchingWeb,
    WaitingForYou,
    Writing,
}

impl Phase {
    /// Wire code used by `ask-agent --signal tool` and `POST /signal`.
    pub fn as_code(self) -> &'static str {
        match self {
            Phase::Editing => "editing",
            Phase::Testing => "testing",
            Phase::Building => "building",
            Phase::RunningCommand => "running_command",
            Phase::Reading => "reading",
            Phase::SearchingWeb => "searching_web",
            Phase::WaitingForYou => "waiting_for_you",
            Phase::Writing => "writing",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s.trim() {
            "editing" => Some(Phase::Editing),
            "testing" => Some(Phase::Testing),
            "building" => Some(Phase::Building),
            "running_command" | "running a command" => Some(Phase::RunningCommand),
            "reading" => Some(Phase::Reading),
            "searching_web" | "searching the web" => Some(Phase::SearchingWeb),
            "waiting_for_you" | "waiting for you" => Some(Phase::WaitingForYou),
            "writing" => Some(Phase::Writing),
            _ => None,
        }
    }

    /// User-facing label (translated via `t!`).
    pub fn label(self) -> &'static str {
        match self {
            Phase::Editing => t!("editing"),
            Phase::Testing => t!("testing"),
            Phase::Building => t!("building"),
            Phase::RunningCommand => t!("running a command"),
            Phase::Reading => t!("reading"),
            Phase::SearchingWeb => t!("searching the web"),
            Phase::WaitingForYou => t!("waiting for you"),
            Phase::Writing => t!("writing"),
        }
    }

    /// Maps a tool call (`name` and `input`) from Claude, Antigravity or Codex to its phase.
    pub fn from_tool(name: &str, input: &Value) -> Option<Self> {
        match name {
            "Edit"
            | "Write"
            | "MultiEdit"
            | "NotebookEdit"
            | "edit_file"
            | "write_file"
            | "write_to_file"
            | "replace_file_content"
            | "multi_replace_file_content"
            | "notebook_edit"
            | "sed_file" => Some(Phase::Editing),
            "Bash" | "run_command" => {
                let cmd = ["command", "CommandLine", "cmd"].iter().find_map(|k| input.get(*k).and_then(Value::as_str)).unwrap_or("");
                Some(phase_for_command(cmd))
            }
            "Read" | "Grep" | "Glob" | "LS" | "NotebookRead" | "read_file" | "view_file" | "grep_search" | "find_by_name" | "list_dir"
            | "view_file_outline" | "view_code_item" => Some(Phase::Reading),
            "WebSearch" | "WebFetch" | "search_web" | "read_url_content" => Some(Phase::SearchingWeb),
            _ => None,
        }
    }
}

/// Classifies a shell command into `Testing`, `Building` or `RunningCommand`.
pub fn phase_for_command(cmd: &str) -> Phase {
    let mut found = None;
    for seg in cmd.split([';', '\n', '|']).flat_map(|s| s.split("&&")) {
        let words: Vec<&str> = seg
            .split_whitespace()
            .map(|w| w.trim_matches(['"', '\'']))
            .filter(|w| !w.is_empty())
            .skip_while(|w| is_env_or_wrapper(w))
            .collect();
        if let Some(p) = command_words_phase(&words) {
            found = Some(p);
        }
    }
    found.unwrap_or(Phase::RunningCommand)
}

fn is_env_or_wrapper(word: &str) -> bool {
    (word.contains('=') && !word.starts_with('-')) || matches!(word, "sudo" | "env" | "time" | "nice")
}

fn command_words_phase(words: &[&str]) -> Option<Phase> {
    let bin = words.first()?.rsplit(['/', '\\']).next()?;
    let bin = bin.strip_suffix(".exe").unwrap_or(bin);
    let mut args = words.get(1..).unwrap_or(&[]).iter().copied().filter(|w| !w.starts_with('-') && !w.starts_with('+'));
    let sub = args.next();
    let sub2 = args.next();
    match (bin, sub, sub2) {
        ("cargo", Some("test" | "nextest" | "t"), _) => Some(Phase::Testing),
        ("cargo", Some("build" | "b" | "check" | "c" | "clippy"), _) => Some(Phase::Building),
        ("npm" | "pnpm" | "yarn" | "bun", Some("test" | "t"), _) => Some(Phase::Testing),
        ("npm" | "pnpm" | "yarn" | "bun", Some("run"), Some("test")) => Some(Phase::Testing),
        ("npm" | "pnpm" | "yarn" | "bun", Some("build"), _) => Some(Phase::Building),
        ("npm" | "pnpm" | "yarn" | "bun", Some("run"), Some("build")) => Some(Phase::Building),
        ("pytest" | "jest" | "vitest" | "ctest", _, _) => Some(Phase::Testing),
        ("python" | "python3", Some("pytest" | "unittest"), _) => Some(Phase::Testing),
        ("go" | "dotnet", Some("test"), _) => Some(Phase::Testing),
        ("go" | "dotnet", Some("build"), _) => Some(Phase::Building),
        ("make", Some("test" | "check"), _) => Some(Phase::Testing),
        ("make" | "cmake" | "ninja" | "tsc" | "rustc" | "gcc" | "clang" | "g++" | "clang++", _, _) => Some(Phase::Building),
        _ => None,
    }
}

/// How long an idle tab has been idle (`"idle <1 min"`, `"idle 12 min"`, `"idle 2 h"`).
pub fn idle_label(elapsed: std::time::Duration) -> String {
    let mins = elapsed.as_secs() / 60;
    if mins == 0 {
        t!("idle <1 min").to_owned()
    } else if mins < 60 {
        t!("idle {n} min", n = mins)
    } else {
        t!("idle {n} h", n = mins / 60)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermState {
    Pending,
    Allowed,
    Denied,
    /// The turn ended (or the CLI withdrew the request) before an answer.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub outcome: Option<ToolOutcome>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionCard {
    pub request_id: String,
    pub tool_use_id: String,
    pub tool: String,
    pub input: Value,
    pub description: String,
    pub suggestions: Vec<Value>,
    pub state: PermState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// What the user typed, or a message another agent sent to this tab (`from` = the agent's id).
    User {
        text: String,
        from: Option<String>,
    },
    Assistant {
        message: String,
        text: String,
        streaming: bool,
    },
    Thinking(String),
    Tool(ToolCall),
    Permission(PermissionCard),
    /// Something Holt says in the conversation (a terminal-only command, an interrupt, an error).
    Notice {
        text: String,
        error: bool,
    },
}

/// The conversation of one chat tab.
#[derive(Debug, Clone, Default)]
pub struct Transcript {
    pub items: Vec<Item>,
    /// Turns sent and not yet finished; the tab is busy while there are any.
    pub(crate) pending_turns: u32,
    pub session: Option<String>,
    pub model: String,
    /// Slash commands the CLI offers (names without the slash).
    pub commands: Vec<String>,
    /// Commands the CLI itself says only work in the terminal.
    pub terminal_commands: Vec<String>,
    pub cost_usd: f64,
    /// Tokens in the context after the last message, and the model's window (0 = unknown).
    pub context_tokens: u64,
    pub context_window: u64,
    /// Bumped on every change, for the interface to know when to lay the text out again.
    pub version: u64,
    /// The conversation was resumed and its history is not read yet: the process sends it ([`ChatEvent::Replay`]).
    pub history_pending: bool,
}

impl Transcript {
    /// How much of the context window the conversation fills, in percent (None until both numbers are known).
    pub fn context_percent(&self) -> Option<u32> {
        (self.context_window > 0 && self.context_tokens > 0)
            .then(|| u32::try_from((self.context_tokens * 100 / self.context_window).min(100)).unwrap_or(100))
    }

    pub fn busy(&self) -> bool {
        self.pending_turns > 0
    }

    /// A permission card is waiting for the user.
    pub fn waiting_for_permission(&self) -> bool {
        self.items.iter().any(|i| matches!(i, Item::Permission(p) if p.state == PermState::Pending))
    }

    /// What the agent is doing right now in the running turn, derived from its CLI's reports.
    pub fn phase(&self) -> Option<Phase> {
        if !self.busy() {
            return None;
        }
        if self.waiting_for_permission() {
            return Some(Phase::WaitingForYou);
        }
        for item in self.items.iter().rev() {
            match item {
                Item::User { .. } => break,
                Item::Assistant { streaming: true, .. } => return Some(Phase::Writing),
                Item::Tool(call) => {
                    if let Some(p) = Phase::from_tool(&call.name, &call.input) {
                        return Some(p);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn push(&mut self, item: Item) {
        self.items.push(item);
        if self.items.len() > ITEMS_MAX {
            let drop = self.items.len() - ITEMS_MAX;
            self.items.drain(..drop);
        }
    }

    /// A message was sent to the CLI (it starts a turn).
    pub fn push_user(&mut self, text: &str, from: Option<&str>) {
        self.version += 1;
        self.finish_streaming();
        self.pending_turns += 1;
        self.push(Item::User { text: text.to_owned(), from: from.map(str::to_owned) });
    }

    pub fn push_notice(&mut self, text: impl Into<String>, error: bool) {
        self.version += 1;
        self.push(Item::Notice { text: text.into(), error });
    }

    fn finish_streaming(&mut self) {
        for item in &mut self.items {
            if let Item::Assistant { streaming, .. } = item {
                *streaming = false;
            }
        }
    }

    /// The turn the process was running is gone (it exited or was killed).
    pub fn process_ended(&mut self) {
        self.version += 1;
        self.pending_turns = 0;
        self.finish_streaming();
        self.cancel_permissions();
    }

    fn cancel_permissions(&mut self) {
        for item in &mut self.items {
            if let Item::Permission(p) = item {
                if p.state == PermState::Pending {
                    p.state = PermState::Cancelled;
                }
            }
        }
    }

    /// The user answered a card.
    pub fn answer_permission(&mut self, request_id: &str, allow: bool) {
        self.version += 1;
        for item in &mut self.items {
            if let Item::Permission(p) = item {
                if p.request_id == request_id && p.state == PermState::Pending {
                    p.state = if allow { PermState::Allowed } else { PermState::Denied };
                }
            }
        }
    }

    pub fn pending_permission(&self) -> Option<&PermissionCard> {
        self.items.iter().find_map(|i| match i {
            Item::Permission(p) if p.state == PermState::Pending => Some(p),
            _ => None,
        })
    }

    pub fn apply(&mut self, ev: &ChatEvent) {
        self.version += 1;
        match ev {
            ChatEvent::Init { session, model, commands, terminal_commands } => {
                if !session.is_empty() {
                    self.session = Some(session.clone());
                }
                if !model.is_empty() {
                    self.model = model.clone();
                }
                self.commands = commands.clone();
                self.terminal_commands = terminal_commands.clone();
            }
            ChatEvent::Requesting | ChatEvent::UnknownRequest { .. } | ChatEvent::Usage(_) | ChatEvent::Ignored => {}
            ChatEvent::Notice { text, error } => self.push(Item::Notice { text: text.clone(), error: *error }),
            ChatEvent::Replay(items) => {
                // What the user already typed while the process was starting stays after the history.
                if std::mem::take(&mut self.history_pending) {
                    let later = std::mem::take(&mut self.items);
                    self.items = items.clone();
                    self.items.extend(later);
                    let drop = self.items.len().saturating_sub(ITEMS_MAX);
                    self.items.drain(..drop);
                }
            }
            ChatEvent::TextDelta { message, text } => match self.items.last_mut() {
                Some(Item::Assistant { message: m, text: t, streaming: true }) if m == message => t.push_str(text),
                _ => self.push(Item::Assistant { message: message.clone(), text: text.clone(), streaming: true }),
            },
            ChatEvent::Assistant { message, blocks, context_tokens } => {
                if *context_tokens > 0 {
                    self.context_tokens = *context_tokens;
                }
                for block in blocks {
                    self.apply_block(message, block);
                }
            }
            ChatEvent::ToolResults(results) => {
                for r in results {
                    let call = self.items.iter_mut().rev().find_map(|i| match i {
                        Item::Tool(c) if c.id == r.tool_use_id => Some(c),
                        _ => None,
                    });
                    if let Some(call) = call {
                        call.outcome = Some(r.clone());
                    }
                }
            }
            ChatEvent::Permission(p) => {
                self.finish_streaming();
                self.push(Item::Permission(PermissionCard {
                    request_id: p.request_id.clone(),
                    tool_use_id: p.tool_use_id.clone(),
                    tool: p.tool.clone(),
                    input: p.input.clone(),
                    description: p.description.clone(),
                    suggestions: p.suggestions.clone(),
                    state: PermState::Pending,
                }));
            }
            ChatEvent::PermissionCancelled(id) => {
                for item in &mut self.items {
                    if let Item::Permission(p) = item {
                        if p.request_id == *id && p.state == PermState::Pending {
                            p.state = PermState::Cancelled;
                        }
                    }
                }
            }
            ChatEvent::Interrupted => self.push(Item::Notice { text: t!("Interrupted.").to_owned(), error: false }),
            ChatEvent::Result(r) => {
                self.pending_turns = self.pending_turns.saturating_sub(1);
                self.finish_streaming();
                self.cancel_permissions();
                self.cost_usd = r.cost_usd;
                if r.context_window > 0 {
                    self.context_window = r.context_window;
                }
                if !r.session.is_empty() {
                    self.session = Some(r.session.clone());
                }
                if r.is_error && !r.error.is_empty() {
                    self.push(Item::Notice { text: r.error.clone(), error: true });
                }
            }
        }
    }

    fn apply_block(&mut self, message: &str, block: &Block) {
        match block {
            Block::Text(text) => {
                // The final text of a block that was streaming replaces what the deltas built.
                let streamed = self.items.iter_mut().rev().find_map(|i| match i {
                    Item::Assistant { message: m, text: t, streaming: true } if m == message => Some(t),
                    _ => None,
                });
                if let Some(t) = streamed {
                    t.clone_from(text);
                    self.finish_streaming();
                } else if !text.trim().is_empty() {
                    self.push(Item::Assistant { message: message.to_owned(), text: text.clone(), streaming: false });
                }
            }
            Block::Thinking(text) => {
                if !text.trim().is_empty() {
                    self.push(Item::Thinking(text.clone()));
                }
            }
            Block::ToolUse { id, name, input } => {
                let known = self.items.iter_mut().rev().find_map(|i| match i {
                    Item::Tool(c) if c.id == *id => Some(c),
                    _ => None,
                });
                match known {
                    // The same call again, with more to say (ACP sends the diff of an edit after the call): it updates the
                    // call while it has no result.
                    Some(call) => {
                        if call.outcome.is_none() && (call.input != *input || call.name != *name) {
                            call.name.clone_from(name);
                            call.input = input.clone();
                        }
                    }
                    None => {
                        self.finish_streaming();
                        self.push(Item::Tool(ToolCall { id: id.clone(), name: name.clone(), input: input.clone(), outcome: None }));
                    }
                }
            }
        }
    }

    /// Rebuilds the conversation from the CLI's own transcript file (`<claude>/projects/<folder>/<session>.jsonl`), so a
    /// resumed tab shows what was said before. Only the last `max_items` items are kept.
    pub fn from_history(jsonl: &str, max_items: usize) -> Transcript {
        let mut t = Transcript::default();
        for line in jsonl.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            if v.get("isSidechain").and_then(Value::as_bool).unwrap_or(false) || v.get("isMeta").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let content = v.get("message").and_then(|m| m.get("content")).unwrap_or(&Value::Null);
            match str_of(&v, "type") {
                "user" => {
                    let results = tool_results(content, v.get("toolUseResult").unwrap_or(&Value::Null));
                    if !results.is_empty() {
                        t.apply(&ChatEvent::ToolResults(results));
                        continue;
                    }
                    let text = user_text(content);
                    let text = history_user_text(&text);
                    if let Some(text) = text {
                        t.push(Item::User { text, from: None });
                    }
                }
                "assistant" => {
                    let msg = v.get("message").unwrap_or(&Value::Null);
                    if str_of(msg, "model") == "<synthetic>" && !t.items.iter().any(|i| matches!(i, Item::User { .. })) {
                        continue;
                    }
                    let context_tokens = context_tokens(msg.get("usage").unwrap_or(&Value::Null));
                    t.apply(&ChatEvent::Assistant { message: str_of(msg, "id").to_owned(), blocks: blocks_of(content), context_tokens });
                }
                _ => {}
            }
        }
        let keep_from = t.items.len().saturating_sub(max_items);
        t.items.drain(..keep_from);
        t.pending_turns = 0;
        t
    }
}

/// What a user line of the CLI's transcript shows: the text, `/command args` for a slash command, nothing for the
/// reminders and local-command noise the CLI stores there.
fn history_user_text(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.starts_with("<local-command") || text.starts_with("<system-reminder") || text.starts_with(INTERRUPTED) {
        return None;
    }
    if text.starts_with("<command-name>") {
        let tag = |name: &str| -> String {
            let open = format!("<{name}>");
            let close = format!("</{name}>");
            text.split_once(&open)
                .and_then(|(_, rest)| rest.split_once(&close))
                .map(|(inner, _)| inner.trim().to_owned())
                .unwrap_or_default()
        };
        let (name, args) = (tag("command-name"), tag("command-args"));
        return Some(if args.is_empty() { name } else { format!("{name} {args}") });
    }
    Some(text.to_owned())
}

/// The path of the CLI's transcript for `session`, if the CLI has it saved.
pub fn history_file(config_dir: &Path, session: &str) -> Option<std::path::PathBuf> {
    if !crate::resume::valid_session_id(session) {
        return None;
    }
    std::fs::read_dir(config_dir.join("projects")).ok()?.flatten().map(|d| d.path().join(format!("{session}.jsonl"))).find(|p| p.is_file())
}

/// The path of Antigravity's transcript for `session`, if saved.
pub fn agy_history_file(home: &Path, session: &str) -> Option<std::path::PathBuf> {
    if !crate::resume::valid_session_id(session) {
        return None;
    }
    let p = home
        .join(".gemini")
        .join("antigravity-cli")
        .join("brain")
        .join(session)
        .join(".system_generated")
        .join("logs")
        .join("transcript.jsonl");
    p.is_file().then_some(p)
}

/// Rebuilds the conversation from Antigravity's transcript file
/// (`<home>/.gemini/antigravity-cli/brain/<session>/.system_generated/logs/transcript.jsonl`), so a resumed tab shows what was said before.
pub fn from_agy_history(jsonl: &str, max_items: usize) -> Transcript {
    let mut t = Transcript::default();
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        match str_of(&v, "type") {
            "USER_INPUT" => {
                let content = str_of(&v, "content");
                let text = history_agy_user_text(content);
                if !text.is_empty() {
                    t.push(Item::User { text, from: None });
                }
            }
            "PLANNER_RESPONSE" => {
                let thinking = str_of(&v, "thinking").trim();
                if !thinking.is_empty() {
                    t.push(Item::Thinking(thinking.to_owned()));
                }
                if let Some(tool_calls) = v.get("tool_calls").and_then(Value::as_array) {
                    let step_idx = v.get("step_index").and_then(Value::as_u64).unwrap_or(0);
                    for (i, call) in tool_calls.iter().enumerate() {
                        let name = str_of(call, "name").to_owned();
                        let input = call.get("args").cloned().unwrap_or(Value::Null);
                        let id = format!("step_{step_idx}_{i}");
                        t.push(Item::Tool(ToolCall { id, name, input, outcome: None }));
                    }
                }
                let content = str_of(&v, "content").trim();
                if !content.is_empty() {
                    let step_idx = v.get("step_index").and_then(Value::as_u64).unwrap_or(0);
                    t.push(Item::Assistant { message: format!("step_{step_idx}"), text: content.to_owned(), streaming: false });
                }
            }
            "GENERIC" => {
                let content = str_of(&v, "content");
                if let Some(last_tool) = t.items.iter_mut().rev().find_map(|item| match item {
                    Item::Tool(c) if c.outcome.is_none() => Some(c),
                    _ => None,
                }) {
                    let patch = if last_tool.name == "edit_file" { agy_patch_of(Some(&last_tool.input)) } else { Vec::new() };
                    last_tool.outcome =
                        Some(ToolOutcome { tool_use_id: last_tool.id.clone(), text: content.to_owned(), is_error: false, patch });
                }
            }
            _ => {}
        }
    }
    let keep_from = t.items.len().saturating_sub(max_items);
    t.items.drain(..keep_from);
    t.pending_turns = 0;
    t
}

fn history_agy_user_text(raw: &str) -> String {
    if let Some((_, rest)) = raw.split_once("<USER_REQUEST>") {
        if let Some((req, _)) = rest.split_once("</USER_REQUEST>") {
            return req.trim().to_owned();
        }
    }
    raw.trim().to_owned()
}
