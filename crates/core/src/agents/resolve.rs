//! Resolution of the executable behind an id or a user command, cached where it costs a process.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::instances::Ident;

use super::*;
pub fn agent_def(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

/// Resolves an alias (`agy` → `antigravity`); unknown names come back unchanged.
pub fn resolve_alias(name: &str) -> &str {
    ALIASES.iter().find(|(a, _)| *a == name).map(|(_, t)| *t).unwrap_or(name)
}

/// Whether agent `kind` can run `ask-agent --reply` autonomously in its tab.
pub fn can_answer(kind: &str) -> bool {
    agent_def(resolve_alias(kind)).is_some_and(|a| a.can_answer)
}

/// Whether a user-added CLI command can run `ask-agent --reply` autonomously in its tab.
pub fn command_can_answer(command: &str) -> bool {
    let Some((prog, _)) = split_command(command) else { return false };
    let stem = Path::new(&prog).file_stem().and_then(|s| s.to_str()).unwrap_or(&prog).to_ascii_lowercase();
    let canonical = resolve_alias(&stem);
    if let Some(def) = agent_def(canonical) {
        return def.can_answer;
    }
    !matches!(canonical, "sh" | "bash" | "zsh" | "fish" | "cmd" | "powershell" | "pwsh" | "llama-server")
}

/// Like [`can_answer`], including custom CLIs from `Config::custom_agents`.
pub fn can_answer_with(kind: &str, custom: &[crate::config::CustomAgent]) -> bool {
    if let Some(def) = agent_def(resolve_alias(kind)) {
        return def.can_answer;
    }
    custom.iter().find(|c| c.id == kind).is_some_and(|c| command_can_answer(&c.command))
}

/// Error message when a caller tries to `ask-agent <kind> "..."` an agent that cannot run `ask-agent --reply`.
pub fn cannot_answer_error(kind: &str, alternatives: &[String]) -> String {
    let mut msg = format!("the agent \"{kind}\" cannot answer inter-agent questions: it has no shell tool to run `ask-agent --reply`");
    if !alternatives.is_empty() {
        msg.push_str(&format!(". Ask one that can: {}", alternatives.join(", ")));
    }
    msg
}

/// Resolved executable + initial fixed arguments (e.g. `cmd.exe /d /c claude`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Resolved {
    pub program: PathBuf,
    pub pre: Vec<String>,
    /// Windows: the agent is a `.cmd` shim (npm/pip), opened via `cmd.exe /d /c <name>`. cmd.exe
    /// interprets `& | < > ^ % "` and line breaks in the arguments: no free text in them.
    pub shim: Option<PathBuf>,
}

impl Resolved {
    pub(super) fn direct(p: PathBuf) -> Self {
        Self { program: p, pre: Vec::new(), shim: None }
    }

    /// `cmd.exe /d /e:on /v:off /c <name> [extra…]` for the shim at `path`. Extensions enabled and
    /// delayed expansion off even if the registry says otherwise: the escaping of
    /// [`cmd_safe_arg`] relies on that (`%cd:~,%` and a literal `!`).
    pub(super) fn via_cmd(path: PathBuf, name: &str, extra: &[&str]) -> Self {
        let mut pre: Vec<String> = CMD_FLAGS.map(String::from).to_vec();
        pre.push(name.to_owned());
        pre.extend(extra.iter().map(|s| s.to_string()));
        Self { program: "cmd.exe".into(), pre, shim: Some(path) }
    }

    /// Program and fixed arguments for `std::process::Command`. A `.cmd` shim is called
    /// directly: that way Rust's std applies its own batch-file escaping (and refuses what
    /// cannot be escaped), which does not happen with an explicit `cmd.exe /c`.
    pub fn command(&self) -> (PathBuf, Vec<String>) {
        match &self.shim {
            Some(path) => (path.clone(), self.pre.iter().skip(CMD_FLAGS.len() + 1).cloned().collect()),
            None => (self.program.clone(), self.pre.clone()),
        }
    }
}

