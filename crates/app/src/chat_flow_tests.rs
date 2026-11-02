//! The chat view of a Claude tab end to end, with the real core and a fake `claude` that speaks the structured mode: a
//! message from another agent becomes part of the conversation (no typing, no Enter), a permission request waits for the
//! user and its answer reaches the CLI, and the tab goes back and forth between chat and terminal on the same
//! conversation. No window: the frame is just `before_frame`.

use std::os::unix::fs::PermissionsExt;

use holt_core::chat::Item;
use holt_core::Ident;
use serde_json::{json, Value};

use crate::tasks_flow_tests::Env;

/// Answers every line with a message and a result; on "need-permission" it asks first and writes the answer it got to
/// `answer.txt` (in the project folder).
const FAKE_CLAUDE: &str = r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *need-permission*)
      echo '{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Write","input":{"file_path":"/x"}}]}}'
      echo '{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/x"},"tool_use_id":"t1","permission_suggestions":[]}}'
      IFS= read -r answer
      printf '%s\n' "$answer" > answer.txt
      ;;
    *) echo '{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"hello"}]}}' ;;
  esac
  echo '{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0.3,"resetsAt":1793506200},"seven_day":{"utilization":0.61,"resetsAt":1793732400}}}}'
  echo '{"type":"result","is_error":false,"total_cost_usd":0.5,"session_id":"s"}'
done
"#;

fn env_with_chat_agent() -> Env {
    let mut env = Env::start_with(true);
    let script = env.ws.parent().expect("root").join("fake-bin").join("claude");
    std::fs::write(&script, FAKE_CLAUDE).expect("write");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    env.pump("the agent list", |d| d.agents.knows("claude"));
    env.deck.open_tab("claude", false);
    env.pump("the chat tab to be running", |d| d.tabs.first().is_some_and(|t| t.alive));
    env
}

fn items(env: &Env) -> Vec<Item> {
    env.deck.tabs[0].chat.as_ref().expect("a chat tab").transcript.items.clone()
}

#[test]
fn a_claude_tab_opens_in_the_chat_view_with_its_own_conversation() {
    let env = env_with_chat_agent();
    let tab = &env.deck.tabs[0];
    assert!(tab.chat.is_some(), "chat is the default view of a Claude tab");
    let spec = tab.spec.as_ref().expect("spec");
    assert!(spec.args.iter().any(|a| a == "stream-json"), "{:?}", spec.args);
    let id = tab.ident.id();
    assert!(env.core.tab_session(&env.project(), &id).is_some(), "the conversation id is saved as soon as the tab starts");
}

#[test]
fn a_question_from_another_agent_becomes_a_message_of_the_conversation_and_ends_with_the_turn() {
    let mut env = env_with_chat_agent();
    let prompt = "[Message from the agent \"claude@dev\" received by Holt]\n\nWhich key did you pick?\n\nAnswer by running in the shell: ask-agent --reply 7 \"your answer\"";
    env.deck.send_to_instance(&Ident::main("claude"), false, prompt.to_owned()).expect("queued");
    env.pump("the answer", |d| {
        d.tabs[0]
            .chat
            .as_ref()
            .is_some_and(|c| c.transcript.items.iter().any(|i| matches!(i, Item::Assistant { text, .. } if text == "hello")))
    });
    let items = items(&env);
    // The conversation shows the message, not the instructions that Holt adds for the model.
    assert!(
        matches!(&items[0], Item::User { text, from: Some(who) } if text == "Which key did you pick?" && who == "claude@dev"),
        "{items:?}"
    );
    env.pump("the turn to end", |d| !d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.busy()) && !d.tabs[0].busy);
}

