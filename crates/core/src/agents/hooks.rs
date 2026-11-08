//! Files written for a CLI: Claude hooks and status line, instructions, Antigravity's folder.

use std::path::{Path, PathBuf};

use super::*;

/// Writes `data/hooks/claude-settings.json`: hooks that tell the bridge when Claude starts
/// (`UserPromptSubmit`) and ends (`Stop`) a turn, which add to the user's settings, and
/// the statusline (`ask-agent --usage`), which sends the plan usage to the bridge. Ours replaces the
/// user's, so `ask-agent --usage` runs theirs and shows its output; its `padding`,
/// if any, is kept here (Claude Code applies it, not our line).
pub(super) fn write_claude_hooks(data_dir: &Path, project_dir: &Path) -> std::io::Result<PathBuf> {
    let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::util::home_dir().join(".claude"));
    write_claude_hooks_with(data_dir, project_dir, &config_dir)
}

/// Like [`write_claude_hooks`], with the Claude configuration folder explicit (test).
pub(super) fn write_claude_hooks_with(data_dir: &Path, project_dir: &Path, config_dir: &Path) -> std::io::Result<PathBuf> {
    let dir = data_dir.join("hooks");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("claude-settings.json");
    let hook = |state: &str| serde_json::json!([{ "hooks": [{ "type": "command", "command": format!("ask-agent --signal {state}") }] }]);
    let padding = user_statusline_padding(project_dir, config_dir);
    let settings = serde_json::json!({
        // `SessionStart` also fires after `/clear` (new session id): `ask-agent --signal session` reads the id from stdin.
        // `PostToolUse` carries `tool_name` and `tool_input` on stdin: `ask-agent --signal tool` maps it to the phase.
        "hooks": {
            "UserPromptSubmit": hook("busy"),
            "Stop": hook("idle"),
            "SessionStart": hook("session"),
            "PostToolUse": hook("tool"),
        },
        "statusLine": { "type": "command", "command": "ask-agent --usage", "padding": padding },
    });
    let text = serde_json::to_string_pretty(&settings).unwrap_or_default();
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text)?;
    }
    Ok(path)
}

/// `padding` of the user's own statusline, in Claude Code's order of precedence (local project,
/// project and user). Without one of theirs, 0 (the default).
pub(super) fn user_statusline_padding(project_dir: &Path, config_dir: &Path) -> serde_json::Value {
    let files = [
        project_dir.join(".claude").join("settings.local.json"),
        project_dir.join(".claude").join("settings.json"),
        config_dir.join("settings.json"),
    ];
    files.iter().find_map(|f| user_statusline_in(f)).and_then(|sl| sl.get("padding").cloned()).unwrap_or_else(|| serde_json::Value::from(0))
}

/// `statusLine` of a Claude Code configuration file, when it is a user command (our own
/// `ask-agent --usage` does not count).
pub(super) fn user_statusline_in(file: &Path) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let sl = v.get("statusLine")?;
    if sl.get("type").and_then(serde_json::Value::as_str).is_some_and(|t| t != "command") {
        return None;
    }
    let cmd = sl.get("command")?.as_str()?.trim();
    (!cmd.is_empty() && !cmd.contains("ask-agent --usage")).then(|| sl.clone())
}

/// Name of the file (in `data/`) with the Holt instructions, for the CLIs that read instructions from a file.
pub(super) const INSTRUCTIONS_FILE: &str = "holt-instructions.md";

/// Writes `data/holt-instructions.md` (only if it changed) and returns the path. It stays outside the user's repository.
pub(super) fn write_instructions_file(data_dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join(INSTRUCTIONS_FILE);
    let text = format!(
        "{text}
"
    );
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text)?;
    }
    Ok(path)
}

/// First-message preamble passed to `q chat "<prompt>"` so Amazon Q treats the Holt block as session rules and waits.
pub(super) const Q_FIRST_MESSAGE: &str =
    "Read and follow these Holt rules for this session. Do not execute commands yet; acknowledge briefly and wait for the next prompt.";

