//! The chat view's plumbing with the real core and a fake `claude`: the launch (structured mode, the tab's own
//! conversation), the saved conversation read back, and a process that gets a message, asks for a permission, gets the
//! answer and ends.

#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::time::{Duration, Instant};

use holt_core::chat::Item;
#[cfg(unix)]
use holt_core::chat::{self, ChatEvent, ChatMsg};
use holt_core::models::Effort;
use holt_core::Ident;
use serde_json::{json, Value};

#[path = "support/tasks_env.rs"]
mod env;
use env::*;

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

#[test]
fn the_chat_launch_uses_the_structured_mode_and_the_tabs_own_conversation() {
    let env = start_with(false, false);
    let ident = Ident::parse("claude@dev").expect("ident");
    let id = ident.id();

    let first = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    for flag in ["-p", "--include-partial-messages", "--verbose"] {
        assert!(first.args.iter().any(|a| a == flag), "{flag} in {:?}", first.args);
    }
    assert_eq!(arg_after(&first.args, "--input-format").as_deref(), Some("stream-json"));
    assert_eq!(arg_after(&first.args, "--output-format").as_deref(), Some("stream-json"));
    assert_eq!(arg_after(&first.args, "--permission-prompt-tool").as_deref(), Some("stdio"));
    // The end of a turn is an event of the stream: no hooks that call back.
    assert!(!first.args.iter().any(|a| a == "--settings"), "{:?}", first.args);
    // The answer to another agent still needs `ask-agent`.
    assert!(first.args.iter().any(|a| a == "Bash(ask-agent:*)"));
    let session = arg_after(&first.args, "--session-id").expect("a new conversation gets an id");
    assert!(!first.resumed);
    assert_eq!(env.core.tab_session(&env.project, &id).as_deref(), Some(session.as_str()));

    // The CLI saved the conversation: the next launch resumes it and the chat shows what was said.
    let dir = env.root.join("home").join(".claude").join("projects").join("-ws");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let lines = [
        json!({ "type": "user", "message": { "content": "Add a menu entry" } }),
        json!({ "type": "assistant", "message": { "id": "m1", "content": [{ "type": "text", "text": "Done." }] } }),
    ];
    let text = lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    std::fs::write(dir.join(format!("{session}.jsonl")), text).expect("transcript");
    let again = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(arg_after(&again.args, "--resume").as_deref(), Some(session.as_str()));
    assert!(again.resumed);
    let history = env.core.chat_history(&env.project, &id);
    assert_eq!(history.items.len(), 2, "{:?}", history.items);
    assert!(matches!(&history.items[1], Item::Assistant { text, .. } if text == "Done."));
    assert_eq!(history.session.as_deref(), Some(session.as_str()));

    // The terminal launch of the same tab keeps resuming the same conversation, with its hooks.
    let term = env.core.spawn_spec_for(&env.project, &ident).expect("spec");
    assert_eq!(arg_after(&term.args, "--resume").as_deref(), Some(session.as_str()));
    assert!(term.args.iter().any(|a| a == "--settings"));
    assert!(!term.args.iter().any(|a| a == "-p"));
    env.core.shutdown();
}

#[test]
fn a_task_prompt_becomes_the_first_message_instead_of_an_argument() {
    let env = start_with(false, false);
    let ident = Ident::parse("claude@t1").expect("ident");
    env.core.set_boot_prompt(&env.project, &ident.id(), "Read the brief in data/tasks/t1.md and start.".into());
    let spec = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(spec.first_message.as_deref(), Some("Read the brief in data/tasks/t1.md and start."));
    assert!(!spec.args.iter().any(|a| a.contains("brief in data")), "{:?}", spec.args);
    env.core.shutdown();
}

#[test]
fn claude_antigravity_and_codex_have_a_chat_view_and_the_choice_is_remembered_per_tab() {
    let env = start_with(false, false);
    let claude = Ident::parse("claude@dev").expect("ident");
    let agy = Ident::parse("antigravity@dev").expect("ident");
    let codex = Ident::parse("codex@dev").expect("ident");
    assert!(env.core.tab_is_chat(&env.project, &claude), "chat is the default for Claude");
    assert!(env.core.tab_is_chat(&env.project, &agy), "chat is the default for Antigravity");
    assert!(env.core.tab_is_chat(&env.project, &codex), "chat is the default for Codex");
    assert!(!env.core.tab_is_chat(&env.project, &Ident::main("aider")));
    assert!(env.core.spawn_spec_chat(&env.project, &Ident::main("aider")).is_err());

    env.core.set_tab_chat(&env.project, &claude.id(), false);
    assert!(!env.core.tab_is_chat(&env.project, &claude));
    assert!(env.core.tab_is_chat(&env.project, &Ident::parse("claude@other").expect("ident")));
    env.core.set_tab_chat(&env.project, &claude.id(), true);
    assert!(env.core.tab_is_chat(&env.project, &claude));

    env.core.set_tab_chat(&env.project, &agy.id(), false);
    assert!(!env.core.tab_is_chat(&env.project, &agy));
    assert!(env.core.tab_is_chat(&env.project, &Ident::parse("antigravity@other").expect("ident")));
    env.core.set_tab_chat(&env.project, &agy.id(), true);
    assert!(env.core.tab_is_chat(&env.project, &agy));

    env.core.set_tab_chat(&env.project, &codex.id(), false);
    assert!(!env.core.tab_is_chat(&env.project, &codex));
    assert!(env.core.tab_is_chat(&env.project, &Ident::parse("codex@other").expect("ident")));
    env.core.set_tab_chat(&env.project, &codex.id(), true);
    assert!(env.core.tab_is_chat(&env.project, &codex));
    env.core.shutdown();
}