#[test]
fn a_permission_request_waits_for_the_user_and_the_answer_reaches_the_cli() {
    let mut env = env_with_chat_agent();
    assert_eq!(env.deck.tabs[0].phase, None);
    assert_eq!(env.deck.tabs[0].idle_since, None);
    env.deck.tabs[0].chat.as_mut().expect("chat").composer = "need-permission please".into();
    env.deck.chat_submit(0);
    assert!(env.deck.tabs[0].busy, "sending a message starts a turn");
    env.pump("the permission card", |d| d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.waiting_for_permission()));
    assert!(env.deck.tabs[0].chat.as_ref().expect("chat").transcript.busy(), "the turn waits for the answer");
    assert_eq!(env.deck.tabs[0].phase, Some(holt_core::Phase::WaitingForYou));
    assert_eq!(env.deck.tabs[0].status_label(std::time::Instant::now()).as_deref(), Some("waiting for you"));
    assert!(env.deck.tabs[0].done || env.deck.active == Some(0));

    env.deck.chat_answer(0, "r1", true, false);
    assert_eq!(
        env.deck.tabs[0].phase,
        Some(holt_core::Phase::Editing),
        "answering the permission moves the phase to the running Write tool"
    );
    env.pump("the end of the turn", |d| !d.tabs[0].busy);
    assert_eq!(env.deck.tabs[0].phase, None, "phase clears when the turn ends");
    assert!(env.deck.tabs[0].idle_since.is_some(), "idle_since is recorded when the turn ends");
    assert_eq!(env.deck.tabs[0].status_label(std::time::Instant::now()).as_deref(), Some("idle <1 min"));
    let answer: Value = serde_json::from_str(&std::fs::read_to_string(env.ws.join("answer.txt")).expect("answer")).expect("json");
    assert_eq!(answer["response"]["response"]["behavior"], "allow");
    assert_eq!(answer["response"]["request_id"], "r1");
    assert!(!env.deck.tabs[0].chat.as_ref().expect("chat").transcript.waiting_for_permission());
}

#[test]
fn a_terminal_only_command_is_not_sent() {
    let mut env = env_with_chat_agent();
    env.deck.tabs[0].chat.as_mut().expect("chat").composer = "/config".into();
    env.deck.chat_submit(0);
    let items = items(&env);
    assert!(matches!(items.last(), Some(Item::Notice { text, .. }) if text.contains("/config")), "{items:?}");
    assert!(!env.deck.tabs[0].busy, "nothing was sent, so no turn started");
    assert!(env.deck.tabs[0].chat.as_ref().expect("chat").composer.is_empty());
}

#[test]
fn picking_a_model_and_an_effort_restarts_the_process_on_the_same_conversation() {
    let mut env = env_with_chat_agent();
    let id = env.deck.tabs[0].ident.id();
    let session = env.core.tab_session(&env.project(), &id).expect("the conversation id");
    let before = env.deck.tabs[0].spawned_at;
    // The CLI saved the conversation once a turn happened: the restart resumes it.
    let dir = env.ws.parent().expect("root").join("home").join(".claude").join("projects").join("-ws");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join(format!("{session}.jsonl")), json!({ "type": "user", "message": { "content": "hi" } }).to_string())
        .expect("file");

    env.deck.chat_set_model(0, Some("opus"));
    env.deck.chat_set_effort(0, Some(holt_core::models::Effort::High));
    env.pump("the process to be running again", |d| d.tabs[0].alive && d.tabs[0].chat.as_ref().is_some_and(|c| c.proc.is_some()));
    let tab = &env.deck.tabs[0];
    assert!(tab.spawned_at > before, "a new process was started");
    let args = &tab.spec.as_ref().expect("spec").args;
    let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    assert_eq!(after("--model").as_deref(), Some("opus"), "{args:?}");
    assert_eq!(after("--effort").as_deref(), Some("high"), "{args:?}");
    assert_eq!(after("--resume").as_deref(), Some(session.as_str()), "{args:?}");
    assert_eq!(tab.model.as_deref(), Some("opus"));
    assert_eq!(env.core.tab_session(&env.project(), &id).as_deref(), Some(session.as_str()));
    // The conversation says what changed and goes on; the choice is saved for the tab.
    assert!(items(&env).iter().any(|i| matches!(i, Item::Notice { error: false, .. })), "{:?}", items(&env));
    let cfg = env.core.config();
    let project = cfg.project(&env.project()).expect("project");
    assert_eq!(project.tab_models.get(&id).map(String::as_str), Some("opus"));
    assert_eq!(project.tab_efforts.get(&id).map(String::as_str), Some("high"));
    // An invalid name is refused with a notice, nothing is restarted or saved.
    let spawned = env.deck.tabs[0].spawned_at;
    env.deck.chat_set_model(0, Some("two words"));
    assert_eq!(env.deck.tabs[0].spawned_at, spawned);
    assert!(items(&env).iter().any(|i| matches!(i, Item::Notice { error: true, .. })));
}

