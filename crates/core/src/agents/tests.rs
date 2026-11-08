//! Registry, environment and command-line rules, with a fake PATH.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::env::build_agent_env;
use super::hooks::*;
use super::resolve::*;
use super::*;

fn registry() -> Registry {
    Registry::new(Discovery { search_path: Some(OsString::new()), home: PathBuf::from("/h"), system_dirs: false })
}

/// The instructions do not grow beyond what is needed (they were 2580 characters before t38; t40 added `--outline`,
/// t64's English text and the mail line took it to 3000, and t16 stated the task-for-every-change rule) and carry
/// the subagent rule, the task rule, the bridge and the general memory.
#[test]
fn the_instructions_carry_the_rules_and_stay_short() {
    let reg = registry();
    let text = reg.bridge_instructions(Some("claude@dev"));
    assert!(text.chars().count() <= 3300, "{} characters", text.chars().count());
    for needle in [
        "do NOT use your CLI's internal subagents (Task/Agent tool, sub-agents) unless the user asks for it",
        "ask-agent <agent>@<role> or ask-agent --task new",
        "choose among the models capable of the size of the work, and among them the one with the most free quota",
        "Unless you are already the agent of a task (role tN, brief [Holt · Task tN: …])",
        "any change to the project (bug fix, feature, validation fix, even a small tweak) must be done in a task",
        "ask-agent --task new \"title\" --start --file <request.md>",
        "only questions, explanations, investigations and checks that change no files are answered in place",
        "memory/learnings/",
        "title:",
        "summary:",
        "touches:",
        "ask-agent --learn",
        "ask-agent --outline <file>",
        "ask-agent --reply",
        "ask-agent --boss",
    ] {
        assert!(text.contains(needle), "missing {needle:?}");
    }
    assert!(!text.contains("Large work becomes a task"), "{text}");
    assert!(text.contains("Your name here is \"claude@dev\"") && text.contains("(agents: )"), "{text}");
}

/// The file text (the same for any tab) does not list agents: it does not change from machine to machine.
#[test]
fn the_generic_instructions_do_not_list_the_installed_agents() {
    let reg = registry();
    let text = reg.bridge_instructions(None);
    assert!(text.contains("HOLT_AGENT") && text.contains("HOLT_ROLE") && !text.contains("(agents:"), "{text}");
}