#[test]
fn the_codex_chat_launch_uses_the_structured_mode_and_the_tabs_own_conversation() {
    let env = start_with(false, false);
    install_fake_agent(&env.root, "codex");
    let ident = Ident::parse("codex@dev").expect("ident");
    let id = ident.id();

    let first = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(first.args.first().map(String::as_str), Some("app-server"), "{:?}", first.args);
    assert!(!first.resumed);

    let session = "01a18f37-5976-7bc1-9616-3ab2e181dab3";
    let codex_dir = env.root.join("home").join(".codex").join("sessions").join("2026").join("10").join("30");
    std::fs::create_dir_all(&codex_dir).expect("mkdir");
    std::fs::write(codex_dir.join(format!("rollout-2026-10-30T12-00-00-{session}.jsonl")), "").expect("rollout");
    assert!(env.core.set_tab_session(&env.project, &id, session));

    let again = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert!(again.resumed);
    assert_eq!(again.session.as_deref(), Some(session));
    assert_eq!(again.args.first().map(String::as_str), Some("app-server"));
    assert!(!again.args.contains(&"resume".to_owned()));

    let history = env.core.chat_history(&env.project, &id);
    assert!(history.history_pending);
    assert_eq!(history.session.as_deref(), Some(session));

    env.core.shutdown();
}

#[test]
fn the_antigravity_chat_launch_uses_the_structured_mode_and_the_tabs_own_conversation() {
    let env = start_with(false, false);
    install_fake_agent(&env.root, "agy");
    let ident = Ident::parse("antigravity@dev").expect("ident");
    let id = ident.id();

    let first = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(arg_after(&first.args, "--input-format").as_deref(), Some("stream-json"));
    assert_eq!(arg_after(&first.args, "--output-format").as_deref(), Some("stream-json"));
    assert_eq!(arg_after(&first.args, "--mode").as_deref(), Some("auto"));
    assert!(!first.args.iter().any(|a| a == "--log-file"), "chat mode does not use log file: {:?}", first.args);
    assert!(!first.resumed);

    // Simulate session saved in config and on disk:
    let session = "3ee16dad-e26c-4460-8dd5-99d773098a00";
    assert!(env.core.set_tab_session(&env.project, &id, session));
    let conv_dir = env.root.join("home").join(".gemini").join("antigravity-cli").join("conversations");
    std::fs::create_dir_all(&conv_dir).expect("mkdir");
    std::fs::write(conv_dir.join(format!("{session}.db")), "").expect("db");

    let brain_dir =
        env.root.join("home").join(".gemini").join("antigravity-cli").join("brain").join(session).join(".system_generated").join("logs");
    std::fs::create_dir_all(&brain_dir).expect("mkdir");
    let transcript = r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-10-30T01:00:00Z","content":"<USER_REQUEST>\nHello Antigravity\n</USER_REQUEST>"}
{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-30T01:00:01Z","content":"Hello from Antigravity!"}"#;
    std::fs::write(brain_dir.join("transcript.jsonl"), transcript).expect("transcript");

    let again = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(arg_after(&again.args, "--conversation").as_deref(), Some(session));
    assert!(again.resumed);

    let history = env.core.chat_history(&env.project, &id);
    assert_eq!(history.items.len(), 2, "{:?}", history.items);
    assert!(matches!(&history.items[0], Item::User { text, .. } if text == "Hello Antigravity"));
    assert!(matches!(&history.items[1], Item::Assistant { text, .. } if text == "Hello from Antigravity!"));
    assert_eq!(history.session.as_deref(), Some(session));

    env.core.shutdown();
}

/// The arguments of `spec` as `(flag, value)` pairs for a flag that can repeat (`-c k=v`).
fn all_after(args: &[String], flag: &str) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == flag).map(|w| w[1].clone()).collect()
}

#[test]
fn a_model_and_effort_picked_in_the_chat_restart_the_claude_launch_on_the_same_conversation() {
    let env = start_with(false, false);
    let ident = Ident::parse("claude@dev").expect("ident");
    let id = ident.id();
    let first = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    let session = arg_after(&first.args, "--session-id").expect("a new conversation gets an id");
    assert_eq!(arg_after(&first.args, "--model"), None);
    assert_eq!(arg_after(&first.args, "--effort"), None);
    // The CLI saved the conversation (a turn happened), so the next launch resumes it.
    let dir = env.root.join("home").join(".claude").join("projects").join("-ws");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join(format!("{session}.jsonl")), json!({ "type": "user", "message": { "content": "hi" } }).to_string())
        .expect("transcript");

    env.core.set_tab_model(&env.project, &ident, Some("opus")).expect("model");
    env.core.set_tab_effort(&env.project, &ident, Some(Effort::High)).expect("effort");
    let again = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(arg_after(&again.args, "--resume").as_deref(), Some(session.as_str()), "{:?}", again.args);
    assert_eq!(arg_after(&again.args, "--model").as_deref(), Some("opus"));
    assert_eq!(arg_after(&again.args, "--effort").as_deref(), Some("high"));
    assert_eq!(again.model.as_deref(), Some("opus"));
    assert_eq!(env.core.tab_session(&env.project, &id).as_deref(), Some(session.as_str()), "the conversation is kept");

    // The Terminal view of the tab uses the same choice and the same conversation.
    let term = env.core.spawn_spec_for(&env.project, &ident).expect("spec");
    assert_eq!(arg_after(&term.args, "--model").as_deref(), Some("opus"));
    assert_eq!(arg_after(&term.args, "--effort").as_deref(), Some("high"));
    assert_eq!(arg_after(&term.args, "--resume").as_deref(), Some(session.as_str()));

    // It is in the config (survives a restart of Holt) and goes away with the tab.
    let saved = env.core.config();
    let cfg = saved.project(&env.project).expect("project");
    assert_eq!(cfg.tab_models.get(&id).map(String::as_str), Some("opus"));
    assert_eq!(cfg.tab_efforts.get(&id).map(String::as_str), Some("high"));
    assert_eq!(env.core.instance_effort(&env.project, &ident), Some(Effort::High));

    // "Default" again drops the arguments.
    env.core.set_tab_model(&env.project, &ident, None).expect("model");
    env.core.set_tab_effort(&env.project, &ident, None).expect("effort");
    let plain = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!((arg_after(&plain.args, "--model"), arg_after(&plain.args, "--effort")), (None, None));
    assert_eq!(arg_after(&plain.args, "--resume").as_deref(), Some(session.as_str()));
    assert!(env.core.set_tab_model(&env.project, &ident, Some("bad name")).is_err());
    env.core.shutdown();
}