/// Arguments that pass the Holt instructions to `kind`, without touching the user's repository. Claude
/// (`--append-system-prompt`) and OpenCode (configuration, see [`models::opencode_tui`]) are assembled separately;
/// Antigravity (`--add-dir`) and Gemini (`--include-directories`) use context folders in `data/`.
/// - Codex: `-c developer_instructions=…` (the value is TOML; in a `.cmd` shim the quotes do not survive and the text
///   goes raw, which Codex accepts as text when it is not valid TOML);
/// - Aider: `--read <file>` (read-only file);
/// - Open Interpreter: `--custom_instructions` (adds to the system prompt);
/// - Qwen Code: `--append-system-prompt <text>` (present in Qwen Code 0.15+);
/// - Amazon Q: `chat "<Q_FIRST_MESSAGE>\n\n<text>"` (no system-prompt flag, so rules go as the initial chat message).
pub(super) fn instruction_args(kind: &str, text: &str, file: Option<&Path>, shim: bool) -> Vec<String> {
    match kind {
        "codex" => {
            let value = if shim { text.to_owned() } else { serde_json::Value::from(text).to_string() };
            vec!["-c".into(), format!("developer_instructions={value}")]
        }
        "aider" => file.map(|f| vec!["--read".into(), f.to_string_lossy().into_owned()]).unwrap_or_default(),
        "interpreter" => vec!["--custom_instructions".into(), text.to_owned()],
        "qwen" => vec!["--append-system-prompt".into(), text.to_owned()],
        "q" => vec!["chat".into(), format!("{Q_FIRST_MESSAGE}\n\n{text}")],
        _ => Vec::new(),
    }
}

/// CLIs without an instructions option that only read the `AGENTS.md` of the folder they run in: without the
/// file, they do not know about `ask-agent`. For them the app creates an `AGENTS.md` with only its block when the
/// folder has none (see [`Core::ensure_agents_file`]). Antigravity (`--add-dir`) and Gemini (`--include-directories`)
/// use their own context folders in `data/` instead.
pub(super) fn reads_only_agents_md(kind: &str) -> bool {
    matches!(kind, "goose" | "copilot" | "ollama" | "mimo" | "crush" | "cline" | "amp" | "droid" | "cn" | "llama-cli" | "plandex")
        || agent_def(kind).is_none()
}

/// Writes `data/<subdir>/<id>/<filename>` with the instance instructions (only if it changed) and returns the folder.
pub(super) fn write_context_folder(data_dir: &Path, subdir: &str, filename: &str, id: &str, text: &str) -> std::io::Result<PathBuf> {
    let dir = data_dir.join(subdir).join(id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(filename);
    let text = format!("{text}\n");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text)?;
    }
    Ok(dir)
}

/// Writes `data/agy/<instance>/AGENTS.md` with the instance instructions (only if it changed) and returns the folder, for
/// Antigravity's `--add-dir`: `agy` reads the `AGENTS.md` of the added folders together with the project one (checked on
/// 2026-10-24 with 1.4.7), so nothing is created in the user's repository.
pub(super) fn write_agy_instructions(data_dir: &Path, id: &str, text: &str) -> std::io::Result<PathBuf> {
    write_context_folder(data_dir, "agy", "AGENTS.md", id, text)
}

/// Writes `data/gemini/<instance>/GEMINI.md` with the instance instructions (only if it changed) and returns the folder,
/// for Gemini CLI's `--include-directories`: Gemini loads `GEMINI.md` from all included workspace directories, so nothing
/// is created in the user's repository.
pub(super) fn write_gemini_instructions(data_dir: &Path, id: &str, text: &str) -> std::io::Result<PathBuf> {
    write_context_folder(data_dir, "gemini", "GEMINI.md", id, text)
}
