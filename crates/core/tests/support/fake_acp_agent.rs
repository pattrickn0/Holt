//! A small Agent Client Protocol agent for the chat tests: a real binary (the `holt-core` test `[[bin]]`, see `Cargo.toml`) that
//! behaves the same on Windows and Unix, in place of a shell script that could not hold a dialogue.
//!
//! The test copies it into the search folder under the name of the CLI it plays (`opencode`, `goose`...) and writes the
//! behaviour next to it, in `<name>.fake.json` (headless clears the environment, see `memory/learnings/fake-agent-test-binary.md`).
//! JSON fields (all optional):
//!
//! - `load_session`: advertise `loadSession` (the agent can resume a conversation with `session/load`);
//! - `models`: put `models` in the answer of `session/new`;
//! - `auth_required`: answer `session/new` with the "auth required" error;
//! - `log`: a file where every line received from Holt is appended, to check what Holt sent.
//!
//! What a turn does depends on a word in the prompt: `think`, `tool`, `edit`, `permission`, `slow` (waits for
//! `session/cancel`), `crash` (exits with code 3 in the middle of the turn), `garbage` (prints a line that is not JSON),
//! `request` (asks Holt for a method it never advertised); any other text is answered with `echo: <text>`.
//! A `session/load` of `gone` fails; any other id replays a short conversation.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

struct Agent {
    spec: Value,
    lines: std::io::Lines<std::io::StdinLock<'static>>,
}

impl Agent {
    fn send(&self, v: Value) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{v}");
        let _ = out.flush();
    }

    fn raw(&self, line: &str) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }

    fn next(&mut self) -> Option<Value> {
        loop {
            let line = self.lines.next()?.ok()?;
            if let Some(path) = self.spec["log"].as_str() {
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("log");
                let _ = writeln!(f, "{line}");
            }
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                return Some(v);
            }
        }
    }

    fn update(&self, session: &str, update: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": session, "update": update } }));
    }

    fn chunk(&self, session: &str, kind: &str, text: &str) {
        self.update(session, json!({ "sessionUpdate": kind, "content": { "type": "text", "text": text } }));
    }
}

