//! The official catalog: ids, labels, colors, install hints and aliases.

use crate::t;

pub const COLOR_CLAUDE: &str = "#d97757";
pub const COLOR_ANTIGRAVITY: &str = "#4f8cf7";
pub const COLOR_CODEX: &str = "#10a37f";
pub const COLOR_GEMINI: &str = "#8e75f2";
pub const COLOR_QWEN: &str = "#0ea5e9";
pub const COLOR_SHELL: &str = "#9aa0a6";

/// Static definition of an agent (equivalent to an `AGENTS` entry).
#[derive(Debug, Clone, Copy)]
pub struct AgentDef {
    pub id: &'static str,
    pub label: &'static str,
    pub color: &'static str,
    pub peer: bool,
    /// Whether the CLI has an autonomous shell tool to run `ask-agent --reply <n> "answer"` in an interactive tab.
    pub can_answer: bool,
    /// "Not installed" message (with `\r\n`, since it goes straight to the terminal).
    pub missing: &'static str,
}

/// The Antigravity installer is a PowerShell script on Windows; on the other systems, the one from the site.
#[cfg(windows)]
const ANTIGRAVITY_MISSING: &str =
    "Antigravity CLI (agy) not found. Install from PowerShell with:\r\n  irm https://antigravity.google/cli/install.ps1 | iex";
#[cfg(not(windows))]
const ANTIGRAVITY_MISSING: &str = "Antigravity CLI (agy) not found. See the installer for your system at:\r\n  https://antigravity.google";

/// All agents, in menu order.
pub const AGENTS: &[AgentDef] = &[
    AgentDef {
        id: "claude",
        label: "Claude",
        color: COLOR_CLAUDE,
        peer: true,
        can_answer: true,
        missing: "Claude Code CLI (claude) not found. Install with:\r\n  npm install -g @anthropic-ai/claude-code",
    },
    AgentDef {
        id: "antigravity",
        label: "Antigravity",
        color: COLOR_ANTIGRAVITY,
        peer: true,
        can_answer: true,
        missing: ANTIGRAVITY_MISSING,
    },
    AgentDef {
        id: "codex",
        label: "Codex",
        color: COLOR_CODEX,
        peer: true,
        can_answer: true,
        missing: "Codex CLI not found. Install with:\r\n  npm install -g @openai/codex",
    },
    AgentDef {
        id: "opencode",
        label: "OpenCode",
        color: "#f97316",
        peer: true,
        can_answer: true,
        missing: "OpenCode not found. Install with:\r\n  npm install -g opencode-ai",
    },
    // Aider has no autonomous shell execution tool (`/run` is manual and does not answer prompts), so it cannot call `ask-agent --reply`.
    AgentDef {
        id: "aider",
        label: "Aider",
        color: "#22c55e",
        peer: true,
        can_answer: false,
        missing: "Aider CLI not found. Install with:\r\n  pip install aider-chat",
    },
    AgentDef {
        id: "goose",
        label: "Goose",
        color: "#10b981",
        peer: true,
        can_answer: true,
        missing: "Goose CLI not found. Install from: https://github.com/block/goose",
    },
    AgentDef {
        id: "copilot",
        label: "Copilot",
        color: "#6e40c9",
        peer: true,
        can_answer: true,
        missing: "GitHub Copilot CLI not found. Install with:\r\n  npm install -g @github/copilot",
    },
    AgentDef {
        id: "gemini",
        label: "Gemini",
        color: COLOR_GEMINI,
        peer: true,
        can_answer: true,
        missing: "Gemini CLI not found. Install with:\r\n  npm install -g @google/gemini-cli",
    },
    AgentDef {
        id: "qwen",
        label: "Qwen",
        color: COLOR_QWEN,
        peer: true,
        can_answer: true,
        missing: "Qwen Code not found. Install with:\r\n  npm install -g @qwen-code/qwen-code",
    },
    AgentDef { id: "q", label: "Amazon Q", color: "#ff9900", peer: true, can_answer: true, missing: "Amazon Q Developer CLI not found." },
    AgentDef {
        id: "interpreter",
        label: "Interpreter",
        color: "#8b5cf6",
        peer: true,
        can_answer: true,
        missing: "Open Interpreter not found. Install with:\r\n  pip install open-interpreter",
    },
    // `ollama run` is a raw model REPL without shell tools, so it cannot call `ask-agent --reply`.
    AgentDef {
        id: "ollama",
        label: "Ollama",
        color: "#38bdf8",
        peer: true,
        can_answer: false,
        missing: "Ollama CLI (ollama) not found. Install from: https://ollama.com",
    },
    AgentDef { id: "mimo", label: "Mimo Code", color: "#a3e635", peer: true, can_answer: true, missing: "Mimo Code CLI (mimo) not found." },
    AgentDef {
        id: "crush",
        label: "Crush",
        color: "#fb7185",
        peer: true,
        can_answer: true,
        missing: "Crush CLI (crush) not found. Install from: https://github.com/charmbracelet/crush",
    },
    AgentDef {
        id: "cline",
        label: "Cline",
        color: "#2dd4bf",
        peer: true,
        can_answer: true,
        missing: "Cline CLI (cline) not found. Install with:\r\n  npm install -g cline",
    },
    AgentDef {
        id: "amp",
        label: "Amp",
        color: "#f59e0b",
        peer: true,
        can_answer: true,
        missing: "Amp CLI (amp) not found. Install with:\r\n  npm install -g @sourcegraph/amp",
    },
    AgentDef {
        id: "droid",
        label: "Droid",
        color: "#34d399",
        peer: true,
        can_answer: true,
        missing: "Factory Droid CLI (droid) not found.",
    },
    AgentDef {
        id: "cn",
        label: "Continue",
        color: "#e879f9",
        peer: true,
        can_answer: true,
        missing: "Continue CLI (cn) not found. Install with:\r\n  npm install -g @continuedev/cli",
    },
    // `llama-cli` is a raw model REPL without shell tools, so it cannot call `ask-agent --reply`.
    AgentDef {
        id: "llama-cli",
        label: "llama.cpp",
        color: "#ef4444",
        peer: true,
        can_answer: false,
        missing: "llama.cpp CLI (llama-cli) not found.",
    },
    AgentDef {
        id: "plandex",
        label: "Plandex",
        color: "#22d3ee",
        peer: true,
        can_answer: true,
        missing: "Plandex CLI (plandex) not found. Install from: https://plandex.ai",
    },
    AgentDef { id: "shell", label: "Terminal", color: COLOR_SHELL, peer: false, can_answer: false, missing: "Shell not found." },
];