#[test]
fn the_tab_goes_to_the_terminal_and_back_on_the_same_conversation() {
    let mut env = env_with_chat_agent();
    let id = env.deck.tabs[0].ident.id();
    let session = env.core.tab_session(&env.project(), &id).expect("session");
    // What the CLI saved of the conversation so far.
    let dir = env.ws.parent().expect("root").join("home").join(".claude").join("projects").join("-ws");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let lines = [
        json!({ "type": "user", "message": { "content": "first question" } }),
        json!({ "type": "assistant", "message": { "id": "m1", "content": [{ "type": "text", "text": "first answer" }] } }),
    ];
    std::fs::write(dir.join(format!("{session}.jsonl")), lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n"))
        .expect("transcript");

    env.deck.set_tab_view(0, false);
    assert!(env.deck.tabs[0].chat.is_none() && env.deck.tabs[0].alive);
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(spec.resumed && spec.args.contains(&session) && !spec.args.iter().any(|a| a == "stream-json"), "{:?}", spec.args);
    assert!(!env.core.tab_is_chat(&env.project(), &env.deck.tabs[0].ident), "the choice is remembered");

    env.deck.set_tab_view(0, true);
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(spec.resumed && spec.args.iter().any(|a| a == "stream-json"));
    let items = items(&env);
    assert_eq!(items.len(), 2, "the conversation comes back from the CLI's own transcript: {items:?}");
    assert!(matches!(&items[0], Item::User { text, .. } if text == "first question"));
    assert_eq!(env.core.tab_session(&env.project(), &id).as_deref(), Some(session.as_str()), "same conversation");
}

#[test]
fn the_plan_usage_in_the_stream_reaches_the_usage_bar() {
    let mut env = env_with_chat_agent();
    env.deck.tabs[0].chat.as_mut().expect("chat").composer = "hi".into();
    env.deck.chat_submit(0);
    env.pump("the usage of the turn", |d| d.core.usage_snapshot().get("claude").is_some_and(|u| u.weekly.is_some()));
    let u = env.core.usage_snapshot()["claude"].clone();
    assert_eq!(u.session.map(|w| w.used), Some(0.3));
    assert_eq!(u.weekly.map(|w| w.used), Some(0.61));
    assert!(u.updated_ms > 0);
}

#[test]
fn a_view_switch_during_a_turn_waits_for_its_end_instead_of_killing_it() {
    let mut env = env_with_chat_agent();
    env.deck.tabs[0].chat.as_mut().expect("chat").composer = "need-permission please".into();
    env.deck.chat_submit(0);
    env.pump("the permission card", |d| d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.waiting_for_permission()));
    let spawned = env.deck.tabs[0].spawned_at;
    let session = env.core.tab_session(&env.project(), &env.deck.tabs[0].ident.id()).expect("session");

    env.deck.set_tab_view(0, false);
    assert!(env.deck.tabs[0].chat.is_some(), "the turn keeps running in the chat view");
    assert_eq!(env.deck.tabs[0].spawned_at, spawned, "the process was not restarted");
    assert_eq!(env.deck.tabs[0].view_after_turn, Some(false));
    // Clicking the current view again cancels it; asking again brings it back.
    env.deck.set_tab_view(0, true);
    assert_eq!(env.deck.tabs[0].view_after_turn, None);
    env.deck.set_tab_view(0, false);

    env.deck.chat_answer(0, "r1", true, false);
    env.pump("the switch at the end of the turn", |d| d.tabs[0].chat.is_none() && d.tabs[0].alive);
    assert!(env.deck.tabs[0].view_after_turn.is_none());
    assert!(env.ws.join("answer.txt").exists(), "the answer reached the CLI before the switch");
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(!spec.args.iter().any(|a| a == "stream-json"), "{:?}", spec.args);
    assert!(spec.args.contains(&session), "same conversation: {:?}", spec.args);
}

const FAKE_ANTIGRAVITY: &str = r#"#!/bin/sh
echo '{"event":"init","conversation_id":"01234567-89ab-cdef-0123-456789abcdef","init":{"permission_mode":"always-proceed"}}'
while IFS= read -r line; do
  echo '{"event":"step_update","step_update":{"conversation_id":"01234567-89ab-cdef-0123-456789abcdef","step_index":1,"state":"ACTIVE","step_type":"agent_response","text_delta":"hello from agy"}}'
  echo '{"event":"step_update","step_update":{"conversation_id":"01234567-89ab-cdef-0123-456789abcdef","step_index":1,"state":"DONE","step_type":"agent_response"}}'
  echo '{"event":"result","result":{"conversation_id":"01234567-89ab-cdef-0123-456789abcdef","status":"SUCCESS","response":"hello from agy"}}'