/// The native `codex.exe` behind the npm shim `shim` (`<npm>\codex.cmd`). The shim runs `node bin/codex.js`, which spawns
/// the executable of the platform package (`node_modules/@openai/codex-win32-x64/vendor/<triple>/bin/codex.exe`, or
/// `vendor/<triple>/bin` inside `@openai/codex` itself): the same places, in the same order, as `codex.js`.
pub(crate) fn codex_native_exe(shim: &Path) -> Option<PathBuf> {
    let package = shim.parent()?.join("node_modules").join("@openai").join("codex");
    let (platform, triple) = match std::env::consts::ARCH {
        "aarch64" => ("codex-win32-arm64", "aarch64-pc-windows-msvc"),
        _ => ("codex-win32-x64", "x86_64-pc-windows-msvc"),
    };
    [
        package.join("node_modules").join("@openai").join(platform).join("vendor"),
        shim.parent()?.join("node_modules").join("@openai").join(platform).join("vendor"),
        package.join("vendor"),
    ]
    .into_iter()
    .map(|vendor| vendor.join(triple).join("bin").join("codex.exe"))
    .find(|exe| exe.is_file())
}

/// The `notify` setting of Codex: its program is `ask-agent` by its full path when the app's folder has it. A bare name was
/// not found by Codex on Windows (checked with 0.164: the same command with the full path ran).
pub(super) fn codex_notify_arg(bin_dir: &Path) -> String {
    let exe = bin_dir.join(if cfg!(windows) { "ask-agent.exe" } else { "ask-agent" });
    let program = if exe.is_file() { exe.to_string_lossy().into_owned() } else { "ask-agent".to_owned() };
    // A JSON array of strings is also a TOML array.
    format!("notify={}", serde_json::json!([program, "--signal", "idle"]))
}

/// `cmd.exe` options that open a `.cmd` shim in a tab (before the shim name).
const CMD_FLAGS: [&str; 4] = ["/d", "/e:on", "/v:off", "/c"];

