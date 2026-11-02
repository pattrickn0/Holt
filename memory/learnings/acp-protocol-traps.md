---
title: ACP (Agent Client Protocol) traps behind the generic chat backend
summary: The flags per CLI come from the ACP registry (not from memory), session/load streams the history before it answers, a cancel must also answer the open permission requests, and the recorded samples are written from the schema, not recorded.
touches: [crates/core/src/chat/acp.rs, crates/core/tests/data/acp-*.jsonl, crates/core/tests/support/fake_acp_agent.rs, crates/core/src/agents.rs]
---
One `AcpBackend`/`AcpProtocol` (`chat/acp.rs`) serves every CLI with an ACP mode; the row of `acp::CLIS` is the only thing
that differs. What cost time or is easy to get wrong:

- **Flags are checked, not remembered.** Source: the registry `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`
  (each agent's `distribution` has the command and `args`) and the CLI's docs, 2026-11-02: `opencode acp`, `goose acp`,
  `gemini --acp` (`--experimental-acp` is the old name), `copilot --acp` (`--stdio` is optional, GitHub's docs show both),
  `qwen --acp`. Amazon Q, Aider and Interpreter are not in the registry: terminal only.
- **Copilot is two CLIs.** `gh copilot` (the old extension Holt used to detect) has no ACP mode; the standalone `copilot`
  (`npm install -g @github/copilot`) has. `detect_copilot` prefers the standalone binary and the chat view refuses the gh route
  with a clear message. The detection is cached 30 s (`COPILOT_CACHE_TTL`), so a test that installs a fake `copilot`
  after the core started does not see it.
- **The samples in `tests/data/acp-*.jsonl` are written from the schema and the registry's agents, not recorded.** No ACP CLI was
  installed when the backend was made (the sandbox refused `npm install -g opencode-ai`). Replace them with a real
  recording of `opencode acp` as soon as one exists, and fix `acp.rs` where the real shape differs.
- **`session/load` streams the whole history as `session/update` before it answers.** Those updates (`user_message_chunk`
  included) are built apart (`Replay`) and handed over as one `ChatEvent::Replay` when the answer comes; if they went live the
  view would show the old turns as new ones. `user_message_chunk` outside a load is ignored.
- **A cancel is `session/cancel` (a notification) plus an answer `{outcome: cancelled}` to every permission request still open**;
  the agent then ends the prompt with `stopReason: "cancelled"` (that is the end of the turn, there is no other event). Calls the
  turn left open are closed with an error result, or their cards spin forever.
- **`tool_call_update` carries only what changed and `content` replaces the list.** An edit's `diff` usually arrives after the
  `tool_call`: `Transcript::apply_block` updates the input of a call without a result when the same id comes again.
- **Holt answers a permission with the option's `optionId`.** "Allow" = `allow_once`, "Allow for this session" = `allow_always`
  (the card only offers it when the agent listed that option; its id travels as the card's suggestion), "Deny" = `reject_once`,
  else `reject_always`, else `cancelled`. Never invent an id.
- **Nothing is advertised that is not implemented:** `fs` and `terminal` capabilities are false and a request for them gets
  -32601, otherwise the turn would wait forever.
- **Session ids belong to the agent** (`session/new` answers with one; the app saves it from `ChatEvent::Init`). `resume::learns_id_later`
  is true for every ACP CLI except OpenCode, whose TUI takes an id Holt chooses (`--session`). A terminal launch of an ACP-only CLI
  carries no id, so it is not a "resume" (`SpawnSpec::resumed` needs the id on the command line in the terminal view).
- **No login flow.** `authMethods` are noted only to hint "sign in in the Terminal view" when `session/new` fails; Gemini and the
  others reuse the credentials the terminal login saved.
- **Model and effort.** ACP has no launch option for them; the agent reports its model in `session/new` (`models`, shown as text).
  Only OpenCode keeps a model picker (its configuration).
- **Tests.** The fake agent is the test `[[bin]]` `fake_acp_agent` (core integration tests, a real dialogue) and a shell script
  in `chat_flow_tests.rs` (the app crate cannot use another crate's `CARGO_BIN_EXE_*`). The script answers by the id of Holt's
  request (1 initialize, 2 session, 3.. prompts) and logs everything it receives.