#[test]
fn the_antigravity_effort_is_a_suffix_of_the_model_and_the_conversation_is_resumed() {
    let env = start_with(false, false);
    install_fake_agent(&env.root, "agy");
    let ident = Ident::parse("antigravity@dev").expect("ident");
    let id = ident.id();
    let session = "3ee16dad-e26c-4460-8dd5-99d773098a00";
    assert!(env.core.set_tab_session(&env.project, &id, session));
    let conv_dir = env.root.join("home").join(".gemini").join("antigravity-cli").join("conversations");
    std::fs::create_dir_all(&conv_dir).expect("mkdir");
    std::fs::write(conv_dir.join(format!("{session}.db")), "").expect("db");

    env.core.set_tab_model(&env.project, &ident, Some("gemini-4-argon-low")).expect("model");
    assert_eq!(env.core.instance_effort(&env.project, &ident), Some(Effort::Low));
    env.core.set_tab_effort(&env.project, &ident, Some(Effort::High)).expect("effort");
    let spec = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(arg_after(&spec.args, "--model").as_deref(), Some("gemini-4-argon-high"), "{:?}", spec.args);
    assert_eq!(arg_after(&spec.args, "--conversation").as_deref(), Some(session));
    assert!(!spec.args.iter().any(|a| a == "--effort"), "{:?}", spec.args);

    // Another model: the caller picks the variant with the effort it wants.
    env.core.set_tab_model(&env.project, &ident, Some("claude-sonnet-5-5-medium")).expect("model");
    assert_eq!(env.core.instance_effort(&env.project, &ident), Some(Effort::Medium));
    // A model without effort variants has no effort to set.
    env.core.set_tab_model(&env.project, &ident, Some("gemini-4-flash")).expect("model");
    assert!(env.core.set_tab_effort(&env.project, &ident, Some(Effort::Low)).is_err());
    env.core.shutdown();
}

#[test]
fn the_codex_effort_goes_as_a_config_override_and_the_session_is_resumed_by_the_protocol() {
    let env = start_with(false, false);
    install_fake_agent(&env.root, "codex");
    let ident = Ident::parse("codex@dev").expect("ident");
    let id = ident.id();
    let session = "01a18f37-5976-7bc1-9616-3ab2e181dab3";
    let dir = env.root.join("home").join(".codex").join("sessions").join("2026").join("10").join("30");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join(format!("rollout-2026-10-30T12-00-00-{session}.jsonl")), "").expect("rollout");
    assert!(env.core.set_tab_session(&env.project, &id, session));

    env.core.set_tab_model(&env.project, &ident, Some("gpt-5.5")).expect("model");
    env.core.set_tab_effort(&env.project, &ident, Some(Effort::Medium)).expect("effort");
    let spec = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    let overrides = all_after(&spec.args, "-c");
    assert!(overrides.contains(&"model=gpt-5.5".to_owned()), "{overrides:?}");
    assert!(overrides.contains(&"model_reasoning_effort=medium".to_owned()), "{overrides:?}");
    assert_eq!(spec.args.first().map(String::as_str), Some("app-server"));
    assert!(spec.resumed);
    assert_eq!(spec.session.as_deref(), Some(session), "the protocol resumes it with thread/resume");
    env.core.shutdown();
}

#[cfg(unix)]
fn install_chat_agent(root: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    // Answers every line; on "need-permission" it asks first and writes the answer it got to `answer.txt`.
    let script = r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *need-permission*)
      echo '{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/x"},"tool_use_id":"t1","permission_suggestions":[]}}'
      IFS= read -r answer
      printf '%s\n' "$answer" > answer.txt
      ;;
    *) echo '{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"hello"}]}}' ;;
  esac
  echo '{"type":"result","is_error":false,"total_cost_usd":0.5,"session_id":"s"}'
done
"#;
    let path = root.join("fake-bin").join("claude");
    std::fs::write(&path, script).expect("write");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