done
"#;

fn env_with_antigravity_chat_agent() -> Env {
    let mut env = Env::start_with(true);
    let script = env.ws.parent().expect("root").join("fake-bin").join("agy");
    std::fs::write(&script, FAKE_ANTIGRAVITY).expect("write");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    env.pump("the agent list", |d| d.agents.knows("antigravity"));
    env.deck.open_tab("antigravity", false);
    env.pump("the chat tab to be running", |d| d.tabs.first().is_some_and(|t| t.alive));
    env
}

#[test]
fn an_antigravity_tab_opens_in_the_chat_view_with_its_own_conversation() {
    let env = env_with_antigravity_chat_agent();
    let tab = &env.deck.tabs[0];
    assert!(tab.chat.is_some(), "chat is the default view of an Antigravity tab");
    let spec = tab.spec.as_ref().expect("spec");
    assert!(spec.args.iter().any(|a| a == "stream-json"), "{:?}", spec.args);
    assert!(spec.args.iter().any(|a| a == "auto"), "{:?}", spec.args);
    assert!(!spec.args.iter().any(|a| a == "--log-file"), "chat mode does not use log file: {:?}", spec.args);
}

#[test]
fn an_antigravity_question_from_another_agent_becomes_a_message_of_the_conversation_and_ends_with_the_turn() {
    let mut env = env_with_antigravity_chat_agent();
    let prompt = "[Message from the agent \"claude@dev\" received by Holt]\n\nCan you help?\n\nAnswer by running in the shell: ask-agent --reply 9 \"your answer\"";
    env.deck.send_to_instance(&Ident::main("antigravity"), false, prompt.to_owned()).expect("queued");
    env.pump("the answer", |d| {
        d.tabs[0]
            .chat
            .as_ref()
            .is_some_and(|c| c.transcript.items.iter().any(|i| matches!(i, Item::Assistant { text, .. } if text == "hello from agy")))
    });
    let items = items(&env);
    assert!(matches!(&items[0], Item::User { text, from: Some(who) } if text == "Can you help?" && who == "claude@dev"), "{items:?}");
    env.pump("the turn to end", |d| !d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.busy()) && !d.tabs[0].busy);
}

