//! Event parsing and the transcript, from real recordings.

use serde_json::{json, Value};

use super::*;

fn events(sample: &str) -> Vec<ChatEvent> {
    sample.lines().flat_map(parse_line).collect()
}

const TOOLS: &str = include_str!("../../tests/data/claude-stream-tools.jsonl");
const TEXT: &str = include_str!("../../tests/data/claude-stream-text.jsonl");
const INTERRUPT: &str = include_str!("../../tests/data/claude-stream-interrupt.jsonl");

const AGY_TEXT: &str = include_str!("../../tests/data/antigravity-stream-text.jsonl");
const AGY_TOOLS: &str = include_str!("../../tests/data/antigravity-stream-tools.jsonl");
const AGY_ERROR: &str = include_str!("../../tests/data/antigravity-stream-error.jsonl");
const AGY_INTERRUPT: &str = include_str!("../../tests/data/antigravity-stream-interrupt.jsonl");

fn replay(sample: &str) -> Transcript {
    let mut t = Transcript::default();
    t.push_user("go", None);
    for ev in events(sample) {
        t.apply(&ev);
    }
    t
}

#[test]
fn the_init_event_names_the_session_model_and_commands() {
    let evs = events(TOOLS);
    let Some(ChatEvent::Init { session, model, commands, terminal_commands }) = evs.first() else { panic!("{:?}", evs.first()) };
    assert_eq!(session.len(), 36);
    assert_eq!(model, "claude-opus-5-5");
    assert!(!commands.is_empty());
    assert!(terminal_commands.contains(&"doctor".to_owned()));
}

#[test]
fn text_deltas_stream_into_one_assistant_item_that_the_final_message_completes() {
    let evs = events(TEXT);
    let deltas = evs.iter().filter(|e| matches!(e, ChatEvent::TextDelta { .. })).count();
    assert!(deltas > 5, "{deltas}");
    let t = replay(TEXT);
    let texts: Vec<&Item> = t.items.iter().filter(|i| matches!(i, Item::Assistant { .. })).collect();
    assert_eq!(texts.len(), 1, "{:?}", t.items);
    let Item::Assistant { text, streaming, .. } = texts[0] else { unreachable!() };
    assert!(!streaming);
    assert!(text.starts_with("I couldn't create `hello.txt`"), "{text}");
    assert!(text.ends_with("try again?"));
}

#[test]
fn a_tool_call_gets_its_result_and_the_permission_card_is_cancelled_at_the_end_of_the_turn() {
    let t = replay(TEXT);
    let Some(Item::Tool(call)) = t.items.iter().find(|i| matches!(i, Item::Tool(_))) else { panic!("{:?}", t.items) };
    assert_eq!(call.name, "Write");
    assert_eq!(call.input["content"], "hi");
    assert!(call.outcome.as_ref().is_some_and(|o| o.is_error));
    let Some(Item::Permission(card)) = t.items.iter().find(|i| matches!(i, Item::Permission(_))) else { panic!() };
    assert_eq!(card.tool, "Write");
    assert_eq!(card.suggestions.len(), 1);
    assert_eq!(card.state, PermState::Cancelled);
    assert!(!t.waiting_for_permission());
}

#[test]
fn a_permission_request_waits_until_it_is_answered() {
    let mut t = Transcript::default();
    t.push_user("write it", None);
    for ev in events(TEXT).into_iter().take_while(|e| !matches!(e, ChatEvent::ToolResults(_))) {
        t.apply(&ev);
    }
    assert!(t.busy());
    let card = t.pending_permission().expect("a pending card").clone();
    assert_eq!(card.tool, "Write");
    t.answer_permission(&card.request_id, true);
    assert!(t.pending_permission().is_none());
    assert!(
        matches!(t.items.iter().find(|i| matches!(i, Item::Permission(_))), Some(Item::Permission(p)) if p.state == PermState::Allowed)
    );
}

