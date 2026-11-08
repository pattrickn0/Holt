//! The environment of an agent process: `HOLT_*`, PATH and the variables removed from the parent.

use std::collections::BTreeMap;
use std::path::Path;

use crate::Core;

// ---------- Environment and interactive command ----------

/// The platform's PATH separator.
pub const PATH_SEP: &str = if cfg!(windows) { ";" } else { ":" };

/// Agent environment (equivalent to `agentEnv`): inherits the process one, removes marks of
/// nested Claude Code sessions, puts the binaries folder at the start of PATH and injects
/// the bridge variables.
/// `token` is the process one (see [`Core::tab_token`] and [`Core::headless_token`]), never the master.
pub(crate) fn agent_env(core: &Core, agent_id: &str, depth: i64, token: &str) -> BTreeMap<String, String> {
    let base: Vec<(String, String)> =
        std::env::vars_os().map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned())).collect();
    build_agent_env(base, &core.paths().bin_dir, core.bridge_port(), token, agent_id, depth)
}

pub(crate) fn build_agent_env(
    base: Vec<(String, String)>,
    bin_dir: &Path,
    port: u16,
    token: &str,
    agent_id: &str,
    depth: i64,
) -> BTreeMap<String, String> {
    // `OPENCODE_CONFIG_CONTENT` is the configuration (model) of the OpenCode tab that opened this process; the one
    // for each new tab is inserted after this filter (`spawn_spec_for`, via `models::opencode_tui`).
    const REMOVE: &[&str] = &[
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_SSE_PORT",
        "ELECTRON_RUN_AS_NODE",
        "OPENCODE_CONFIG_CONTENT",
    ];
    let mut env = BTreeMap::new();
    let mut old_path = String::new();
    for (k, v) in base {
        // On Windows variables are not case-sensitive ("Path" vs "PATH").
        let is_path = if cfg!(windows) { k.eq_ignore_ascii_case("PATH") } else { k == "PATH" };
        if is_path {
            if old_path.is_empty() {
                old_path = v;
            }
            continue;
        }
        let removed = if cfg!(windows) { REMOVE.iter().any(|r| r.eq_ignore_ascii_case(&k)) } else { REMOVE.contains(&k.as_str()) }
            // Nothing from the Holt that opened this app (e.g. the app running inside a tab of
            // another Holt) leaks to the agents: role, data folder, debug…
            || (crate::env::is_app_var(&k) && !crate::env::is_app_var_named(&k, "ASK_WAIT"));
        if !removed {
            env.insert(k, v);
        }
    }
    env.insert("PATH".into(), format!("{}{PATH_SEP}{old_path}", bin_dir.display()));
    env.insert("HOLT_PORT".into(), port.to_string());
    env.insert("HOLT_TOKEN".into(), token.to_owned());
    env.insert("HOLT_AGENT".into(), agent_id.to_owned());
    env.insert("HOLT_DEPTH".into(), depth.to_string());
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("COLORTERM".into(), "truecolor".into());
    if cfg!(windows) {
        env.insert("PYTHONUTF8".into(), "1".into());
        env.insert("PYTHONIOENCODING".into(), "utf-8".into());
        // Git Bash (the one from Claude Code) rewrites arguments that start with "/" as paths
        // (`ask-agent claude@dev "/review src"` would become `C:/Program Files/Git/review src`).
        env.insert("MSYS_NO_PATHCONV".into(), "1".into());
        env.insert("MSYS2_ARG_CONV_EXCL".into(), "*".into());
    }
    env
}