#[test]
fn an_antigravity_tab_goes_to_the_terminal_and_back_on_the_same_conversation() {
    let mut env = env_with_antigravity_chat_agent();
    let id = env.deck.tabs[0].ident.id();
    let session = "01234567-89ab-cdef-0123-456789abcdef";
    assert!(env.core.set_tab_session(&env.project(), &id, session));

    // What Antigravity saved of the conversation so far in brain:
    let dir = env
        .ws
        .parent()
        .expect("root")
        .join("home")
        .join(".gemini")
        .join("antigravity-cli")
        .join("brain")
        .join(session)
        .join(".system_generated")
        .join("logs");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let lines = [
        json!({ "step_index": 0, "source": "USER_EXPLICIT", "type": "USER_INPUT", "status": "DONE", "created_at": "2026-10-30T01:00:00Z", "content": "<USER_REQUEST>\nfirst question\n</USER_REQUEST>" }),
        json!({ "step_index": 1, "source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "created_at": "2026-10-30T01:00:01Z", "content": "first answer" }),
    ];
    std::fs::write(dir.join("transcript.jsonl"), lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")).expect("transcript");

    // Also need the conversation .db file for agy_has_conversation:
    let conv_dir = env.ws.parent().expect("root").join("home").join(".gemini").join("antigravity-cli").join("conversations");
    std::fs::create_dir_all(&conv_dir).expect("mkdir");
    std::fs::write(conv_dir.join(format!("{session}.db")), "").expect("db");

    env.deck.set_tab_view(0, false);
    assert!(env.deck.tabs[0].chat.is_none() && env.deck.tabs[0].alive);
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(spec.resumed && spec.args.contains(&session.to_owned()) && !spec.args.iter().any(|a| a == "stream-json"), "{:?}", spec.args);
    assert!(!env.core.tab_is_chat(&env.project(), &env.deck.tabs[0].ident), "the choice is remembered");

    env.deck.set_tab_view(0, true);
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(spec.resumed && spec.args.iter().any(|a| a == "stream-json"));
    let items = items(&env);
    assert_eq!(items.len(), 2, "the conversation comes back from the CLI's own transcript: {items:?}");
    assert!(matches!(&items[0], Item::User { text, .. } if text == "first question"));
    assert_eq!(env.core.tab_session(&env.project(), &id).as_deref(), Some(session), "same conversation");
}

const FAKE_CODEX: &str = r#"#!/bin/sh
if [ "$1" = "app-server" ]; then
  while IFS= read -r line; do
    case "$line" in
      *\"method\":\"initialize\"*)
        id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
        echo "{\"id\":$id,\"result\":{\"userAgent\":\"fake\"}}"
        ;;
      *\"method\":\"thread/resume\"*)
        id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
        echo "{\"id\":$id,\"result\":{\"thread\":{\"id\":\"th-123\",\"turns\":[{\"items\":[{\"type\":\"userMessage\",\"content\":[{\"type\":\"text\",\"text\":\"prior question\"}]},{\"type\":\"agentMessage\",\"id\":\"m1\",\"text\":\"prior answer\"}]}]}}}"
        ;;
      *\"method\":\"thread/start\"*)
        id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
        echo "{\"id\":$id,\"result\":{\"thread\":{\"id\":\"th-123\"}}}"
        ;;
      *\"method\":\"turn/start\"*)
        id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
        echo "{\"id\":$id,\"result\":{\"turn\":{\"id\":\"turn-1\"}}}"
        echo '{"method":"turn/started","params":{"turn":{"id":"turn-1"}}}'
        echo '{"method":"item/completed","params":{"item":{"type":"agentMessage","id":"m2","text":"hello from codex"}}}'
        echo '{"method":"turn/completed","params":{"turn":{"id":"turn-1","status":"completed"}}}'
        ;;
    esac
  done
else
  while IFS= read -r line; do
    :
  done
fi
"#;

fn env_with_codex_chat_agent() -> Env {
    let mut env = Env::start_with(true);
    let script = env.ws.parent().expect("root").join("fake-bin").join("codex");
    std::fs::write(&script, FAKE_CODEX).expect("write");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    env.pump("the agent list", |d| d.agents.knows("codex"));
    env.deck.open_tab("codex", false);
    env.pump("the chat tab to be running", |d| d.tabs.first().is_some_and(|t| t.alive));
    env
}

#[test]
fn a_codex_tab_opens_in_the_chat_view_with_its_own_conversation() {
    let env = env_with_codex_chat_agent();
    let tab = &env.deck.tabs[0];
    assert!(tab.chat.is_some(), "chat is the default view of a Codex tab");
    let spec = tab.spec.as_ref().expect("spec");
    assert!(spec.args.iter().any(|a| a == "app-server"), "{:?}", spec.args);
}

#[test]
fn a_codex_question_from_another_agent_becomes_a_message_of_the_conversation_and_ends_with_the_turn() {
    let mut env = env_with_codex_chat_agent();
    let prompt = "[Message from the agent \"claude@dev\" received by Holt]\n\nCan you help with Codex?\n\nAnswer by running in the shell: ask-agent --reply 10 \"your answer\"";
    env.deck.send_to_instance(&Ident::main("codex"), false, prompt.to_owned()).expect("queued");
    env.pump("the answer", |d| {
        d.tabs[0]
            .chat
            .as_ref()
            .is_some_and(|c| c.transcript.items.iter().any(|i| matches!(i, Item::Assistant { text, .. } if text == "hello from codex")))
    });
    let items = items(&env);
    assert!(
        matches!(&items[0], Item::User { text, from: Some(who) } if text == "Can you help with Codex?" && who == "claude@dev"),
        "{items:?}"
    );
    env.pump("the turn to end", |d| !d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.busy()) && !d.tabs[0].busy);
}

#[test]
fn a_codex_tab_goes_to_the_terminal_and_back_on_the_same_conversation() {
    let mut env = env_with_codex_chat_agent();
    let id = env.deck.tabs[0].ident.id();
    let session = "th-123";

    // A rollout file under ~/.codex/sessions ensures codex_has_session finds it:
    let codex_dir = env.ws.parent().expect("root").join("home").join(".codex").join("sessions").join("2026").join("10").join("30");
    std::fs::create_dir_all(&codex_dir).expect("mkdir");
    std::fs::write(codex_dir.join(format!("rollout-2026-10-30T12-00-00-{session}.jsonl")), "").expect("rollout");

    assert!(env.core.set_tab_session(&env.project(), &id, session));

    env.deck.set_tab_view(0, false);
    assert!(env.deck.tabs[0].chat.is_none() && env.deck.tabs[0].alive);
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(
        spec.resumed
            && spec.args.contains(&session.to_owned())
            && spec.args.contains(&"resume".to_owned())
            && !spec.args.iter().any(|a| a == "app-server"),
        "{:?}",
        spec.args
    );
    assert!(!env.core.tab_is_chat(&env.project(), &env.deck.tabs[0].ident), "the choice is remembered");

    env.deck.set_tab_view(0, true);
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(spec.resumed && spec.args.iter().any(|a| a == "app-server"));
    env.pump("the replay items to load", |d| d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.items.len() >= 2));
    let items = items(&env);
    assert_eq!(items.len(), 2, "the conversation comes back from thread/resume: {items:?}");
    assert!(matches!(&items[0], Item::User { text, .. } if text == "prior question"));
    assert_eq!(env.core.tab_session(&env.project(), &id).as_deref(), Some(session), "same conversation");
}

// ---------- A CLI that speaks ACP (here "goose"), through the generic backend ----------

/// A shell ACP agent: `acp` as its first argument starts the protocol; any other launch (the terminal view) just waits.
/// The ids of Holt's requests are answered as they come. `need-permission` asks and appends the answer it got to
/// `answers.txt`; `slow` waits for `session/cancel`; `crash` exits with code 3 in the middle of the turn; the log of
/// everything received is `acp.log`.
const FAKE_ACP: &str = r#"#!/bin/sh
if [ "$1" != "acp" ]; then
  while IFS= read -r line; do :; done
  exit 0
fi
while IFS= read -r line; do
  printf '%s\n' "$line" >> acp.log
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *\"method\":\"initialize\"*)
      echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true},\"authMethods\":[]}}"
      ;;
    *\"method\":\"session/new\"*)
      echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"sessionId\":\"acp-sess-1\"}}"
      ;;
    *\"method\":\"session/load\"*)
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"prior question"}}}}'
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"prior answer"}}}}'
      echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":null}"
      ;;
    *\"method\":\"session/prompt\"*)
      case "$line" in
        *need-permission*)
          echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"tool_call","toolCallId":"c1","title":"touch x","kind":"execute","status":"pending","rawInput":{"command":"touch x"}}}}'
          echo '{"jsonrpc":"2.0","id":50,"method":"session/request_permission","params":{"sessionId":"acp-sess-1","toolCall":{"toolCallId":"c1","title":"touch x","kind":"execute","rawInput":{"command":"touch x"}},"options":[{"optionId":"once","name":"Allow once","kind":"allow_once"},{"optionId":"always","name":"Always","kind":"allow_always"},{"optionId":"no","name":"Reject","kind":"reject_once"}]}}'
          IFS= read -r answer
          printf '%s\n' "$answer" >> answers.txt
          echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"completed"}}}'
          echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"permission handled"}}}}'
          ;;
        *slow*)
          echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"tool_call","toolCallId":"c2","title":"sleep 60","kind":"execute","status":"in_progress","rawInput":{"command":"sleep 60"}}}}'
          while IFS= read -r next; do
            printf '%s\n' "$next" >> acp.log
            case "$next" in *session/cancel*) break ;; esac
          done
          echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"cancelled\"}}"
          continue
          ;;
        *crash*)
          echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"about to fail"}}}}'
          exit 3
          ;;
        *)
          echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"acp-sess-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello from acp"}}}}'
          ;;
      esac
      echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"end_turn\"}}"
      ;;
  esac