#[cfg(unix)]
fn wait_for(proc: &chat::ChatProcess, what: &str, mut done: impl FnMut(&ChatMsg) -> bool) -> Vec<ChatMsg> {
    let limit = Instant::now() + Duration::from_secs(15);
    let mut seen = Vec::new();
    loop {
        assert!(Instant::now() < limit, "timed out waiting for {what}; got {seen:?}");
        match proc.try_recv() {
            Some(msg) => {
                let stop = done(&msg);
                seen.push(msg);
                if stop {
                    return seen;
                }
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[cfg(unix)]
#[test]
fn a_message_goes_in_the_events_come_back_and_the_permission_answer_reaches_the_process() {
    let env = start_with(false, false);
    install_chat_agent(&env.root);
    let ident = Ident::parse("claude@dev").expect("ident");
    let spec = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let w = woken.clone();
    let proc = env
        .core
        .spawn_chat(
            &spec,
            Arc::new(move || {
                w.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
        )
        .expect("spawn");

    proc.send_user("hi");
    let seen = wait_for(&proc, "the first turn", |m| matches!(m, ChatMsg::Event(ChatEvent::Result(_))));
    assert!(seen.iter().any(|m| matches!(m, ChatMsg::Event(ChatEvent::Assistant { .. }))));
    assert!(woken.load(std::sync::atomic::Ordering::SeqCst) > 0, "the interface is woken");

    proc.send_user("need-permission");
    let seen = wait_for(&proc, "the permission request", |m| matches!(m, ChatMsg::Event(ChatEvent::Permission(_))));
    let Some(ChatMsg::Event(ChatEvent::Permission(request))) = seen.last() else { unreachable!() };
    assert_eq!((request.request_id.as_str(), request.tool.as_str()), ("r1", "Write"));
    proc.answer_permission("r1", true, &request.input, "", &[]);
    wait_for(&proc, "the end of that turn", |m| matches!(m, ChatMsg::Event(ChatEvent::Result(_))));
    let answer: Value = serde_json::from_str(&std::fs::read_to_string(spec.cwd.join("answer.txt")).expect("answer")).expect("json");
    assert_eq!(answer["response"]["request_id"], "r1");
    assert_eq!(answer["response"]["response"]["behavior"], "allow");
    assert_eq!(answer["response"]["response"]["updatedInput"]["file_path"], "/x");

    // Killing the process is reported as its end.
    proc.kill();
    wait_for(&proc, "the exit", |m| matches!(m, ChatMsg::Exited(_)));
    env.core.shutdown();
}

#[cfg(unix)]
#[test]
fn a_process_that_cannot_start_is_an_error_not_a_hang() {
    let env = start_with(false, false);
    let ident = Ident::parse("claude@dev").expect("ident");
    let mut spec = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    spec.program = env.root.join("does-not-exist");
    assert!(env.core.spawn_chat(&spec, Arc::new(|| {})).is_err());
    env.core.shutdown();
}

/// Against the real Claude Code (costs a few tokens, needs the user's login): `cargo test -p holt-core --test chat -- --ignored`.
/// Checks that the arguments the core builds are accepted by the installed CLI, that a message gets an answer, and that a
/// process started again with `--resume` knows the conversation.
#[cfg(unix)]
#[test]
#[ignore = "talks to the real claude CLI"]
fn the_real_claude_answers_in_the_chat_mode_and_resumes_the_conversation() {
    isolated_config();
    let tmp = tempfile::tempdir().expect("tmp");
    let root = holt_core::util::strip_verbatim(&std::fs::canonicalize(tmp.path()).expect("canonicalize"));
    let ws = root.join("ws");
    std::fs::create_dir_all(&ws).expect("ws");
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).expect("HOME");
    let opts = holt_core::StartOptions {
        search_path: std::env::var_os("PATH"),
        home_dir: Some(home),
        watch_workspace: false,
        config: Some(holt_core::Config::with_project(ws.clone())),
        system_dirs: false,
    };
    let (core, _rx) = holt_core::Core::start_with(paths_in(&root), opts).expect("start");
    let project = core.active_project_id();
    let ident = Ident::parse("claude@dev").expect("ident");

    let run = |text: &str| -> String {
        let spec = core.spawn_spec_chat(&project, &ident).expect("spec");
        let proc = core.spawn_chat(&spec, Arc::new(|| {})).expect("spawn");
        proc.send_user(text);
        let limit = Instant::now() + Duration::from_secs(120);
        let mut transcript = chat::Transcript::default();
        transcript.push_user(text, None);
        loop {
            assert!(Instant::now() < limit, "no answer: {:?}", transcript.items);
            match proc.try_recv() {
                Some(ChatMsg::Event(ev)) => {
                    transcript.apply(&ev);
                    if matches!(ev, ChatEvent::Result(_)) {
                        break;
                    }
                }
                Some(ChatMsg::Stderr(line)) => eprintln!("stderr: {line}"),
                Some(ChatMsg::Exited(code)) => panic!("exited {code:?}: {:?}", transcript.items),
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        transcript
            .items
            .iter()
            .filter_map(|i| if let Item::Assistant { text, .. } = i { Some(text.clone()) } else { None })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let first = run("Remember the word BANANA7. Reply with just: ok");
    assert!(!first.is_empty());
    // A new process for the same tab resumes the conversation (the second launch uses `--resume`).
    let second = run("What was the word I told you to remember? Reply with just the word.");
    assert!(second.contains("BANANA7"), "{second}");
    core.shutdown();
}

// ---------- The generic ACP backend, against a fake ACP agent ----------

/// The fake ACP agent (a test `[[bin]]`, see `tests/support/fake_acp_agent.rs`) installed as the CLI `name`, with `behaviour`
/// next to it.
mod acp {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use holt_core::chat::{self, ChatEvent, ChatMsg, ChatProcess, Item, Transcript};
    use holt_core::Ident;
    use serde_json::{json, Value};

    use super::env::*;

    const FAKE_ACP_AGENT: &str = env!("CARGO_BIN_EXE_fake_acp_agent");

    pub fn install(root: &Path, name: &str, behaviour: &Value) -> PathBuf {
        let exe = if cfg!(windows) {
            let bin = root.join("home").join(".local").join("bin");
            std::fs::create_dir_all(&bin).expect("mkdir");
            bin.join(format!("{name}.exe"))
        } else {
            let bin = root.join("fake-bin");
            std::fs::create_dir_all(&bin).expect("mkdir");
            bin.join(name)
        };
        std::fs::copy(FAKE_ACP_AGENT, &exe).expect("copy the fake agent");
        std::fs::write(exe.with_extension("fake.json"), behaviour.to_string()).expect("behaviour");
        exe
    }

    /// A chat process of the fake agent and the conversation its events build, like the interface does.
    pub struct Session {
        pub proc: ChatProcess,
        pub transcript: Transcript,
        pub events: Vec<ChatEvent>,
        pub exited: Option<Option<i32>>,
        pub spec: holt_core::SpawnSpec,
    }

    pub fn start(env: &Env, ident: &str) -> Session {
        let ident = Ident::parse(ident).expect("ident");
        let spec = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
        let proc = env.core.spawn_chat(&spec, Arc::new(|| {})).expect("spawn");
        // A chat opened on a saved conversation waits for its history.
        let mut transcript = env.core.chat_history(&env.project, &ident.id());
        transcript.history_pending = spec.session.is_some();
        Session { proc, transcript, events: Vec::new(), exited: None, spec }
    }

    impl Session {
        pub fn send(&mut self, text: &str) {
            self.transcript.push_user(text, None);
            self.proc.send_user(text);
        }

        /// Applies what arrives until `done` says stop.
        pub fn until(&mut self, what: &str, mut done: impl FnMut(&Session) -> bool) {
            let limit = Instant::now() + Duration::from_secs(15);
            while !done(self) {
                assert!(Instant::now() < limit, "timed out waiting for {what}; items: {:?}; events: {:?}", self.transcript.items, self.events);
                match self.proc.try_recv() {
                    Some(ChatMsg::Event(ev)) => {
                        self.transcript.apply(&ev);
                        self.events.push(ev);
                    }
                    Some(ChatMsg::Exited(code)) => self.exited = Some(code),
                    Some(ChatMsg::Stderr(_)) => {}
                    None => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        }

        pub fn results(&self) -> usize {
            self.events.iter().filter(|e| matches!(e, ChatEvent::Result(_))).count()
        }

        /// Waits for `n` results (turns that ended).
        pub fn turns(&mut self, n: usize) {
            self.until("the end of the turn", |s| s.results() >= n);
        }

        pub fn texts(&self) -> Vec<String> {
            self.transcript.items.iter().filter_map(|i| if let Item::Assistant { text, .. } = i { Some(text.clone()) } else { None }).collect()
        }

        pub fn permission(&mut self) -> chat::PermissionRequest {
            self.until("a permission request", |s| s.events.iter().any(|e| matches!(e, ChatEvent::Permission(_))));
            self.events
                .iter()
                .find_map(|e| if let ChatEvent::Permission(p) = e { Some(p.clone()) } else { None })
                .expect("a permission request")
        }
    }

    pub fn log_lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    pub fn behaviour(log: &Path) -> Value {
        json!({ "log": log, "load_session": true, "models": true })
    }
}

fn acp_env(kind: &str, behaviour: impl FnOnce(&std::path::Path) -> Value) -> (Env, std::path::PathBuf) {
    let env = start_with(false, false);
    let log = env.root.join(format!("{kind}-acp.log"));
    acp::install(&env.root, kind, &behaviour(&log));
    (env, log)
}

#[test]
fn every_cli_with_an_acp_mode_starts_it_with_its_own_flags_and_resumes_through_the_protocol() {
    let env = start_with(false, false);
    for (kind, leading, mode) in [
        ("opencode", vec!["acp"], vec![]),
        ("goose", vec!["acp"], vec![]),
        ("gemini", vec![], vec!["--acp"]),
        ("qwen", vec![], vec!["--acp"]),
    ] {
        acp::install(&env.root, kind, &json!({}));
        let ident = Ident::main(kind);
        let spec = env.core.spawn_spec_chat(&env.project, &ident).unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert_eq!(spec.args.iter().take(leading.len()).collect::<Vec<_>>(), leading, "{kind}: {:?}", spec.args);
        assert_eq!(spec.args.iter().rev().take(mode.len()).rev().collect::<Vec<_>>(), mode, "{kind}: {:?}", spec.args);
        assert!(!spec.args.iter().any(|a| a == "--session" || a == "--resume"), "{kind} resumes by session/load: {:?}", spec.args);
        assert!(env.core.tab_is_chat(&env.project, &ident), "{kind} opens in the chat view");
        // Only the CLIs with no instructions channel of their own get them in front of the first prompt.
        let own_channel = matches!(kind, "opencode" | "goose");
        assert_eq!(spec.instructions.is_some(), !own_channel, "{kind}");
    }
    // A CLI without an ACP mode stays terminal-only.
    for kind in ["aider", "interpreter", "q"] {
        assert!(!chat::supports(kind), "{kind}");
        assert!(env.core.spawn_spec_chat(&env.project, &Ident::main(kind)).is_err(), "{kind}");
    }
    env.core.shutdown();
}

#[test]
fn opencode_in_acp_mode_keeps_its_model_and_instructions_in_the_configuration() {
    let (env, _) = acp_env("opencode", |_| json!({}));
    let ident = Ident::main("opencode");
    env.core.set_tab_model(&env.project, &ident, Some("opencode/glm-5.3-flash-free")).expect("model");
    let spec = env.core.spawn_spec_chat(&env.project, &ident).expect("spec");
    assert_eq!(spec.args, ["acp"], "no --standalone, no --model: {:?}", spec.args);
    let config: Value = serde_json::from_str(&spec.env["OPENCODE_CONFIG_CONTENT"]).expect("json");
    assert_eq!(config["model"], "opencode/glm-5.3-flash-free");
    assert!(config["instructions"][0].as_str().is_some());
    // The terminal view of the same tab keeps its own launch.
    let term = env.core.spawn_spec_for(&env.project, &ident).expect("spec");
    assert!(term.args.iter().any(|a| a == "--standalone"), "{:?}", term.args);
    env.core.shutdown();
}

#[test]
fn copilot_in_chat_needs_the_standalone_cli_not_the_old_gh_extension() {
    let env = start_with(false, false);
    // Only `gh` with the extension: the terminal works, the chat has nothing to speak ACP with.
    let gh = env.root.join("fake-bin").join("gh");
    std::fs::write(&gh, "#!/bin/sh\necho 'gh-copilot  github/gh-copilot'\n").expect("gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let err = env.core.spawn_spec_chat(&env.project, &Ident::main("copilot")).expect_err("no standalone copilot");
        assert!(err.contains("standalone"), "{err}");
        assert!(env.core.spawn_spec_for(&env.project, &Ident::main("copilot")).is_ok());
    }
    env.core.shutdown();
}

#[test]
fn the_handshake_opens_a_session_and_text_streams_into_one_message_per_turn() {
    let (env, log) = acp_env("opencode", acp::behaviour);
    let mut chat = acp::start(&env, "opencode");
    chat.send("hello there");
    chat.turns(1);
    assert_eq!(chat.texts(), ["echo: hello there"]);
    assert!(!chat.transcript.busy());
    // What Holt said: initialize with no fs and no terminal, then a session in the tab's folder, then the prompt.
    let sent = acp::log_lines(&log);
    assert_eq!(sent[0]["method"], "initialize");
    assert_eq!(sent[0]["params"]["protocolVersion"], 1);
    assert_eq!(sent[0]["params"]["clientCapabilities"]["fs"], json!({ "readTextFile": false, "writeTextFile": false }));
    assert_eq!(sent[0]["params"]["clientCapabilities"]["terminal"], false);
    assert_eq!(sent[1]["method"], "session/new");
    assert_eq!(sent[1]["params"]["cwd"], chat.spec.cwd.to_string_lossy().as_ref());
    assert_eq!(sent[1]["params"]["mcpServers"], json!([]));
    assert_eq!(sent[2]["method"], "session/prompt");
    assert_eq!(sent[2]["params"]["sessionId"], "sess-new");
    assert_eq!(sent[2]["params"]["prompt"], json!([{ "type": "text", "text": "hello there" }]));
    assert!(sent.iter().all(|m| m["jsonrpc"] == "2.0"));
    // The session id, the model the agent says it uses and its commands reach the view.
    assert_eq!(chat.transcript.session.as_deref(), Some("sess-new"));
    assert_eq!(chat.transcript.model, "Fast One");
    assert_eq!(chat.transcript.commands, ["review"]);
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn a_message_sent_during_a_turn_waits_and_each_message_gets_its_own_result() {
    let (env, log) = acp_env("goose", acp::behaviour);
    let mut chat = acp::start(&env, "goose");
    chat.send("first");
    chat.send("second");
    chat.send("third");
    chat.turns(3);
    assert_eq!(chat.texts(), ["echo: first", "echo: second", "echo: third"]);
    let prompts: Vec<Value> = acp::log_lines(&log).into_iter().filter(|m| m["method"] == "session/prompt").collect();
    assert_eq!(prompts.len(), 3);
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn thinking_and_a_tool_call_with_its_output_become_items_in_order() {
    let (env, _) = acp_env("opencode", acp::behaviour);
    let mut chat = acp::start(&env, "opencode");
    chat.send("think and run a tool");
    chat.turns(1);
    let kinds: Vec<&str> = chat
        .transcript
        .items
        .iter()
        .map(|i| match i {
            Item::User { .. } => "user",
            Item::Thinking(_) => "thinking",
            Item::Assistant { .. } => "text",
            Item::Tool(_) => "tool",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["user", "thinking", "text", "tool", "text"]);
    assert!(matches!(&chat.transcript.items[1], Item::Thinking(t) if t == "Let me think."));
    let Item::Tool(call) = &chat.transcript.items[3] else { panic!() };
    assert_eq!((call.name.as_str(), call.input["command"].as_str()), ("Bash", Some("echo hi")));
    let outcome = call.outcome.as_ref().expect("a result");
    assert_eq!((outcome.text.as_str(), outcome.is_error), ("hi\n", false));
    assert_eq!(chat.texts(), ["Running it.", "It printed hi."]);
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn an_edit_carries_its_diff_as_the_patch_hunks() {
    let (env, _) = acp_env("goose", acp::behaviour);
    let mut chat = acp::start(&env, "goose");
    chat.send("edit the notes");
    chat.turns(1);
    let call = chat.transcript.items.iter().find_map(|i| if let Item::Tool(c) = i { Some(c) } else { None }).expect("a tool call");
    assert_eq!((call.name.as_str(), call.input["file_path"].as_str()), ("Edit", Some("/work/notes.txt")));
    let patch = &call.outcome.as_ref().expect("a result").patch;
    assert_eq!(patch.len(), 1);
    assert_eq!(patch[0].lines, [" one", "-two", "+TWO", " three"]);
    assert!(call.input["diff"].as_str().unwrap().contains("+TWO"), "the diff is also in the call, for the permission card");
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn a_permission_allowed_once_for_the_session_or_denied_reaches_the_agent_as_the_matching_option() {
    for (allow, for_session, expect) in [(true, false, "allowed once"), (true, true, "allowed always"), (false, false, "denied")] {
        let (env, log) = acp_env("opencode", acp::behaviour);
        let mut chat = acp::start(&env, "opencode");
        chat.send("ask a permission");
        let request = chat.permission();
        assert_eq!((request.tool.as_str(), request.input["command"].as_str()), ("Bash", Some("rm -rf build")));
        assert_eq!(request.request_id, "77");
        // "For this session" is offered because the agent has an allow_always option; it travels as that option's id.
        assert_eq!(request.suggestions, vec![json!("always")]);
        assert!(chat.transcript.waiting_for_permission());
        assert!(chat.transcript.busy(), "the turn waits for the user");
        let rules = if for_session { request.suggestions.clone() } else { Vec::new() };
        chat.proc.answer_permission(&request.request_id, allow, &request.input, "no", &rules);
        chat.transcript.answer_permission(&request.request_id, allow);
        chat.turns(1);
        assert_eq!(chat.texts(), [expect]);
        let answer = acp::log_lines(&log).into_iter().find(|m| m["id"] == 77).expect("the answer");
        assert_eq!(answer["result"]["outcome"]["outcome"], "selected");
        let option = match expect {
            "allowed once" => "once",
            "allowed always" => "always",
            _ => "no",
        };
        assert_eq!(answer["result"]["outcome"]["optionId"], option);
        chat.proc.kill();
        env.core.shutdown();
    }
}

#[test]
fn cancelling_mid_turn_sends_session_cancel_and_the_turn_ends_interrupted() {
    let (env, log) = acp_env("goose", acp::behaviour);
    let mut chat = acp::start(&env, "goose");
    chat.send("do something slow");
    chat.until("the tool call", |s| s.transcript.items.iter().any(|i| matches!(i, Item::Tool(_))));
    assert!(chat.transcript.busy());
    chat.proc.interrupt();
    chat.turns(1);
    assert!(chat.events.iter().any(|e| matches!(e, ChatEvent::Interrupted)));
    assert!(!chat.transcript.busy());
    assert!(chat.transcript.items.iter().any(|i| matches!(i, Item::Notice { text, .. } if text == "Interrupted.")));
    // The call the turn left open is closed, not left spinning.
    let call = chat.transcript.items.iter().find_map(|i| if let Item::Tool(c) = i { Some(c) } else { None }).expect("call");
    assert!(call.outcome.as_ref().is_some_and(|o| o.is_error));
    let cancel = acp::log_lines(&log).into_iter().find(|m| m["method"] == "session/cancel").expect("a cancel");
    assert_eq!(cancel["params"]["sessionId"], "sess-new");
    assert!(cancel.get("id").is_none(), "session/cancel is a notification");
    // The agent is still alive and takes the next message.
    chat.send("hello again");
    chat.turns(2);
    assert_eq!(chat.texts().last().map(String::as_str), Some("echo: hello again"));
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn an_interrupt_with_a_pending_permission_cancels_the_request_too() {
    let (env, log) = acp_env("opencode", acp::behaviour);
    let mut chat = acp::start(&env, "opencode");
    chat.send("ask a permission");
    let _ = chat.permission();
    chat.proc.interrupt();
    chat.turns(1);
    let answer = acp::log_lines(&log).into_iter().find(|m| m["id"] == 77).expect("the request is answered");
    assert_eq!(answer["result"]["outcome"], json!({ "outcome": "cancelled" }));
    assert!(!chat.transcript.waiting_for_permission(), "the card is no longer pending once the turn ended");
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn a_process_that_exits_in_the_middle_of_a_turn_is_reported() {
    let (env, _) = acp_env("opencode", acp::behaviour);
    let mut chat = acp::start(&env, "opencode");
    chat.send("crash now");
    chat.until("the exit", |s| s.exited.is_some());
    assert_eq!(chat.exited, Some(Some(3)));
    assert_eq!(chat.results(), 0, "the turn never ended");
    chat.transcript.process_ended();
    assert!(!chat.transcript.busy());
    assert_eq!(chat.texts(), ["About to fail"]);
    env.core.shutdown();
}

#[test]
fn a_line_that_is_not_json_and_a_method_holt_never_advertised_do_not_stop_the_turn() {
    let (env, log) = acp_env("goose", acp::behaviour);
    let mut chat = acp::start(&env, "goose");
    chat.send("garbage and a request");
    chat.turns(1);
    assert_eq!(chat.texts(), ["echo: garbage and a request"]);
    // The agent asked for a file Holt never offered to read: it got an error, not silence.
    let reply = acp::log_lines(&log).into_iter().find(|m| m["id"] == 900).expect("an answer to the request");
    assert_eq!(reply["error"]["code"], -32601);
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn a_saved_conversation_is_loaded_and_its_history_replaces_the_empty_view() {
    let (env, log) = acp_env("goose", acp::behaviour);
    let ident = Ident::main("goose");
    assert!(env.core.set_tab_session(&env.project, &ident.id(), "sess-old"));
    let mut chat = acp::start(&env, "goose");
    assert_eq!(chat.spec.session.as_deref(), Some("sess-old"));
    assert!(chat.transcript.history_pending);
    chat.send("and now?");
    chat.turns(1);
    let load = acp::log_lines(&log).into_iter().find(|m| m["method"] == "session/load").expect("session/load");
    assert_eq!((load["params"]["sessionId"].as_str(), load["params"]["mcpServers"].clone()), (Some("sess-old"), json!([])));
    assert!(acp::log_lines(&log).iter().all(|m| m["method"] != "session/new"));
    // The history comes first, then what was typed while the process started.
    let items = &chat.transcript.items;
    assert!(matches!(&items[0], Item::User { text, .. } if text == "What is in main.rs?"), "{items:?}");
    assert!(matches!(&items[1], Item::Assistant { text, .. } if text == "It has a main function."), "{items:?}");
    assert!(matches!(&items[2], Item::Tool(c) if c.name == "Read" && c.input["file_path"] == "/work/main.rs" && c.outcome.is_some()), "{items:?}");
    assert!(matches!(&items[3], Item::User { text, .. } if text == "and now?"), "{items:?}");
    assert_eq!(chat.transcript.session.as_deref(), Some("sess-old"));
    assert!(!chat.transcript.history_pending);
    // The prompt goes to the loaded session.
    let prompt = acp::log_lines(&log).into_iter().find(|m| m["method"] == "session/prompt").expect("prompt");
    assert_eq!(prompt["params"]["sessionId"], "sess-old");
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn an_agent_without_load_session_starts_a_new_conversation_and_says_so() {
    let (env, log) = acp_env("qwen", |log| json!({ "log": log, "load_session": false }));
    let ident = Ident::main("qwen");
    assert!(env.core.set_tab_session(&env.project, &ident.id(), "sess-old"));
    let mut chat = acp::start(&env, "qwen");
    chat.send("hi");
    chat.turns(1);
    assert!(chat.transcript.items.iter().any(|i| matches!(i, Item::Notice { text, error: false } if text.contains("cannot resume"))));
    assert!(acp::log_lines(&log).iter().all(|m| m["method"] != "session/load"));
    assert_eq!(chat.transcript.session.as_deref(), Some("sess-new"), "the new id replaces the saved one");
    assert!(!chat.transcript.history_pending, "nothing to wait for");
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn a_conversation_the_agent_no_longer_has_falls_back_to_a_new_one() {
    let (env, _) = acp_env("goose", acp::behaviour);
    let ident = Ident::main("goose");
    assert!(env.core.set_tab_session(&env.project, &ident.id(), "gone"));
    let mut chat = acp::start(&env, "goose");
    chat.send("hi");
    chat.turns(1);
    assert!(chat.transcript.items.iter().any(|i| matches!(i, Item::Notice { text, .. } if text.contains("Session not found"))));
    assert_eq!(chat.texts(), ["echo: hi"]);
    assert_eq!(chat.transcript.session.as_deref(), Some("sess-new"));
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn an_agent_that_wants_a_login_says_where_to_do_it() {
    let (env, _) = acp_env("gemini", |_| json!({ "auth_required": true }));
    let mut chat = acp::start(&env, "gemini");
    chat.send("hi");
    chat.until("the error", |s| s.transcript.items.iter().any(|i| matches!(i, Item::Notice { error: true, .. })));
    let notice = chat.transcript.items.iter().find_map(|i| if let Item::Notice { text, .. } = i { Some(text.clone()) } else { None }).unwrap();
    assert!(notice.contains("Authentication required") && notice.contains("Terminal view"), "{notice}");
    // The message that waited ends as a failed turn instead of hanging the tab.
    chat.turns(1);
    assert!(!chat.transcript.busy());
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn a_cli_without_its_own_instructions_channel_gets_them_in_front_of_the_first_prompt_only() {
    let (env, log) = acp_env("gemini", acp::behaviour);
    let mut chat = acp::start(&env, "gemini");
    chat.send("first");
    chat.send("second");
    chat.turns(2);
    let prompts: Vec<String> = acp::log_lines(&log)
        .into_iter()
        .filter(|m| m["method"] == "session/prompt")
        .map(|m| m["params"]["prompt"][0]["text"].as_str().unwrap().to_owned())
        .collect();
    assert!(prompts[0].starts_with("<holt-instructions>\n") && prompts[0].contains("do NOT use your CLI's internal subagents"), "{}", prompts[0]);
    assert!(prompts[0].ends_with("</holt-instructions>\n\nfirst"));
    assert_eq!(prompts[1], "second");
    chat.proc.kill();
    env.core.shutdown();
}

#[test]
fn the_instructions_are_not_shown_when_a_conversation_that_carried_them_is_replayed() {
    // The agent replays the first prompt as it received it, preamble included.
    let (env, _) = acp_env("qwen", acp::behaviour);
    let ident = Ident::main("qwen");
    assert!(env.core.set_tab_session(&env.project, &ident.id(), "sess-old"));
    let mut chat = acp::start(&env, "qwen");
    chat.send("again");
    chat.turns(1);
    assert!(chat.transcript.items.iter().all(|i| !matches!(i, Item::User { text, .. } if text.contains("holt-instructions"))));
    chat.proc.kill();
    env.core.shutdown();
}