fn main() {
    let exe = std::env::current_exe().expect("path of the fake agent");
    let path = exe.with_extension("fake.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("no behaviour of the fake agent at {}: {e}", path.display()));
    let spec: Value = serde_json::from_str(&text).expect("behaviour");
    let mut agent = Agent { spec, lines: std::io::stdin().lock().lines() };
    let mut session = String::from("sess-1");

    while let Some(msg) = agent.next() {
        let (id, method) = (msg.get("id").cloned(), msg["method"].as_str().unwrap_or("").to_owned());
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match method.as_str() {
            "initialize" => {
                let caps = json!({ "loadSession": agent.spec["load_session"].as_bool().unwrap_or(false) });
                let auth = if agent.spec["auth_required"].as_bool().unwrap_or(false) { json!([{ "id": "login", "name": "Log in" }]) } else { json!([]) };
                agent.send(json!({ "jsonrpc": "2.0", "id": id, "result": { "protocolVersion": 1, "agentCapabilities": caps, "authMethods": auth } }));
            }
            "session/new" => {
                if agent.spec["auth_required"].as_bool().unwrap_or(false) {
                    agent.send(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": "Authentication required" } }));
                    continue;
                }
                session = "sess-new".into();
                let mut result = json!({ "sessionId": session });
                if agent.spec["models"].as_bool().unwrap_or(false) {
                    result["models"] = json!({
                        "currentModelId": "fast",
                        "availableModels": [{ "modelId": "fast", "name": "Fast One" }, { "modelId": "deep", "name": "Deep One" }],
                    });
                }
                agent.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
                agent.update(&session, json!({ "sessionUpdate": "available_commands_update", "availableCommands": [{ "name": "review", "description": "Review" }] }));
            }
            "session/load" => {
                let wanted = params["sessionId"].as_str().unwrap_or("").to_owned();
                if wanted == "gone" {
                    agent.send(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": "Session not found" } }));
                    continue;
                }
                session = wanted;
                agent.chunk(&session, "user_message_chunk", "What is in ");
                agent.chunk(&session, "user_message_chunk", "main.rs?");
                agent.chunk(&session, "agent_message_chunk", "It has a ");
                agent.chunk(&session, "agent_message_chunk", "main function.");
                agent.update(&session, json!({
                    "sessionUpdate": "tool_call", "toolCallId": "old-1", "title": "cat main.rs", "kind": "read",
                    "status": "completed", "locations": [{ "path": "/work/main.rs" }],
                    "content": [{ "type": "content", "content": { "type": "text", "text": "fn main() {}" } }],
                }));
                agent.send(json!({ "jsonrpc": "2.0", "id": id, "result": null }));
            }
            "session/prompt" => {
                let prompt = params["prompt"][0]["text"].as_str().unwrap_or("").to_owned();
                let stop = turn(&mut agent, &session, &prompt);
                agent.send(json!({ "jsonrpc": "2.0", "id": id, "result": { "stopReason": stop } }));
            }
            _ => {}
        }
    }
}

/// One turn; returns the stop reason.
fn turn(agent: &mut Agent, session: &str, prompt: &str) -> &'static str {
    let has = |word: &str| prompt.contains(word);
    if has("garbage") {
        agent.raw("this is not json {");
        agent.raw("{\"jsonrpc\":\"2.0\"");
    }
    if has("think") {
        agent.chunk(session, "agent_thought_chunk", "Let me ");
        agent.chunk(session, "agent_thought_chunk", "think.");
    }
    if has("request") {
        agent.send(json!({ "jsonrpc": "2.0", "id": 900, "method": "fs/read_text_file", "params": { "sessionId": session, "path": "/etc/passwd" } }));
        // Holt answers with an error; the turn goes on.
        let _ = agent.next();
    }
    if has("tool") {
        agent.chunk(session, "agent_message_chunk", "Running it.");
        agent.update(session, json!({
            "sessionUpdate": "tool_call", "toolCallId": "call-1", "title": "echo hi", "kind": "execute", "status": "pending",
            "rawInput": { "command": "echo hi" },
        }));
        agent.update(session, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "call-1", "status": "in_progress" }));
        agent.update(session, json!({
            "sessionUpdate": "tool_call_update", "toolCallId": "call-1", "status": "completed",
            "content": [{ "type": "content", "content": { "type": "text", "text": "hi\n" } }],
        }));
        agent.chunk(session, "agent_message_chunk", "It printed hi.");
    } else if has("edit") {
        agent.update(session, json!({
            "sessionUpdate": "tool_call", "toolCallId": "call-2", "title": "Edit notes.txt", "kind": "edit", "status": "pending",
            "locations": [{ "path": "/work/notes.txt" }],
        }));
        agent.update(session, json!({
            "sessionUpdate": "tool_call_update", "toolCallId": "call-2", "status": "completed",
            "content": [{ "type": "diff", "path": "/work/notes.txt", "oldText": "one\ntwo\nthree\n", "newText": "one\nTWO\nthree\n" }],
        }));
        agent.chunk(session, "agent_message_chunk", "Edited.");
    } else if has("permission") {
        agent.update(session, json!({
            "sessionUpdate": "tool_call", "toolCallId": "call-3", "title": "rm -rf build", "kind": "execute", "status": "pending",
            "rawInput": { "command": "rm -rf build" },
        }));
        agent.send(json!({
            "jsonrpc": "2.0", "id": 77, "method": "session/request_permission",
            "params": {
                "sessionId": session,
                "toolCall": { "toolCallId": "call-3", "title": "rm -rf build", "kind": "execute", "status": "pending", "rawInput": { "command": "rm -rf build" } },
                "options": [
                    { "optionId": "once", "name": "Allow once", "kind": "allow_once" },
                    { "optionId": "always", "name": "Always allow", "kind": "allow_always" },
                    { "optionId": "no", "name": "Reject", "kind": "reject_once" },
                ],
            },
        }));
        // The answer is the next line from Holt.
        let answer = agent.next().unwrap_or(Value::Null);
        let outcome = &answer["result"]["outcome"];
        let (status, text) = match (outcome["outcome"].as_str(), outcome["optionId"].as_str()) {
            (Some("selected"), Some("once")) => ("completed", "allowed once"),
            (Some("selected"), Some("always")) => ("completed", "allowed always"),
            (Some("selected"), _) => ("failed", "denied"),
            _ => ("failed", "cancelled"),
        };
        agent.update(session, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "call-3", "status": status }));
        agent.chunk(session, "agent_message_chunk", text);
    } else if has("slow") {
        agent.chunk(session, "agent_message_chunk", "Working");
        agent.update(session, json!({
            "sessionUpdate": "tool_call", "toolCallId": "call-4", "title": "sleep 60", "kind": "execute", "status": "in_progress",
            "rawInput": { "command": "sleep 60" },
        }));
        while let Some(msg) = agent.next() {
            if msg["method"] == "session/cancel" {
                return "cancelled";
            }
        }
        return "cancelled";
    } else if has("crash") {
        agent.chunk(session, "agent_message_chunk", "About to fail");
        std::process::exit(3);
    } else {
        agent.chunk(session, "agent_message_chunk", &format!("echo: {prompt}"));
    }
    "end_turn"
}
