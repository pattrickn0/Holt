//! What an agent is doing during a turn, and how long a tab has been idle.
//!
//! The phase is derived from what the CLI reports, never self-declared by the agent: Claude's `PostToolUse` hook calls
//! `ask-agent --signal tool`, which maps the tool call with [`Phase::from_tool`] (and a shell command with
//! [`phase_for_command`]), and `POST /signal` (see `bridge.rs`) hands it to the interface.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::t;

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
