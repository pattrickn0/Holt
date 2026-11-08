//! The list items the interface consumes and the full command of a tab.

use std::collections::BTreeMap;
use std::path::PathBuf;
/// Agent menu item (equivalent to the return of `agents:list`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentInfo {
    /// `claude`, `antigravity`, `codex`, ..., `shell`, or a custom CLI slug.
    pub id: String,
    pub label: String,
    /// `#rrggbb` color.
    pub color: String,
    pub installed: bool,
    pub peer: bool,
    /// `true` when the entry was manually added by the user (`Config::custom_agents`).
    pub custom: bool,
    /// Full command line (`program + args`) for user-added CLIs.
    pub command: Option<String>,
    /// Local model detected from the CLI or its config (`ollama list`, Mimo config, etc.).
    pub local_model: Option<String>,
}

/// Full command to open an interactive tab in a pseudo-terminal.
#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// COMPLETE environment to use (already derived from the process environment, with PATH/HOLT_* injected
    /// and CLAUDECODE & co. removed). The UI must clear the inherited environment and apply this one.
    pub env: BTreeMap<String, String>,
    /// Tab label (e.g. "Claude").
    pub label: String,
    pub color: String,
    /// Original `kind` requested.
    pub kind: String,
    /// If `true`, delivering a message to the tab must send an extra `\r` after 600 ms when the
    /// text contains `@` (Claude/Antigravity file autocomplete).
    pub double_enter_on_mention: bool,
    /// LLM model of the instance (already resolved and saved in `tab_models`); `None` = the CLI default.
    pub model: Option<String>,
    /// The launch continues a conversation saved for the tab (`--resume <id>` and the like). If the process ends right
    /// away with a failure, the UI forgets the id and opens the tab again from scratch (see `resume::resume_failed`).
    pub resumed: bool,
    /// Antigravity's `--log-file` for this process: where its conversation id shows up (see `resume::agy_session_in_log`).
    pub session_log: Option<PathBuf>,
    /// Chat launch only: the task's initial prompt, which is sent as the first message instead of being a command-line
    /// argument (see `chat.rs`).
    pub first_message: Option<String>,
    /// The conversation this launch continues (the session id saved for the tab), when `resumed`. The command line
    /// already carries it for the CLIs that resume by argument; a chat backend that resumes through its protocol
    /// (Codex's `thread/resume`) reads it here.
    pub session: Option<String>,
    /// Chat launch only: the Holt instructions, when the CLI's chat backend puts them in front of the first prompt of a new
    /// conversation ([`crate::chat::ChatBackend::prompt_instructions`]).
    pub instructions: Option<String>,
}