#[test]
fn a_session_with_edits_has_tools_with_results_and_ends_the_turn() {
    let t = replay(TOOLS);
    let names: Vec<&str> = t.items.iter().filter_map(|i| if let Item::Tool(c) = i { Some(c.name.as_str()) } else { None }).collect();
    assert_eq!(names, ["Write", "Edit", "Bash"]);
    for item in &t.items {
        if let Item::Tool(c) = item {
            assert!(c.outcome.as_ref().is_some_and(|o| !o.is_error), "{}", c.name);
        }
    }
    assert!(t.cost_usd > 0.0);
    assert!(t.session.is_some());
}

#[test]
fn two_results_are_two_turns() {
    // The sample has two user turns (a task and `/context`): `replay` sent one message, so it ends with one pending.
    assert_eq!(events(TOOLS).iter().filter(|e| matches!(e, ChatEvent::Result(_))).count(), 2);
    assert!(replay(TOOLS).pending_turns == 0);
}

#[test]
fn a_slash_command_answers_with_a_plain_assistant_message() {
    let t = replay(TOOLS);
    let Some(Item::Assistant { text, .. }) = t.items.last() else { panic!("{:?}", t.items.last()) };
    assert!(text.starts_with("## Context Usage"), "{text}");
}

#[test]
fn an_interrupt_is_noticed_and_the_next_turn_works() {
    let evs = events(INTERRUPT);
    assert!(evs.contains(&ChatEvent::Interrupted));
    let t = replay(INTERRUPT);
    assert!(t.items.iter().any(|i| matches!(i, Item::Notice { text, .. } if text == "Interrupted.")));
    assert!(matches!(t.items.last(), Some(Item::Assistant { text, .. }) if text == "OK"));
}

#[test]
fn what_goes_to_stdin_is_one_json_object() {
    let m: Value = serde_json::from_str(&user_message_json("hi\nthere")).unwrap();
    assert_eq!((m["type"].as_str(), m["message"]["content"].as_str()), (Some("user"), Some("hi\nthere")));
    let input = json!({ "file_path": "/x" });
    let ok: Value = serde_json::from_str(&permission_response_json("r1", true, &input, "", &[])).unwrap();
    assert_eq!(ok["response"]["request_id"], "r1");
    assert_eq!(ok["response"]["response"]["behavior"], "allow");
    assert_eq!(ok["response"]["response"]["updatedInput"], input);
    let rules = [json!({ "type": "setMode", "mode": "acceptEdits", "destination": "session" })];
    let session: Value = serde_json::from_str(&permission_response_json("r3", true, &input, "", &rules)).unwrap();
    assert_eq!(session["response"]["response"]["updatedPermissions"], json!(rules));
    let no: Value = serde_json::from_str(&permission_response_json("r2", false, &input, "No.", &[])).unwrap();
    assert_eq!(
        (no["response"]["response"]["behavior"].as_str(), no["response"]["response"]["message"].as_str()),
        (Some("deny"), Some("No."))
    );
    let i: Value = serde_json::from_str(&interrupt_json()).unwrap();
    assert_eq!(i["request"]["subtype"], "interrupt");
}