done
"#;

fn env_with_acp_chat_agent() -> Env {
    let mut env = Env::start_with(true);
    let script = env.ws.parent().expect("root").join("fake-bin").join("goose");
    std::fs::write(&script, FAKE_ACP).expect("write");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    env.pump("the agent list", |d| d.agents.knows("goose"));
    env.deck.open_tab("goose", false);
    env.pump("the chat tab to be running", |d| d.tabs.first().is_some_and(|t| t.alive));
    env
}

fn acp_answers(env: &Env) -> Vec<Value> {
    std::fs::read_to_string(env.ws.join("answers.txt")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn send(env: &mut Env, text: &str) {
    env.deck.tabs[0].chat.as_mut().expect("chat").composer = text.into();
    env.deck.chat_submit(0);
}

#[test]
fn an_acp_tab_opens_in_the_chat_view_and_the_conversation_id_is_saved_when_the_agent_names_it() {
    let mut env = env_with_acp_chat_agent();
    let tab = &env.deck.tabs[0];
    assert!(tab.chat.is_some(), "chat is the default view of an ACP agent");
    assert_eq!(tab.spec.as_ref().expect("spec").args, ["acp"]);
    let id = tab.ident.id();
    send(&mut env, "hello");
    env.pump("the answer", |d| !d.tabs[0].busy && d.tabs[0].idle_since.is_some());
    let items = items(&env);
    assert!(matches!(&items[0], Item::User { text, .. } if text == "hello"));
    assert!(matches!(&items[1], Item::Assistant { text, .. } if text == "hello from acp"), "{items:?}");
    assert_eq!(env.core.tab_session(&env.project(), &id).as_deref(), Some("acp-sess-1"), "saved from session/new");
}

#[test]
fn an_acp_question_from_another_agent_becomes_a_message_of_the_conversation() {
    let mut env = env_with_acp_chat_agent();
    let prompt = "[Message from the agent \"claude@dev\" received by Holt]\n\nCan you help?\n\nAnswer by running in the shell: ask-agent --reply 9 \"your answer\"";
    env.deck.send_to_instance(&Ident::main("goose"), false, prompt.to_owned()).expect("queued");
    env.pump("the answer", |d| {
        d.tabs[0]
            .chat
            .as_ref()
            .is_some_and(|c| c.transcript.items.iter().any(|i| matches!(i, Item::Assistant { text, .. } if text == "hello from acp")))
    });
    let items = items(&env);
    assert!(matches!(&items[0], Item::User { text, from: Some(who) } if text == "Can you help?" && who == "claude@dev"), "{items:?}");
    env.pump("the turn to end", |d| !d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.busy()) && !d.tabs[0].busy);
}

#[test]
fn acp_permission_cards_wait_for_the_user_and_each_answer_reaches_the_agent_as_its_option() {
    let mut env = env_with_acp_chat_agent();
    assert_eq!(env.deck.tabs[0].phase, None);
    for (allow, for_session, option) in [(true, false, "once"), (true, true, "always"), (false, false, "no")] {
        send(&mut env, "need-permission please");
        assert!(env.deck.tabs[0].busy, "sending a message starts a turn");
        env.pump("the permission card", |d| d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.waiting_for_permission()));
        assert_eq!(env.deck.tabs[0].phase, Some(holt_core::Phase::WaitingForYou));
        assert_eq!(env.deck.tabs[0].status_label(std::time::Instant::now()).as_deref(), Some("waiting for you"));
        let card = env.deck.tabs[0].chat.as_ref().expect("chat").transcript.pending_permission().expect("card").clone();
        assert_eq!((card.request_id.as_str(), card.tool.as_str(), card.input["command"].as_str()), ("50", "Bash", Some("touch x")));
        assert_eq!(card.suggestions.len(), 1, "the agent offers allow_always: the session button is there");
        env.deck.chat_answer(0, "50", allow, for_session);
        env.pump("the end of the turn", |d| !d.tabs[0].busy);
        assert_eq!(env.deck.tabs[0].phase, None, "phase clears when the turn ends");
        assert!(env.deck.tabs[0].idle_since.is_some());
        assert_eq!(env.deck.tabs[0].status_label(std::time::Instant::now()).as_deref(), Some("idle <1 min"));
        let answers = acp_answers(&env);
        let last = answers.last().expect("an answer");
        assert_eq!(last["id"], 50);
        assert_eq!(last["result"]["outcome"]["outcome"], "selected");
        assert_eq!(last["result"]["outcome"]["optionId"], option);
        assert!(!env.deck.tabs[0].chat.as_ref().expect("chat").transcript.waiting_for_permission());
    }
    assert_eq!(acp_answers(&env).len(), 3);
}