#[test]
fn each_cli_gets_its_own_instruction_channel() {
    let file = Path::new("/data/holt-instructions.md");
    let text = "Você \"é\"
ok";
    // Codex: TOML value (basic string) with quotes and line break escaped; in a shim, the raw text.
    assert_eq!(instruction_args("codex", text, Some(file), false), ["-c", r#"developer_instructions="Você \"é\"\nok""#]);
    assert_eq!(
        instruction_args("codex", text, Some(file), true),
        [
            "-c",
            "developer_instructions=Você \"é\"
ok"
        ]
    );
    assert_eq!(instruction_args("aider", text, Some(file), false), ["--read", "/data/holt-instructions.md"]);
    assert!(instruction_args("aider", text, None, false).is_empty());
    assert_eq!(instruction_args("interpreter", text, Some(file), false), ["--custom_instructions", text]);
    // Claude and OpenCode are assembled separately; Antigravity reads the AGENTS.md.
    for kind in ["claude", "opencode", "antigravity", "goose", "shell"] {
        assert!(instruction_args(kind, text, Some(file), false).is_empty(), "{kind}");
    }
    assert!(["goose", "copilot"].iter().all(|k| reads_only_agents_md(k)));
    assert!(["claude", "codex", "opencode", "aider", "interpreter", "shell", "antigravity"].iter().all(|k| !reads_only_agents_md(k)));
}

#[test]
fn the_instructions_file_is_rewritten_only_when_it_changes() {
    let tmp = tempfile::tempdir().expect("tmp");
    let data_dir = tmp.path().join("data");
    let path = write_instructions_file(&data_dir, "one").expect("write");
    assert_eq!(path, data_dir.join(INSTRUCTIONS_FILE));
    assert_eq!(
        std::fs::read_to_string(&path).expect("ler"),
        "one
"
    );
    let before = std::fs::metadata(&path).and_then(|m| m.modified()).expect("mtime");
    write_instructions_file(&data_dir, "one").expect("again");
    assert_eq!(std::fs::metadata(&path).and_then(|m| m.modified()).expect("mtime"), before);
    write_instructions_file(&data_dir, "two").expect("changes");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        "two
"
    );
}

#[test]
fn aliases() {
    assert_eq!(resolve_alias("agy"), "antigravity");
    assert_eq!(resolve_alias("amazon-q"), "q");
    assert_eq!(resolve_alias("claude"), "claude");
}

#[test]
fn env_is_sanitized() {
    let base = vec![
        ("CLAUDECODE".to_owned(), "1".to_owned()),
        ("CLAUDE_CODE_ENTRYPOINT".to_owned(), "cli".to_owned()),
        ("HOME".to_owned(), "/h".to_owned()),
        ("PATH".to_owned(), "/usr/bin".to_owned()),
        // Inherited from a Holt that opened this one (app running inside a tab).
        ("HOLT_ROLE".to_owned(), "dev".to_owned()),
        ("HOLT_HOME".to_owned(), "/outro".to_owned()),
        ("HOLT_TOKEN".to_owned(), "old".to_owned()),
        ("HOLT_DEBUG_UI".to_owned(), "1".to_owned()),
        // User's configuration: still valid.
        ("HOLT_ASK_WAIT".to_owned(), "300".to_owned()),
        // Model of the OpenCode tab that opened this app: it does not reach the agents.
        ("OPENCODE_CONFIG_CONTENT".to_owned(), r#"{"model":"herdado"}"#.to_owned()),
    ];
    let env = build_agent_env(base, Path::new("/app/bin"), 1234, "tok", "claude", 1);
    for k in ["HOLT_ROLE", "HOLT_HOME", "HOLT_DEBUG_UI"] {
        assert!(!env.contains_key(k), "{k} inherited");
    }
    assert_eq!(env["HOLT_TOKEN"], "tok");
    assert_eq!(env["HOLT_ASK_WAIT"], "300");
    assert!(!env.contains_key("CLAUDECODE"));
    assert!(!env.contains_key("CLAUDE_CODE_ENTRYPOINT"));
    assert!(!env.contains_key("OPENCODE_CONFIG_CONTENT"), "the configuration of the OpenCode tab that opened the app leaked");
    assert_eq!(env["HOME"], "/h");
    assert_eq!(env["PATH"], format!("/app/bin{PATH_SEP}/usr/bin"));
    assert_eq!(env["HOLT_PORT"], "1234");
    assert_eq!(env["HOLT_TOKEN"], "tok");
    assert_eq!(env["HOLT_AGENT"], "claude");
    assert_eq!(env["HOLT_DEPTH"], "1");
    assert_eq!(env["TERM"], "xterm-256color");
    assert_eq!(env["COLORTERM"], "truecolor");
}

#[test]
fn cmd_args_are_escaped_not_mangled() {
    // With a space portable-pty adds quotes: only `%`, quotes and line breaks change.
    assert_eq!(cmd_safe_arg("a \"b\" & calc | x <y> ^z %PATH% !v!\r\nfim\n"), "a b & calc | x <y> ^z %%cd:~,%PATH%%cd:~,% !v! fim ");
    assert_eq!(cmd_safe_arg(r"C:\Users\ana\data"), r"C:\Users\ana\data");
    // Without a space, without quotes: carets for the two cmd passes.
    assert_eq!(cmd_safe_arg(r"C:\Users\Ana&Bia\data"), r"C:\Users\Ana^^^&Bia\data");
    assert_eq!(cmd_safe_arg("Bash(ask-agent:*)"), "Bash^^^(ask-agent:*^^^)");
    assert_eq!(cmd_safe_arg("50%!"), "50%%cd:~,%!");
}

/// Command line as portable-pty assembles it (`append_quoted`) for arguments without quotes.
#[cfg(windows)]
fn pty_quoted(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_owned();
    }
    let trailing = arg.len() - arg.trim_end_matches('\\').len();
    format!("\"{arg}{}\"", "\\".repeat(trailing))
}

/// Paths and app text go through the `cmd.exe /c` and the `%*` of a shim (the two npm
/// formats: single line and `IF … ( … ) ELSE ( … )` block) without breaking or expanding anything.
#[cfg(windows)]
#[test]
fn escaped_args_reach_the_shim_intact() {
    use std::os::windows::process::CommandExt;
    let tmp = tempfile::tempdir().expect("tmp");
    std::fs::write(tmp.path().join("line.cmd"), "@ECHO off\r\nSETLOCAL\r\necho(%*\r\n").expect("shim");
    std::fs::write(tmp.path().join("block.cmd"), "@IF EXIST \"%~dp0\\nothing.exe\" (\r\n  echo(%*\r\n) ELSE (\r\n  echo(%*\r\n)\r\n")
        .expect("shim");
    let args = [
        r"C:\Users\Ana&Bia\data",
        r"C:\Users\Ana Bia&X (1)\data\",
        "Bash(ask-agent:*)",
        "50%PATH%!PATH!^x|y<z>w",
        "text with %PATH% & quotes \"x\" | end",
    ];
    let expected: Vec<String> = args.iter().map(|a| pty_quoted(&a.replace('"', ""))).collect();
    for shim in ["line", "block"] {
        let mut tail: Vec<String> = CMD_FLAGS.map(String::from).to_vec();
        tail.push(format!(r".\{shim}")); // without depending on NoDefaultCurrentDirectoryInExePath
        tail.extend(args.iter().map(|a| pty_quoted(&cmd_safe_arg(a))));
        let out = std::process::Command::new("cmd.exe")
            .raw_arg(tail.join(" "))
            .current_dir(tmp.path())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .output()
            .expect("cmd");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(stdout.trim_end_matches(['\r', '\n']), expected.join(" "), "{shim}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

#[test]
fn a_cmd_shim_is_called_directly_by_std() {
    let r = Resolved::via_cmd(PathBuf::from(r"C:\npm\gh.cmd"), "gh", &["copilot"]);
    assert_eq!(r.program, PathBuf::from("cmd.exe"));
    assert_eq!(r.pre, ["/d", "/e:on", "/v:off", "/c", "gh", "copilot"]);
    assert_eq!(r.command(), (PathBuf::from(r"C:\npm\gh.cmd"), vec!["copilot".to_owned()]));
    let d = Resolved::direct(PathBuf::from("/bin/claude"));
    assert_eq!(d.command(), (PathBuf::from("/bin/claude"), Vec::new()));
}

#[test]
fn claude_hooks_signal_the_bridge() {
    let tmp = tempfile::tempdir().expect("tmp");
    let path = write_claude_hooks_with(&tmp.path().join("data"), &tmp.path().join("project"), &tmp.path().join("claude")).expect("hooks");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("ler")).expect("json");
    assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "ask-agent --signal idle");
    assert_eq!(v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"], "ask-agent --signal busy");
    // `/clear` starts a new conversation: the hook reports its id (the JSON arrives on stdin).
    assert_eq!(v["hooks"]["SessionStart"][0]["hooks"][0]["command"], "ask-agent --signal session");
    assert_eq!(v["hooks"]["PostToolUse"][0]["hooks"][0]["command"], "ask-agent --signal tool");
    assert_eq!(v["statusLine"], serde_json::json!({ "type": "command", "command": "ask-agent --usage", "padding": 0 }));
    assert_eq!(
        write_claude_hooks_with(&tmp.path().join("data"), &tmp.path().join("project"), &tmp.path().join("claude")).expect("again"),
        path
    );
}

#[test]
fn claude_hooks_keep_the_user_statusline_padding() {
    let tmp = tempfile::tempdir().expect("tmp");
    let data = tmp.path().join("data");
    let project = tmp.path().join("project");
    let config = tmp.path().join("claude");
    let statusline =
        |padding: serde_json::Value| serde_json::json!({ "statusLine": { "type": "command", "command": "my-status", "padding": padding } });
    let write = |file: &Path, json: &serde_json::Value| {
        std::fs::create_dir_all(file.parent().expect("pai")).expect("mkdir");
        std::fs::write(file, json.to_string()).expect("write");
    };
    let statusline_padding = || {
        let path = write_claude_hooks_with(&data, &project, &config).expect("hooks");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("ler")).expect("json");
        v["statusLine"]["padding"].clone()
    };

    // Without the user's statusline: 0, as before.
    assert_eq!(statusline_padding(), serde_json::json!(0));

    // The user padding is the one of our statusline (Claude Code applies it).
    write(&config.join("settings.json"), &statusline(serde_json::json!(2)));
    assert_eq!(statusline_padding(), serde_json::json!(2));

    // The local project wins over the user one.
    write(&project.join(".claude").join("settings.local.json"), &statusline(serde_json::json!(1)));
    assert_eq!(statusline_padding(), serde_json::json!(1));

    // Our own statusline (`ask-agent --usage`) does not count as the user's.
    write(
        &project.join(".claude").join("settings.local.json"),
        &serde_json::json!({ "statusLine": { "type": "command", "command": "ask-agent --usage", "padding": 7 } }),
    );
    assert_eq!(statusline_padding(), serde_json::json!(2));
}

/// An npm global install of Codex: `<npm>/codex.cmd` and the platform package under `node_modules`.
fn fake_codex_install(npm: &Path, nested: bool) -> PathBuf {
    let triple = if cfg!(target_arch = "aarch64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" };
    let platform = if cfg!(target_arch = "aarch64") { "codex-win32-arm64" } else { "codex-win32-x64" };
    let package = npm.join("node_modules").join("@openai").join("codex");
    let vendor = if nested {
        package.join("node_modules").join("@openai").join(platform).join("vendor")
    } else {
        npm.join("node_modules").join("@openai").join(platform).join("vendor")
    };
    let bin = vendor.join(triple).join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir");
    std::fs::write(npm.join("codex.cmd"), "@echo off\r\n").expect("shim");
    std::fs::write(bin.join("codex.exe"), "").expect("exe");
    bin.join("codex.exe")
}

#[test]
fn the_codex_npm_shim_resolves_to_the_native_executable() {
    // Where npm puts the platform package (nested under `@openai/codex`, as `codex.js` looks first)...
    let tmp = tempfile::tempdir().expect("tmp");
    let exe = fake_codex_install(tmp.path(), true);
    assert_eq!(codex_native_exe(&tmp.path().join("codex.cmd")), Some(exe));
    // ...or hoisted next to it.
    let tmp = tempfile::tempdir().expect("tmp");
    let exe = fake_codex_install(tmp.path(), false);
    assert_eq!(codex_native_exe(&tmp.path().join("codex.cmd")), Some(exe));
    // Nothing there (a different layout): the shim stays, with no `notify`.
    let tmp = tempfile::tempdir().expect("tmp");
    std::fs::write(tmp.path().join("codex.cmd"), "").expect("shim");
    assert_eq!(codex_native_exe(&tmp.path().join("codex.cmd")), None);
}

#[cfg(windows)]
#[test]
fn on_windows_the_codex_shim_is_launched_as_the_executable_so_it_gets_the_hook() {
    let tmp = tempfile::tempdir().expect("tmp");
    let npm = tmp.path().join("npm");
    let exe = fake_codex_install(&npm, true);
    let reg = Registry::new(Discovery { search_path: Some(npm.clone().into_os_string()), home: tmp.path().join("h"), system_dirs: false });
    let r = reg.resolve("codex").expect("codex");
    assert!(r.shim.is_none() && r.program == exe && r.pre.is_empty(), "{r:?}");
    // Without the platform package it is still the shim, through cmd.exe.
    std::fs::remove_dir_all(npm.join("node_modules")).expect("rm");
    let r = reg.resolve("codex").expect("codex");
    assert_eq!(r.shim, Some(npm.join("codex.cmd")));
}

#[test]
fn the_codex_notify_hook_is_ask_agent_by_its_path_when_the_app_has_it() {
    let tmp = tempfile::tempdir().expect("tmp");
    let toml = |bin: &Path| codex_notify_arg(bin).trim_start_matches("notify=").to_owned();
    // No `ask-agent` next to the app (tests, a partial install): the bare name, resolved through the tab's PATH.
    let value: serde_json::Value = serde_json::from_str(&toml(tmp.path())).expect("array");
    assert_eq!(value, serde_json::json!(["ask-agent", "--signal", "idle"]));
    let exe = tmp.path().join(if cfg!(windows) { "ask-agent.exe" } else { "ask-agent" });
    std::fs::write(&exe, "").expect("exe");
    let value: serde_json::Value = serde_json::from_str(&toml(tmp.path())).expect("array");
    assert_eq!(value, serde_json::json!([exe.to_string_lossy(), "--signal", "idle"]));
}

#[cfg(windows)]
#[test]
fn env_path_casing_windows() {
    let base = vec![("Path".to_owned(), "C:\\Windows".to_owned())];
    let env = build_agent_env(base, Path::new("C:\\app"), 1, "t", "shell", 0);
    assert!(!env.contains_key("Path"));
    assert_eq!(env["PATH"], "C:\\app;C:\\Windows");
    // Git Bash does not turn "/review" into a path.
    assert_eq!((env["MSYS_NO_PATHCONV"].as_str(), env["MSYS2_ARG_CONV_EXCL"].as_str()), ("1", "*"));
}

#[test]
fn catalog_has_twenty_ai_clis_and_unique_colors() {
    let ai_count = AGENTS.iter().filter(|a| a.id != "shell").count();
    assert_eq!(ai_count, 20);
    assert!(agent_def("ollama").is_some_and(|a| a.label == "Ollama" && a.peer));
    assert!(agent_def("mimo").is_some_and(|a| a.label == "Mimo Code" && a.peer));
    let mut seen = std::collections::HashSet::new();
    for a in AGENTS {
        let lower = a.color.to_ascii_lowercase();
        assert!(seen.insert(lower.clone()), "duplicate color in AGENTS: {} ({})", a.id, a.color);
    }
    // Automatically generated colors for added CLIs never repeat any fixed or already assigned color.
    let mut custom_used: Vec<String> = Vec::new();
    for _ in 0..30 {
        let refs: Vec<&str> = custom_used.iter().map(String::as_str).collect();
        let c = next_agent_color(&refs);
        let lower = c.to_ascii_lowercase();
        assert!(seen.insert(lower), "generated color collided: {c}");
        custom_used.push(c);
    }
}

#[test]
fn local_models_and_command_splitting_are_parsed() {
    let ollama_out = "NAME                        ID              SIZE      MODIFIED\nqwen2.5-coder:7b            2b0496514337    4.7 GB    2 hours ago\nllama3.2:latest             a80c4f17acd5    2.0 GB    5 days ago\n";
    assert_eq!(parse_ollama_list(ollama_out).as_deref(), Some("qwen2.5-coder:7b"));
    assert_eq!(parse_ollama_list("NAME   ID   SIZE   MODIFIED\n"), None);

    assert_eq!(parse_mimo_model(r#"{"model": "mimo-2.6-9b"}"#).as_deref(), Some("mimo-2.6-9b"));
    assert_eq!(parse_mimo_model("model = \"mimo-2.6-9b-q4\"\n").as_deref(), Some("mimo-2.6-9b-q4"));
    assert_eq!(parse_mimo_model("mimo-2.6-9b\n").as_deref(), Some("mimo-2.6-9b"));

    assert_eq!(
        split_command(r#""C:\Program Files\MyCLI\my-cli.exe" --model qwen3 --prompt "hello world""#),
        Some((
            r"C:\Program Files\MyCLI\my-cli.exe".to_owned(),
            vec!["--model".into(), "qwen3".into(), "--prompt".into(), "hello world".into()]
        ))
    );
    assert_eq!(extract_command_model("ollama run deepseek-r1:8b").as_deref(), Some("deepseek-r1:8b"));
    assert_eq!(extract_command_model("llama-cli -m /models/qwen2.5-7b-instruct.gguf -cnv").as_deref(), Some("qwen2.5-7b-instruct"));
    assert_eq!(extract_command_model("my-agent --model=phi4-mini").as_deref(), Some("phi4-mini"));
}

#[cfg(unix)]
#[test]
fn ollama_and_mimo_are_detected_with_their_local_models() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tmp");
    let bin = tmp.path().join("bin");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&bin).expect("bin");
    std::fs::create_dir_all(home.join(".mimo")).expect("mimo dir");

    let ollama_bin = bin.join("ollama");
    std::fs::write(
        &ollama_bin,
        "#!/bin/sh\nif [ \"$1\" = \"list\" ]; then\n  printf 'NAME ID SIZE MODIFIED\\nqwen2.5-coder:7b abc 4.7GB now\\n'\nfi\n",
    )
    .expect("write ollama");
    std::fs::set_permissions(&ollama_bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let mimo_bin = bin.join("mimo");
    std::fs::write(&mimo_bin, "#!/bin/sh\nexit 0\n").expect("write mimo");
    std::fs::set_permissions(&mimo_bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(home.join(".mimo").join("config.json"), r#"{"model":"mimo-2.6-9b"}"#).expect("mimo config");

    let reg = Registry::new(Discovery { search_path: Some(bin.into_os_string()), home, system_dirs: false });
    let list = reg.list();
    let ollama = list.iter().find(|a| a.id == "ollama").expect("ollama");
    assert!(ollama.installed);
    assert_eq!(ollama.label, "Ollama");
    assert_eq!(ollama.local_model.as_deref(), Some("qwen2.5-coder:7b"));

    let mimo = list.iter().find(|a| a.id == "mimo").expect("mimo");
    assert!(mimo.installed);
    assert_eq!(mimo.label, "Mimo Code");
    assert_eq!(mimo.local_model.as_deref(), Some("mimo-2.6-9b"));
}