/// Escapes an argument for the `cmd.exe /c <shim>` of the tabs on Windows. The text goes through two
/// cmd interpreters: the `cmd.exe /c` line and the shim line that forwards `%*`.
/// - portable-pty quotes the argument with a space or tab (and only it): inside quotes,
///   `& | < > ^ ( )` are already literal in both passes;
/// - without quotes, each of those becomes `^^^c` (the first pass leaves `^c`, the second, `c`);
/// - `%` becomes `%%cd:~,%` (like Rust's std), which the command line expands to `%` and is not
///   expanded again in `%*`; `!` stays literal with `/v:off`;
/// - quotes are removed (they do not appear in paths) and line breaks become a space.
///
/// Only for text generated by the app (instructions, paths such as `C:\Users\Ana&Bia`): free text
/// from other agents never goes through a shim's command line.
pub(crate) fn cmd_safe_arg(arg: &str) -> String {
    let arg: String = arg.replace("\r\n", " ").replace(['\r', '\n'], " ").chars().filter(|&c| c != '"').collect();
    let quoted = arg.is_empty() || arg.contains([' ', '\t', '\x0b']);
    let mut out = String::with_capacity(arg.len());
    for c in arg.chars() {
        match c {
            '%' => out.push_str("%%cd:~,%"),
            '&' | '|' | '<' | '>' | '^' | '(' | ')' if !quoted => {
                out.push_str("^^^");
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// Discovery options (replaceable for tests/embedding).
#[derive(Debug, Clone)]
pub(crate) struct Discovery {
    pub search_path: Option<OsString>,
    pub home: PathBuf,
    /// Look for CLIs in the system folders (`APPDATA`, `LOCALAPPDATA`, `ProgramFiles`). Off
    /// in tests: those folders are then looked up inside `home`.
    pub system_dirs: bool,
}

const COPILOT_CACHE_TTL: Duration = Duration::from_secs(30);
const LOCAL_MODEL_CACHE_TTL: Duration = Duration::from_secs(30);

/// Registry state: Copilot and local-model detection caches.
pub(crate) struct Registry {
    disc: Discovery,
    copilot: Mutex<Option<(Instant, Option<Resolved>)>>,
    local_models: Mutex<BTreeMap<String, (Instant, Option<String>)>>,
}

impl Registry {
    pub fn new(disc: Discovery) -> Self {
        Self { disc, copilot: Mutex::new(None), local_models: Mutex::new(BTreeMap::new()) }
    }

    pub(crate) fn which(&self, name: &str) -> Option<PathBuf> {
        let found = match &self.disc.search_path {
            Some(p) => which::which_in(name, Some(p), std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            None => which::which(name),
        };
        found.ok()
    }

    fn home(&self) -> &Path {
        &self.disc.home
    }

    /// System folder from the `key` variable (`APPDATA`…). In tests (`system_dirs = false`) it sits
    /// inside `home`, so the result does not depend on what is installed on the machine.
    pub(crate) fn system_dir(&self, key: &str) -> Option<PathBuf> {
        if self.disc.system_dirs {
            std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
        } else {
            Some(self.home().join(key))
        }
    }

    // ---------- Executable resolution ----------

    /// Resolves a base agent.
    pub fn resolve(&self, id: &str) -> Option<Resolved> {
        match id {
            "copilot" => self.resolve_copilot(),
            "shell" => Some(shell_resolved()),
            _ if agent_def(id).is_some() => self.resolve_cli(id),
            _ => None,
        }
    }

    /// Resolves a user-entered command (`program + args`) to a [`Resolved`], falling back to the literal
    /// program token if it is not found in `PATH` (so a custom CLI always produces a valid [`SpawnSpec`]).
    pub(crate) fn resolve_command_or_fallback(&self, command: &str) -> Option<Resolved> {
        let (program, extra) = split_command(command)?;
        let mut resolved = self.resolve_program(&program).unwrap_or_else(|| Resolved::direct(PathBuf::from(&program)));
        if resolved.shim.is_some() {
            resolved.pre.extend(extra.iter().map(|a| cmd_safe_arg(a)));
        } else {
            resolved.pre.extend(extra);
        }
        Some(resolved)
    }

    fn resolve_program(&self, program: &str) -> Option<Resolved> {
        let p = Path::new(program);
        if p.components().count() > 1 || p.is_absolute() {
            if p.is_file() {
                if cfg!(windows) && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd")) {
                    let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or(program);
                    return Some(Resolved::via_cmd(p.to_path_buf(), name, &[]));
                }
                return Some(Resolved::direct(p.to_path_buf()));
            }
            return None;
        }
        self.resolve_cli(program)
    }

    /// Default resolution: known candidates → `where`/`which` → `.cmd` shim via `cmd.exe`.
    fn resolve_cli(&self, id: &str) -> Option<Resolved> {
        let name = if id == "antigravity" { "agy" } else { id };
        let first_existing = |c: Vec<Option<PathBuf>>| c.into_iter().flatten().find(|p| p.is_file());
        if cfg!(windows) {
            let env = |k: &str| self.system_dir(k).unwrap_or_default();
            let home = self.home().to_path_buf();
            let exe = format!("{name}.exe");
            let which_exe = || self.which(&exe);
            let local_bin = Some(home.join(".local").join("bin").join(&exe));
            let candidates = match id {
                "claude" => vec![
                    Some(
                        env("APPDATA")
                            .join("npm")
                            .join("node_modules")
                            .join("@anthropic-ai")
                            .join("claude-code")
                            .join("bin")
                            .join("claude.exe"),
                    ),
                    local_bin,
                    which_exe(),
                ],
                "antigravity" => {
                    let local = env("LOCALAPPDATA");
                    vec![
                        which_exe(),
                        Some(local.join("agy").join("bin").join("agy.exe")),
                        local_bin,
                        Some(local.join("Programs").join("agy").join("agy.exe")),
                        Some(local.join("Programs").join("Antigravity CLI").join("agy.exe")),
                        Some(local.join("antigravity-cli").join("bin").join("agy.exe")),
                        Some(home.join(".antigravity").join("bin").join("agy.exe")),
                    ]
                }
                "aider" => vec![
                    which_exe(),
                    Some(env("APPDATA").join("Python").join("Python311").join("Scripts").join("aider.exe")),
                    Some(env("APPDATA").join("Python").join("Python312").join("Scripts").join("aider.exe")),
                    local_bin,
                ],
                "q" => vec![which_exe(), Some(env("PROGRAMFILES").join("Amazon").join("Q").join("q.exe")), local_bin],
                "ollama" => vec![which_exe(), Some(env("LOCALAPPDATA").join("Programs").join("Ollama").join("ollama.exe")), local_bin],
                _ => vec![which_exe(), local_bin],
            };
            if let Some(p) = first_existing(candidates) {
                return Some(Resolved::direct(p));
            }
            // npm/pip shims (`.cmd`) must go through cmd.exe.
            let shim =
                if id == "antigravity" { self.which("agy.cmd") } else { self.which(&format!("{name}.cmd")).or_else(|| self.which(name)) };
            shim.map(|p| {
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
                    Resolved::direct(p)
                } else if let Some(exe) = (id == "codex").then(|| codex_native_exe(&p)).flatten() {
                    // The real executable, not the shim: only then does Codex get the `notify` hook (and with it resume).
                    Resolved::direct(exe)
                } else {
                    Resolved::via_cmd(p, name, &[])
                }
            })
        } else {
            let home = self.home().to_path_buf();
            let mut candidates = vec![self.which(name), Some(home.join(".local").join("bin").join(name))];
            match id {
                "claude" => candidates.push(Some(home.join(".claude").join("local").join("claude"))),
                "antigravity" => candidates.push(Some(home.join(".antigravity").join("bin").join("agy"))),
                _ => {}
            }
            first_existing(candidates).map(Resolved::direct)
        }
    }

    /// GitHub Copilot: the standalone `copilot` CLI (`npm install -g @github/copilot`, the one with an ACP mode), else `gh`
    /// with the old `gh-copilot` extension (result cached for 30 s, because it requires running `gh extension list`).
    fn resolve_copilot(&self) -> Option<Resolved> {
        if let Some((at, r)) = self.copilot.lock().as_ref() {
            if at.elapsed() < COPILOT_CACHE_TTL {
                return r.clone();
            }
        }
        let r = self.detect_copilot();
        *self.copilot.lock() = Some((Instant::now(), r.clone()));
        r
    }

    fn detect_copilot(&self) -> Option<Resolved> {
        if let Some(standalone) = self.resolve_cli("copilot") {
            return Some(standalone);
        }
        let gh = if cfg!(windows) {
            self.which("gh.exe").or_else(|| self.which("gh.cmd")).or_else(|| self.which("gh"))
        } else {
            self.which("gh")
        }?;
        let is_cmd = gh.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd"));
        let gh_resolved = if is_cmd { Resolved::via_cmd(gh.clone(), "gh", &[]) } else { Resolved::direct(gh.clone()) };
        let (program, pre) = gh_resolved.command();
        let mut cmd = std::process::Command::new(&program);
        cmd.args(&pre).args(["extension", "list"]);
        let out = crate::process::run_captured(cmd, None, Duration::from_secs(3), None, None).ok()?;
        if out.stdout.contains("gh-copilot") || out.stdout.contains("copilot") {
            return Some(if is_cmd {
                Resolved::via_cmd(gh, "gh", &["copilot"])
            } else {
                Resolved { program: gh, pre: vec!["copilot".into()], shim: None }
            });
        }
        None
    }

    /// Queries the local model in use for a local CLI (`ollama list`, Mimo config/CLI), cached for 30 s.
    pub(crate) fn detect_local_model(&self, id: &str, resolved: &Resolved) -> Option<String> {
        if !is_local_cli(id) {
            return None;
        }
        if let Some((at, cached)) = self.local_models.lock().get(id) {
            if at.elapsed() < LOCAL_MODEL_CACHE_TTL {
                return cached.clone();
            }
        }
        let detected = match id {
            "ollama" => self.query_ollama_model(resolved),
            "mimo" => self.query_mimo_model(resolved),
            _ => None,
        };
        self.local_models.lock().insert(id.to_owned(), (Instant::now(), detected.clone()));
        detected
    }

    /// Detects the local model of a user-added CLI from its command flags or, when the binary is `ollama`/`mimo`, by querying it.
    pub(crate) fn detect_custom_local_model(&self, command: &str) -> Option<String> {
        if let Some(m) = extract_command_model(command) {
            return Some(m);
        }
        let (program, _) = split_command(command)?;
        let stem = Path::new(&program).file_stem().and_then(|s| s.to_str()).unwrap_or(&program).to_ascii_lowercase();
        if matches!(stem.as_str(), "ollama" | "mimo") {
            if let Some(resolved) = self.resolve_program(&program) {
                return self.detect_local_model(&stem, &resolved);
            }
        }
        None
    }

    fn query_ollama_model(&self, resolved: &Resolved) -> Option<String> {
        let (program, pre) = resolved.command();
        let mut cmd = std::process::Command::new(&program);
        cmd.args(&pre).arg("list");
        if let Ok(out) = crate::process::run_captured(cmd, None, Duration::from_secs(2), None, None) {
            if let Some(m) = parse_ollama_list(&out.stdout) {
                return Some(m);
            }
        }
        if self.disc.system_dirs {
            if let Some(m) =
                std::env::var_os("OLLAMA_MODEL").and_then(|v| v.into_string().ok()).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
            {
                return Some(m);
            }
        }
        None
    }

    fn query_mimo_model(&self, resolved: &Resolved) -> Option<String> {
        let home = self.home();
        let candidates = [
            home.join(".mimo").join("config.json"),
            home.join(".mimo").join("settings.json"),
            home.join(".mimo").join("config.toml"),
            home.join(".mimo").join("model"),
            home.join(".config").join("mimo").join("config.json"),
            home.join(".config").join("mimo").join("settings.json"),
            home.join(".config").join("mimo").join("config.toml"),
            home.join(".mimo.json"),
        ];
        for path in candidates {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Some(m) = parse_mimo_model(&text) {
                    return Some(m);
                }
            }
        }
        if self.disc.system_dirs {
            if let Some(m) =
                std::env::var_os("MIMO_MODEL").and_then(|v| v.into_string().ok()).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
            {
                return Some(m);
            }
        }
        let (program, pre) = resolved.command();
        for sub in ["model", "list"] {
            let mut cmd = std::process::Command::new(&program);
            cmd.args(&pre).arg(sub);
            if let Ok(out) = crate::process::run_captured(cmd, None, Duration::from_secs(2), None, None) {
                if out.exit == crate::process::Exit::Code(Some(0)) {
                    if let Some(m) = parse_mimo_model(&out.stdout) {
                        return Some(m);
                    }
                }
            }
        }
        None
    }

    // ---------- Peers and instructions ----------

    /// Only installed agents enabled as peers join the bridge.
    #[cfg(test)]
    pub fn peers(&self) -> Vec<String> {
        self.peers_with(&[])
    }

    /// Installed catalog peers plus user-added custom CLIs.
    pub fn peers_with(&self, custom: &[crate::config::CustomAgent]) -> Vec<String> {
        let mut out: Vec<String> = AGENTS.iter().filter(|a| a.peer && self.resolve(a.id).is_some()).map(|a| a.id.to_owned()).collect();
        for c in custom {
            if !c.id.is_empty() && !out.iter().any(|p| p == &c.id) {
                out.push(c.id.clone());
            }
        }
        out
    }

    /// Installed peer agents that can run `ask-agent --reply` autonomously (`can_answer`).
    #[cfg(test)]
    pub fn answering_peers(&self) -> Vec<String> {
        self.answering_peers_with(&[])
    }

    /// Installed peer agents (including custom CLIs) that can run `ask-agent --reply` autonomously (`can_answer`).
    pub fn answering_peers_with(&self, custom: &[crate::config::CustomAgent]) -> Vec<String> {
        self.peers_with(custom).into_iter().filter(|p| can_answer_with(p, custom)).collect()
    }

    /// Like [`Registry::bridge_instructions`], for the instance `ident`, with its role and custom CLIs. With
    /// `orchestrator`, the instance is the project's designated Orchestrator and is told so (a regular agent that only
    /// opens helpers is not).
    pub fn bridge_instructions_for_with(&self, ident: &Ident, custom: &[crate::config::CustomAgent], orchestrator: bool) -> String {
        let mut text = self.bridge_instructions_with(Some(&ident.id()), custom);
        if let Some(role) = &ident.role {
            text.push_str(&format!("\nYour role in this instance: {role}. Act accordingly."));
        }
        if orchestrator {
            text.push('\n');
            text.push_str(crate::team::orchestrator_note());
        }
        text
    }

    /// The Holt instructions, the "general memory" shared by every agent. With `self_id`, it carries the
    /// instance name and the list of the other agents; without it (file text, the same for any tab), it tells the
    /// agent to read the name from the environment variable and lists nobody, so the file does not change from machine to machine.
    pub fn bridge_instructions(&self, self_id: Option<&str>) -> String {
        self.bridge_instructions_with(self_id, &[])
    }

    pub fn bridge_instructions_with(&self, self_id: Option<&str>, custom: &[crate::config::CustomAgent]) -> String {
        const INTRO: &str = "You are running inside Holt, an environment that connects several AI agents.";
        let (first, listed) = match self_id {
            Some(id) => {
                let others: Vec<String> = self.peers_with(custom).into_iter().filter(|p| p != id).collect();
                (format!("{INTRO} Your name here is \"{id}\"."), format!(" (agents: {})", others.join(",")))
            }
            None => {
                (format!("{INTRO} Your name and role are in the HOLT_AGENT and HOLT_ROLE variables; never ask who you are."), String::new())
            }
        };
        [
            first,
            format!("To talk to another agent, run this in the shell: ask-agent <agent> \"message\"{listed}. If you need another agent, open it in Holt (ask-agent <agent>, ask-agent <agent>@<role> or ask-agent --task new): do NOT use your CLI's internal subagents (Task/Agent tool, sub-agents) unless the user asks for it."),
            "The answer comes on stdout and can take minutes (timeout up to 600000 ms). If ask-agent warns that the answer will be typed later in this terminal, do not repeat the question; if it does not show up, run ask-agent --inbox (--all: the last 20).".to_owned(),
            "ask-agent --models [<agent>] lists the agents, this machine's models and the quota of each one (from the most free to the most spent): before delegating or opening tasks, look at everyone's quota and choose among the models capable of the size of the work, and among them the one with the most free quota, instead of always using the same one (the bridge refuses the exhausted ones). The history of each pair is kept (inside a task, the user reads it in the task's timeline): do not report or summarize the conversation; talk with the other agent until it is done, without stopping to ask the user.".to_owned(),
            "Each tab is an instance <model>[@<role>[@<name>]] (claude, claude@dev, claude@dev@ana): ask-agent claude@dev \"task\" opens the tab if it does not exist; --new opens another one with the same role; --model <model> picks the LLM when opening. Unless you are already the agent of a task (role tN, brief [Holt · Task tN: …]), any change to the project (bug fix, feature, validation fix, even a small tweak) must be done in a task, with its own worktree, branch, report, diff and review: ask-agent --task new \"title\" --start --file <request.md>; only questions, explanations, investigations and checks that change no files are answered in place.".to_owned(),
            "Text with accents goes in a file (--file <file>): through standard input the Windows shell turns accents into \"?\".".to_owned(),
            "When another agent sends you a message, answer with the command it shows (ask-agent --reply <number> \"answer\" or --file <file>): only then does the answer reach it.".to_owned(),
            "A long or multi-line message (or late answer) is not typed in full: you get a notice [mail #12 from claude@dev: \"…\"]. Read it with ask-agent --read 12 (it says how to answer); ask-agent --inbox lists unread mail.".to_owned(),
            "The agents form a team (boss and subordinates; the user can reorganize it). Before delegating or reporting, see your position with ask-agent --team; delegate preferably to the team and, when you finish something the boss asked for, report with ask-agent --boss \"message\".".to_owned(),
            "Project memory: memory/learnings/, one .md per trap, decision or convention that cost time and the code does not show. Format: header --- title: … / summary: one line / touches: [globs of the files; optional] --- and a short body, with Why and How to apply. Before touching files, run ask-agent --learn <file>... (without arguments: the index and the state of the tasks; with files, it warns if another task touches them); when you find out something like that, create the file. ask-agent --outline <file>... shows the signatures with the line, so you read only the part of a large file you need (offset).".to_owned(),
        ]
        .join("\n")
    }

    /// List for menus (equivalent to `agents:list`).
    #[cfg(test)]
    pub fn list(&self) -> Vec<AgentInfo> {
        self.list_with(&[])
    }

    /// Lists all catalog agents, any user-added custom CLIs (before `shell`), and `shell` last.
    pub fn list_with(&self, custom: &[crate::config::CustomAgent]) -> Vec<AgentInfo> {
        let mut out = Vec::with_capacity(AGENTS.len() + custom.len());
        for a in AGENTS.iter().filter(|a| a.id != "shell") {
            let resolved = self.resolve(a.id);
            let local_model = resolved.as_ref().and_then(|r| self.detect_local_model(a.id, r));
            out.push(AgentInfo {
                id: a.id.into(),
                label: a.label.into(),
                color: a.color.into(),
                installed: resolved.is_some(),
                peer: a.peer,
                can_answer: a.can_answer,
                custom: false,
                command: None,
                local_model,
            });
        }
        for c in custom {
            let local_model = self.detect_custom_local_model(&c.command);
            out.push(AgentInfo {
                id: c.id.clone(),
                label: c.label.clone(),
                color: c.color.clone(),
                installed: true,
                peer: true,
                can_answer: command_can_answer(&c.command),
                custom: true,
                command: Some(c.command.clone()),
                local_model,
            });
        }
        if let Some(sh) = AGENTS.iter().find(|a| a.id == "shell") {
            out.push(AgentInfo {
                id: sh.id.into(),
                label: sh.label.into(),
                color: sh.color.into(),
                installed: self.resolve(sh.id).is_some(),
                peer: sh.peer,
                can_answer: sh.can_answer,
                custom: false,
                command: None,
                local_model: None,
            });
        }
        out
    }
}

/// Shell of the "Terminal" tab: PowerShell on Windows; `$SHELL` (or /bin/bash) on the others.
fn shell_resolved() -> Resolved {
    if cfg!(windows) {
        // On Windows, force UTF-8 without BOM in the console and in the pipeline to keep
        // pipes to ask-agent or other executables from turning accents into "?".
        Resolved {
            program: "powershell.exe".into(),
            pre: vec![
                "-NoLogo".into(),
                "-NoExit".into(),
                "-Command".into(),
                "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)".into(),
            ],
            shim: None,
        }
    } else {
        let sh = std::env::var_os("SHELL").filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| "/bin/bash".into());
        Resolved::direct(sh)
    }
}