#[test]
fn interrupting_an_acp_turn_cancels_the_session_and_the_tab_goes_idle() {
    let mut env = env_with_acp_chat_agent();
    send(&mut env, "do something slow");
    env.pump("the running call", |d| d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.items.iter().any(|i| matches!(i, Item::Tool(_)))));
    assert!(env.deck.tabs[0].busy);
    assert_eq!(env.deck.tabs[0].phase, Some(holt_core::Phase::RunningCommand));
    env.deck.chat_interrupt(0);
    env.pump("the end of the turn", |d| !d.tabs[0].busy);
    let items = items(&env);
    assert!(items.iter().any(|i| matches!(i, Item::Notice { text, .. } if text == "Interrupted.")), "{items:?}");
    assert!(!env.deck.tabs[0].chat.as_ref().expect("chat").transcript.busy());
    assert_eq!(env.deck.tabs[0].phase, None);
    assert!(env.deck.tabs[0].idle_since.is_some());
    let log = std::fs::read_to_string(env.ws.join("acp.log")).expect("log");
    assert!(log.contains("\"method\":\"session/cancel\""), "{log}");
}

#[test]
fn an_acp_process_that_dies_is_reported_and_start_resumes_the_same_conversation() {
    let mut env = env_with_acp_chat_agent();
    send(&mut env, "hello");
    env.pump("the first answer", |d| !d.tabs[0].busy && d.tabs[0].idle_since.is_some());
    send(&mut env, "crash please");
    env.pump("the process to end", |d| !d.tabs[0].alive);
    let items = items(&env);
    assert!(matches!(items.last(), Some(Item::Notice { text, error: true }) if text.contains("code 3")), "{items:?}");
    assert!(!env.deck.tabs[0].chat.as_ref().expect("chat").transcript.busy());
    let before = items.len();

    // "Restart": the new process loads the conversation by its id; what the view already shows is not repeated.
    env.deck.start(0);
    env.pump("the process to be running again", |d| d.tabs[0].alive);
    assert_eq!(env.deck.tabs[0].spec.as_ref().expect("spec").session.as_deref(), Some("acp-sess-1"));
    send(&mut env, "and now");
    env.pump("the answer", |d| !d.tabs[0].busy && d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.items.len() > before));
    let log = std::fs::read_to_string(env.ws.join("acp.log")).expect("log");
    assert!(log.contains("\"method\":\"session/load\"") && log.contains("\"sessionId\":\"acp-sess-1\""), "{log}");
    let after = self::items(&env);
    assert_eq!(after.iter().filter(|i| matches!(i, Item::User { text, .. } if text == "prior question")).count(), 0, "{after:?}");
}