impl AgentDef {
    /// The "not installed" message for the app: user-facing text, so it follows the interface language. The agents
    /// get [`AgentDef::missing`] instead, which is always English.
    pub fn install_hint(&self) -> &'static str {
        match self.id {
            "claude" => t!("Claude Code CLI (claude) not found. Install with:\r\n  npm install -g @anthropic-ai/claude-code"),
            "antigravity" => antigravity_hint(),
            "codex" => t!("Codex CLI not found. Install with:\r\n  npm install -g @openai/codex"),
            "opencode" => t!("OpenCode not found. Install with:\r\n  npm install -g opencode-ai"),
            "aider" => t!("Aider CLI not found. Install with:\r\n  pip install aider-chat"),
            "goose" => t!("Goose CLI not found. Install from: https://github.com/block/goose"),
            "copilot" => t!("GitHub Copilot CLI not found. Install with:\r\n  npm install -g @github/copilot"),
            "gemini" => t!("Gemini CLI not found. Install with:\r\n  npm install -g @google/gemini-cli"),
            "qwen" => t!("Qwen Code not found. Install with:\r\n  npm install -g @qwen-code/qwen-code"),
            "q" => t!("Amazon Q Developer CLI not found."),
            "interpreter" => t!("Open Interpreter not found. Install with:\r\n  pip install open-interpreter"),
            "ollama" => t!("Ollama CLI (ollama) not found. Install from: https://ollama.com"),
            "mimo" => t!("Mimo Code CLI (mimo) not found."),
            "crush" => t!("Crush CLI (crush) not found. Install from: https://github.com/charmbracelet/crush"),
            "cline" => t!("Cline CLI (cline) not found. Install with:\r\n  npm install -g cline"),
            "amp" => t!("Amp CLI (amp) not found. Install with:\r\n  npm install -g @sourcegraph/amp"),
            "droid" => t!("Factory Droid CLI (droid) not found."),
            "cn" => t!("Continue CLI (cn) not found. Install with:\r\n  npm install -g @continuedev/cli"),
            "llama-cli" => t!("llama.cpp CLI (llama-cli) not found."),
            "plandex" => t!("Plandex CLI (plandex) not found. Install from: https://plandex.ai"),
            _ => t!("Shell not found."),
        }
    }
}

/// [`AgentDef::install_hint`] for the Antigravity errors, in the wording of [`ANTIGRAVITY_MISSING`].
#[cfg(windows)]
fn antigravity_hint() -> &'static str {
    t!("Antigravity CLI (agy) not found. Install from PowerShell with:\r\n  irm https://antigravity.google/cli/install.ps1 | iex")
}

/// [`AgentDef::install_hint`] for the Antigravity errors, in the wording of [`ANTIGRAVITY_MISSING`].
#[cfg(not(windows))]
fn antigravity_hint() -> &'static str {
    t!("Antigravity CLI (agy) not found. See the installer for your system at:\r\n  https://antigravity.google")
}

/// Aliases accepted by the bridge (`ask-agent agy ...`).
pub const ALIASES: &[(&str, &str)] = &[
    ("agy", "antigravity"),
    ("claudecode", "claude"),
    ("gh-copilot", "copilot"),
    ("amazon-q", "q"),
    ("openai", "codex"),
    ("open-code", "opencode"),
    ("gemini-cli", "gemini"),
    ("qwen-code", "qwen"),
    ("mimo-code", "mimo"),
    ("continue", "cn"),
    ("llamacpp", "llama-cli"),
    ("llama", "llama-cli"),
    ("factory", "droid"),
];
