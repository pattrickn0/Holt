//! The interactive command of a tab: program, arguments, environment, cwd and model.

use crate::instances::Ident;
use crate::models;
use crate::resume;
use crate::t;
use crate::Core;

use super::hooks::*;
use super::resolve::codex_notify_arg;
use super::*;

/// Assembles the command of an interactive tab (equivalent to `pty:spawn`).
pub(crate) fn spawn_spec(core: &Core, project: &str, kind: &str) -> Result<SpawnSpec, String> {
    spawn_spec_for(core, project, &Ident::main(kind))
}

/// Like [`spawn_spec`], for an instance: `HOLT_AGENT` gets its id and the role goes
/// in `HOLT_ROLE` and in the system prompt. The terminal opens in the folder of project `project`
/// (on the first tab of a project that is not the active one, its AGENTS.md is written).
pub(crate) fn spawn_spec_for(core: &Core, project: &str, ident: &Ident) -> Result<SpawnSpec, String> {
    let project_dir = core.project_path(project).ok_or_else(|| t!("Unknown project: {project}", project))?;
    let kind = ident.kind.as_str();
    let id = ident.id();
    // A task instance works in its own worktree, not in the project folder.
    let cwd = match core.task_cwd(project, &id) {
        Some(dir) if dir.is_dir() => dir,
        Some(dir) => return Err(t!("The task folder no longer exists ({dir}). Discard the task on the tasks desk.", dir = dir.display())),
        None => project_dir.clone(),
    };
    let reg = &core.inner.agents;
    let custom_agents = core.inner.config.read().custom_agents.clone();
    let def = agent_def(kind);
    let custom = if def.is_none() { custom_agents.iter().find(|c| c.id == kind).cloned() } else { None };
    if def.is_none() && custom.is_none() {
        return Err(t!("Unknown agent: {kind}", kind));
    }
    let data_dir = core.paths().data_dir.to_string_lossy().into_owned();
    // No CLI depends on the user's `AGENTS.md`: each one gets the instructions through the channel it has.
    let instructions = reg.bridge_instructions_for_with(ident, &custom_agents);
    let generic_file = write_instructions_file(&core.paths().data_dir, &reg.bridge_instructions(None))
        .map_err(|e| log::warn!("could not write the Holt instructions file: {e}"))
        .ok();
    let mut env = agent_env(core, &id, 0, &core.tab_token(project, &id));
    if let Some(role) = &ident.role {
        env.insert("HOLT_ROLE".into(), role.clone());
    }

    // Requested > saved > role default; it is saved for the interface and for reopening the app.
    let mut model = core.assign_model(project, ident, None);
    let (label, color, resolved) = match (def, &custom) {
        (Some(d), _) => {
            let r = reg.resolve(kind).ok_or_else(|| d.install_hint().to_owned())?;
            if model.is_none() {
                model = reg.detect_local_model(kind, &r);
            }
            (d.label.to_owned(), d.color.to_owned(), r)
        }
        (None, Some(c)) => {
            let r = reg.resolve_command_or_fallback(&c.command).ok_or_else(|| t!("Unknown agent: {kind}", kind))?;
            if model.is_none() {
                model = reg.detect_custom_local_model(&c.command);
            }
            (c.label.clone(), c.color.clone(), r)
        }
        (None, None) => unreachable!(),
    };
    let mut args = Vec::new();
    // Initial prompt of a newly opened task (see `Core::set_boot_prompt`). In Claude it is positional and goes
    // first: `--add-dir` and `--allowedTools` accept several values and would swallow the text.
    let boot = models::boot_prompt_flag(kind).and_then(|flag| core.take_boot_prompt(project, &id).map(|p| (flag, p)));
    if let Some((None, prompt)) = &boot {
        args.push(prompt.clone());
    }
    // The tab's own conversation (never "the last one": see `resume.rs`). OpenCode's goes next to `--standalone` below.
    let launch = core.plan_launch(project, ident);
    let resumed = launch.as_ref().is_some_and(|l| l.resumed && !l.args.is_empty());
    let launch_args = || launch.iter().flat_map(|l| l.args.iter().cloned());
    let mut session_log = None;
    if kind == "codex" {
        // `resume <id>` is a subcommand: before every other option (Codex has no initial-prompt flag, so nothing precedes it).
        args.extend(launch_args());
    }
    if kind == "claude" {
        args.extend(launch_args());
        args.push("--append-system-prompt".into());
        args.push(instructions.clone());
        // Answering another agent means running `ask-agent`: allow only that command, without asking.
        args.push("--allowedTools".into());
        args.push("Bash(ask-agent:*)".into());
        // Hooks tell the start and the end of each turn (see `delivery.rs`).
        match write_claude_hooks(&core.paths().data_dir, &project_dir) {
            Ok(path) => {
                args.push("--settings".into());
                args.push(path.to_string_lossy().into_owned());
            }
            Err(e) => log::warn!("could not write the Claude hooks: {e}"),
        }
    }
    if kind == "codex" && resolved.shim.is_none() {
        // Codex end of turn → `ask-agent --signal idle <json>`; the JSON carries the thread id that `codex resume` takes.
        // Through cmd.exe the TOML value (with quotes) does not survive, so only with the direct executable.
        args.push("-c".into());
        args.push(codex_notify_arg(&core.paths().bin_dir));
    }
    args.extend(instruction_args(kind, &instructions, generic_file.as_deref(), resolved.shim.is_some()));
    // Without the "Do you trust this folder?" question, which left the tab opened in the background stuck.
    args.extend(crate::trust::prepare(kind, &cwd, &project_dir, &core.inner.home));
    if kind == "antigravity" {
        // The conversation id is not ours to choose: `agy` names it in its log (see `resume::agy_session_in_log`). A new
        // process truncates the file, but a stale one is removed anyway so no old id is ever read.
        let log = resume::agy_log_path(&core.paths().data_dir, &id);
        if std::fs::create_dir_all(log.parent().unwrap_or(&core.paths().data_dir))
            .and_then(|()| match std::fs::remove_file(&log) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            })
            .is_ok()
        {
            args.extend(launch_args());
            args.push("--log-file".into());
            args.push(log.to_string_lossy().into_owned());
            session_log = Some(log);
        } else {
            log::warn!("could not prepare the Antigravity log {}: this tab will not resume", log.display());
        }
    }
    if kind == "claude" || kind == "antigravity" {
        args.push("--add-dir".into());
        args.push(data_dir);
    }
    if kind == "antigravity" {
        match write_agy_instructions(&core.paths().data_dir, &id, &instructions) {
            Ok(dir) => {
                args.push("--add-dir".into());
                args.push(dir.to_string_lossy().into_owned());
            }
            Err(e) => log::warn!("could not write the Antigravity instructions: {e}"),
        }
    }
    if kind == "gemini" {
        match write_gemini_instructions(&core.paths().data_dir, &id, &instructions) {
            Ok(dir) => {
                args.push("--include-directories".into());
                args.push(dir.to_string_lossy().into_owned());
            }
            Err(e) => log::warn!("could not write the Gemini instructions: {e}"),
        }
    }
    let effort = core.instance_effort(project, ident);
    if kind == "opencode" {
        // The TUI has no --model or --system: its own server + model and instructions in the configuration
        // (see `models::opencode_tui`).
        let (extra, (key, value)) = models::opencode_tui(model.as_deref(), generic_file.as_deref());
        args.extend(extra);
        args.extend(launch.iter().flat_map(|l| l.args.iter().cloned()));
        env.insert(key, value);
    } else if kind == "ollama" {
        if let Some(m) = &model {
            args.push("run".into());
            args.push(m.clone());
        }
    } else if let Some(m) = &model {
        args.extend(models::cli_args(kind, m));
    }
    // Task effort (Antigravity's is already in the model name; see `Task::effort`).
    if let Some(effort) = effort {
        args.extend(models::effort_args(kind, effort));
    }
    if let Some((Some(flag), prompt)) = boot {
        args.push(flag.into());
        args.push(prompt);
    }
    if resolved.shim.is_some() {
        args = args.iter().map(|a| cmd_safe_arg(a)).collect();
    }
    let mut full = resolved.pre;
    full.extend(args);

    core.ensure_project_context(project, &project_dir);
    if reads_only_agents_md(kind) {
        core.ensure_agents_file(project, &cwd);
    }
    Ok(SpawnSpec {
        program: resolved.program,
        args: full,
        cwd,
        env,
        label,
        color,
        kind: kind.to_owned(),
        double_enter_on_mention: kind == "claude" || kind == "antigravity",
        model,
        resumed: resumed && (kind != "antigravity" || session_log.is_some()),
        session_log,
        session: launch.as_ref().filter(|l| l.resumed).and_then(|l| l.id.clone()),
    })
}