#[test]
fn an_acp_tab_goes_to_the_terminal_and_back_and_the_conversation_is_loaded_from_the_agent() {
    let mut env = env_with_acp_chat_agent();
    let id = env.deck.tabs[0].ident.id();
    send(&mut env, "hello");
    env.pump("the first answer", |d| !d.tabs[0].busy && d.tabs[0].idle_since.is_some());
    assert_eq!(env.core.tab_session(&env.project(), &id).as_deref(), Some("acp-sess-1"));

    env.deck.set_tab_view(0, false);
    assert!(env.deck.tabs[0].chat.is_none() && env.deck.tabs[0].alive);
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert!(!spec.args.iter().any(|a| a == "acp"), "the terminal launch does not start the protocol: {:?}", spec.args);
    assert!(!spec.resumed, "a terminal launch that carries no session id is not a resume");
    assert!(!env.core.tab_is_chat(&env.project(), &env.deck.tabs[0].ident), "the choice is remembered");

    env.deck.set_tab_view(0, true);
    env.pump("the history", |d| d.tabs[0].chat.as_ref().is_some_and(|c| c.transcript.items.len() >= 2));
    let spec = env.deck.tabs[0].spec.as_ref().expect("spec");
    assert_eq!((spec.args.as_slice(), spec.session.as_deref()), (&["acp".to_owned()][..], Some("acp-sess-1")));
    let items = items(&env);
    assert_eq!(items.len(), 2, "the conversation comes back from session/load: {items:?}");
    assert!(matches!(&items[0], Item::User { text, .. } if text == "prior question"));
    assert!(matches!(&items[1], Item::Assistant { text, .. } if text == "prior answer"));
    assert_eq!(env.core.tab_session(&env.project(), &id).as_deref(), Some("acp-sess-1"), "same conversation");
}

#[test]
fn the_model_and_effort_pickers_do_not_apply_to_an_acp_agent_that_has_none() {
    let mut env = env_with_acp_chat_agent();
    let spawned = env.deck.tabs[0].spawned_at;
    env.deck.chat_set_model(0, Some("some-model"));
    env.deck.chat_set_effort(0, Some(holt_core::models::Effort::High));
    assert_eq!(env.deck.tabs[0].spawned_at, spawned, "nothing was restarted");
    let notices = items(&env).into_iter().filter(|i| matches!(i, Item::Notice { error: true, .. })).count();
    assert_eq!(notices, 2, "{:?}", items(&env));
    assert!(holt_core::models::effort_unavailable("goose", None).is_some());
    assert!(!holt_core::models::has_model_list("goose"));
}