#[test]
fn unknown_requests_and_stray_lines() {
    assert!(parse_line("not json").is_empty());
    assert!(parse_line(r#"{"type":"rate_limit_event"}"#).is_empty());
    let ev = parse_line(
        r#"{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0.3,"resetsAt":1793506200}}}}"#,
    );
    assert!(matches!(&ev[..], [ChatEvent::Usage(u)] if u.session.is_some_and(|w| w.used == 0.3) && u.weekly.is_none()), "{ev:?}");
    let ev = parse_line(r#"{"type":"control_request","request_id":"a","request":{"subtype":"hook_callback"}}"#);
    assert_eq!(ev, vec![ChatEvent::UnknownRequest { request_id: "a".into() }]);
    assert_eq!(parse_line(r#"{"type":"control_cancel_request","request_id":"a"}"#), vec![ChatEvent::PermissionCancelled("a".into())]);
}

#[test]
fn a_subagents_messages_are_not_the_conversation() {
    let line = r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{"id":"m","content":[{"type":"text","text":"x"}]}}"#;
    assert!(parse_line(line).is_empty());
}

#[test]
fn long_tool_output_is_cut() {
    let big = "x".repeat(RESULT_MAX_CHARS + 50);
    let line = json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t", "content": big }] } });
    let Some(ChatEvent::ToolResults(r)) = parse_line(&line.to_string()).pop() else { panic!() };
    assert!(r[0].text.ends_with("(50 more characters)"));
}

#[test]
fn an_edit_result_carries_the_patch_with_its_line_numbers() {
    let t = replay(TOOLS);
    let edit = t.items.iter().find_map(|i| match i {
        Item::Tool(c) if c.name == "Edit" => c.outcome.clone(),
        _ => None,
    });
    let patch = edit.expect("the Edit result").patch;
    assert_eq!(patch.len(), 1);
    assert_eq!((patch[0].old_start, patch[0].new_start), (1, 1));
    // The "no newline" markers are not lines of the file.
    assert_eq!(patch[0].lines, ["-hi", "+hello"]);
}

#[test]
fn the_context_percent_comes_from_the_last_message_and_the_window() {
    let mut t = replay(TOOLS);
    assert!(t.context_tokens > 0);
    assert_eq!(t.context_percent(), None, "the window is unknown until a result says it");
    let line = r#"{"type":"result","is_error":false,"total_cost_usd":0.1,"session_id":"s","modelUsage":{"claude-opus-5-5":{"contextWindow":1000000}}}"#;
    for ev in parse_line(line) {
        t.apply(&ev);
    }
    t.context_tokens = 310_000;
    assert_eq!(t.context_percent(), Some(31));
}

#[test]
fn the_history_of_a_saved_conversation_shows_what_was_said() {
    let jsonl = [
        json!({ "type": "queue-operation" }),
        json!({ "type": "user", "message": { "content": "Add a menu entry" } }),
        json!({ "type": "attachment" }),
        json!({ "type": "assistant", "message": { "id": "m1", "content": [{ "type": "tool_use", "id": "t1", "name": "Edit", "input": { "file_path": "a.rs" } }] } }),
        json!({ "type": "user", "toolUseResult": { "structuredPatch": [{ "oldStart": 4, "newStart": 4, "lines": [" a", "-b", "+c"] }] }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "ok" }] } }),
        json!({ "type": "user", "isMeta": true, "message": { "content": "<local-command-caveat>x</local-command-caveat>" } }),
        json!({ "type": "user", "message": { "content": "<command-name>/context</command-name>\n<command-message>context</command-message>\n<command-args></command-args>" } }),
        json!({ "type": "user", "isSidechain": true, "message": { "content": "sub-agent prompt" } }),
        json!({ "type": "assistant", "message": { "id": "m2", "content": [{ "type": "text", "text": "Done." }] } }),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    let t = Transcript::from_history(&jsonl, 100);
    assert!(!t.busy());
    assert_eq!(t.items.len(), 4, "{:?}", t.items);
    assert!(matches!(&t.items[0], Item::User { text, .. } if text == "Add a menu entry"));
    assert!(matches!(&t.items[1], Item::Tool(c) if c.name == "Edit" && c.outcome.as_ref().is_some_and(|o| o.patch[0].old_start == 4)));
    assert!(matches!(&t.items[2], Item::User { text, .. } if text == "/context"));
    assert!(matches!(&t.items[3], Item::Assistant { text, .. } if text == "Done."));
    assert_eq!(Transcript::from_history(&jsonl, 2).items.len(), 2);
}

#[test]
fn terminal_only_commands_are_recognised() {
    assert_eq!(terminal_only_command("/config", &[]), Some("config".into()));
    assert_eq!(terminal_only_command("  /Theme dark", &[]), Some("theme".into()));
    assert_eq!(terminal_only_command("/doctor", &["doctor".into()]), Some("doctor".into()));
    assert_eq!(terminal_only_command("/context", &["doctor".into()]), None);
    assert_eq!(terminal_only_command("config", &[]), None);
}

#[test]
fn a_dead_process_frees_the_tab_and_withdraws_the_cards() {
    let mut t = Transcript::default();
    t.push_user("x", None);
    t.apply(&ChatEvent::Permission(PermissionRequest {
        request_id: "r".into(),
        tool_use_id: "t".into(),
        tool: "Bash".into(),
        input: Value::Null,
        description: String::new(),
        suggestions: vec![],
    }));
    assert!(t.busy() && t.waiting_for_permission());
    t.process_ended();
    assert!(!t.busy() && !t.waiting_for_permission());
}

#[test]
fn history_files_are_found_by_session_id() {
    let dir = std::env::temp_dir().join(format!("holt-chat-hist-{}", std::process::id()));
    let proj = dir.join("projects").join("-some-folder");
    std::fs::create_dir_all(&proj).unwrap();
    let id = "11111111-2222-4333-8444-555555555555";
    std::fs::write(proj.join(format!("{id}.jsonl")), "{}").unwrap();
    assert!(history_file(&dir, id).is_some());
    assert!(history_file(&dir, "22222222-2222-4333-8444-555555555555").is_none());
    assert!(history_file(&dir, "../x").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn antigravity_init_names_the_session() {
    let evs = events(AGY_TEXT);
    let Some(ChatEvent::Init { session, model, .. }) = evs.first() else { panic!("{:?}", evs.first()) };
    assert_eq!(session, "fc83064b-5e28-4d0b-8e95-4a21f9bbb02f");
    assert_eq!(model, "antigravity");
}

#[test]
fn antigravity_text_deltas_stream_into_assistant_message() {
    let evs = events(AGY_TEXT);
    let deltas = evs.iter().filter(|e| matches!(e, ChatEvent::TextDelta { .. })).count();
    assert!(deltas > 0, "{deltas}");
    let t = replay(AGY_TEXT);
    assert_eq!(t.pending_turns, 0);
    let texts: Vec<&Item> = t.items.iter().filter(|i| matches!(i, Item::Assistant { .. })).collect();
    assert_eq!(texts.len(), 1, "{:?}", t.items);
    let Item::Assistant { text, streaming, .. } = texts[0] else { unreachable!() };
    assert!(!streaming);
    assert!(text.contains("Google Antigravity"), "{text}");
}

#[test]
fn antigravity_tools_with_edits_and_patches() {
    let t = replay(AGY_TOOLS);
    assert_eq!(t.pending_turns, 0);
    let names: Vec<&str> = t.items.iter().filter_map(|i| if let Item::Tool(c) = i { Some(c.name.as_str()) } else { None }).collect();
    assert_eq!(names, ["read_file", "write_file", "edit_file"]);
    for item in &t.items {
        if let Item::Tool(c) = item {
            assert!(c.outcome.as_ref().is_some_and(|o| !o.is_error), "{}", c.name);
        }
    }
    let Some(Item::Assistant { text, .. }) = t.items.last() else { panic!("{:?}", t.items.last()) };
    assert!(text.contains("Done:"), "{text}");
}

#[test]
fn antigravity_tool_error_is_recorded() {
    let t = replay(AGY_ERROR);
    assert_eq!(t.pending_turns, 0);
    let tool = t
        .items
        .iter()
        .find_map(|i| match i {
            Item::Tool(c) => Some(c),
            _ => None,
        })
        .expect("tool call");
    assert_eq!(tool.name, "read_file");
    let outcome = tool.outcome.as_ref().expect("outcome");
    assert!(outcome.is_error, "expected error outcome");
    assert!(
        outcome.text.contains("No such file or directory")
            || outcome.text.contains("File not found")
            || outcome.text.contains("does not exist"),
        "{}",
        outcome.text
    );
}

#[test]
fn antigravity_interrupt_is_noticed() {
    let evs = events(AGY_INTERRUPT);
    assert!(evs.contains(&ChatEvent::Interrupted));
    let t = replay(AGY_INTERRUPT);
    assert!(t.items.iter().any(|i| matches!(i, Item::Notice { text, .. } if text == "Interrupted.")));
}

#[test]
fn antigravity_stdin_message_format() {
    let claude_msg: Value = serde_json::from_str(&user_message_json("test")).unwrap();
    assert_eq!(claude_msg["type"], "user");
    assert_eq!(claude_msg["message"]["content"], "test");

    let agy_msg: Value = serde_json::from_str(&agy_user_message_json("test")).unwrap();
    assert_eq!(agy_msg["event"], "user");
    assert_eq!(agy_msg["message"]["content"], "test");
}

#[test]
fn antigravity_history_parses_transcript_and_extracts_patches() {
    let sample = r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-10-30T01:00:00Z","content":"<USER_REQUEST>\nFix the bug\n</USER_REQUEST>"}
{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-30T01:00:01Z","thinking":"I should edit the file.","tool_calls":[{"name":"edit_file","args":{"Path":"\"main.rs\"","Replacement":"\"fn main() { println!(\\\"fixed\\\"); }\"","Target":"\"fn main() {}\""}}]}
{"step_index":2,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-30T01:00:02Z","content":"Made 1 replacement."}
{"step_index":3,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-30T01:00:03Z","content":"Bug is fixed."}"#;

    let t = from_agy_history(sample, 50);
    assert_eq!(t.items.len(), 4);
    assert!(matches!(&t.items[0], Item::User { text, .. } if text == "Fix the bug"));
    assert!(matches!(&t.items[1], Item::Thinking(text) if text == "I should edit the file."));
    let Item::Tool(tool) = &t.items[2] else { panic!() };
    assert_eq!(tool.name, "edit_file");
    let outcome = tool.outcome.as_ref().unwrap();
    assert_eq!(outcome.patch.len(), 1);
    assert_eq!(outcome.patch[0].lines, ["-fn main() {}", "+fn main() { println!(\"fixed\"); }"]);
    assert!(matches!(&t.items[3], Item::Assistant { text, .. } if text == "Bug is fixed."));
}

#[test]
fn antigravity_history_file_lookup() {
    let dir = std::env::temp_dir().join(format!("holt-agy-hist-{}", std::process::id()));
    let session = "12345678-1234-1234-1234-123456789abc";
    let logs_dir = dir.join(".gemini").join("antigravity-cli").join("brain").join(session).join(".system_generated").join("logs");
    std::fs::create_dir_all(&logs_dir).unwrap();
    std::fs::write(logs_dir.join("transcript.jsonl"), "{}").unwrap();

    assert!(agy_history_file(&dir, session).is_some());
    assert!(agy_history_file(&dir, "00000000-0000-0000-0000-000000000000").is_none());
    assert!(agy_history_file(&dir, "../escape").is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn commands_and_tools_map_to_their_phase() {
    for cmd in [
        "cargo test",
        "cargo test --workspace --locked",
        "cargo +nightly test -p holt-core",
        "RUST_BACKTRACE=1 cargo nextest run",
        "npm test",
        "npm run test",
        "pnpm test",
        "yarn test",
        "bun test",
        "pytest -q",
        "python3 -m pytest tests/",
        "go test ./...",
        "cd app && npm test",
    ] {
        assert_eq!(phase_for_command(cmd), Phase::Testing, "{cmd}");
        assert_eq!(Phase::from_tool("Bash", &json!({ "command": cmd })), Some(Phase::Testing), "{cmd}");
        assert_eq!(Phase::from_tool("run_command", &json!({ "CommandLine": cmd })), Some(Phase::Testing), "{cmd}");
    }
    for cmd in [
        "cargo build",
        "cargo build --release",
        "cargo check --workspace",
        "cargo clippy --workspace --all-targets -- -D warnings",
        "npm run build",
        "pnpm build",
        "go build ./...",
        "cmake --build build",
        "tsc --noEmit",
    ] {
        assert_eq!(phase_for_command(cmd), Phase::Building, "{cmd}");
        assert_eq!(Phase::from_tool("Bash", &json!({ "command": cmd })), Some(Phase::Building), "{cmd}");
    }
    for cmd in ["git status", "ls -la", "ask-agent --learn src/lib.rs", "echo hello"] {
        assert_eq!(phase_for_command(cmd), Phase::RunningCommand, "{cmd}");
    }

    for tool in ["Edit", "Write", "MultiEdit", "NotebookEdit", "edit_file", "write_file"] {
        assert_eq!(Phase::from_tool(tool, &json!({})), Some(Phase::Editing), "{tool}");
    }
    for tool in ["Read", "Grep", "Glob", "LS", "read_file", "grep_search", "find_by_name", "list_dir"] {
        assert_eq!(Phase::from_tool(tool, &json!({})), Some(Phase::Reading), "{tool}");
    }
    for tool in ["WebSearch", "WebFetch", "search_web", "read_url_content"] {
        assert_eq!(Phase::from_tool(tool, &json!({})), Some(Phase::SearchingWeb), "{tool}");
    }
    assert_eq!(Phase::from_tool("TodoWrite", &json!({})), None);
}

#[test]
fn transcript_phase_tracks_the_running_turn_and_clears_when_idle() {
    let mut t = Transcript::default();
    assert_eq!(t.phase(), None);

    t.push_user("fix and test", None);
    assert_eq!(t.phase(), None);

    // Streaming text -> writing.
    t.apply(&ChatEvent::TextDelta { message: "m1".into(), text: "Looking…".into() });
    assert_eq!(t.phase(), Some(Phase::Writing));

    // Tool calls update the phase as the turn progresses.
    t.apply(&ChatEvent::Assistant {
        message: "m1".into(),
        blocks: vec![Block::ToolUse { id: "u1".into(), name: "Read".into(), input: json!({ "file_path": "a.rs" }) }],
        context_tokens: 0,
    });
    assert_eq!(t.phase(), Some(Phase::Reading));

    t.apply(&ChatEvent::Assistant {
        message: "m2".into(),
        blocks: vec![Block::ToolUse { id: "u2".into(), name: "Edit".into(), input: json!({ "file_path": "a.rs" }) }],
        context_tokens: 0,
    });
    assert_eq!(t.phase(), Some(Phase::Editing));

    t.apply(&ChatEvent::Assistant {
        message: "m3".into(),
        blocks: vec![Block::ToolUse { id: "u3".into(), name: "Bash".into(), input: json!({ "command": "cargo test --workspace" }) }],
        context_tokens: 0,
    });
    assert_eq!(t.phase(), Some(Phase::Testing));

    // A pending permission takes precedence until answered.
    t.apply(&ChatEvent::Permission(PermissionRequest {
        request_id: "r1".into(),
        tool_use_id: "u3".into(),
        tool: "Bash".into(),
        input: json!({ "command": "cargo test --workspace" }),
        description: String::new(),
        suggestions: Vec::new(),
    }));
    assert_eq!(t.phase(), Some(Phase::WaitingForYou));
    t.answer_permission("r1", true);
    assert_eq!(t.phase(), Some(Phase::Testing));

    // Turn finishes -> idle (no phase).
    t.apply(&ChatEvent::Result(TurnResult::default()));
    assert!(!t.busy());
    assert_eq!(t.phase(), None);
}

#[test]
fn idle_time_rounds_to_minutes_and_hours() {
    use std::time::Duration;
    crate::i18n::english(|| {
        assert_eq!(idle_label(Duration::from_secs(0)), "idle <1 min");
        assert_eq!(idle_label(Duration::from_secs(59)), "idle <1 min");
        assert_eq!(idle_label(Duration::from_secs(60)), "idle 1 min");
        assert_eq!(idle_label(Duration::from_secs(12 * 60 + 30)), "idle 12 min");
        assert_eq!(idle_label(Duration::from_secs(59 * 60 + 59)), "idle 59 min");
        assert_eq!(idle_label(Duration::from_secs(60 * 60)), "idle 1 h");
        assert_eq!(idle_label(Duration::from_secs(3 * 3600 + 900)), "idle 3 h");
    });
    crate::i18n::with_lang(crate::i18n::Lang::PtBr, || {
        assert_eq!(idle_label(Duration::from_secs(10)), "ocioso <1 min");
        assert_eq!(idle_label(Duration::from_secs(12 * 60)), "ocioso 12 min");
        assert_eq!(idle_label(Duration::from_secs(2 * 3600)), "ocioso 2 h");
    });
}
