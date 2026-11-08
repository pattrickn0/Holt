---
title: A CLI without an autonomous shell tool cannot answer inter-agent questions
summary: CLIs without autonomous shell tools (such as Aider, Ollama REPL, and llama-cli) cannot answer inter-agent questions; the bridge refuses questions to them before opening a tab and marks them in --models/--list/--team.
touches: [crates/core/src/agents/**, crates/core/src/bridge.rs, crates/core/src/models.rs, crates/core/src/tasks/lifecycle.rs, crates/cli/src/bin/ask-agent.rs]
---
Inter-agent conversation in Holt relies on the recipient running `ask-agent --reply <n> "answer"`. An agent CLI
without an autonomous shell tool (such as Aider, whose `/run` command is manual and does not answer prompts) will
never run the command, leaving the asker waiting until the 10-minute timeout.

- **Refusal before opening a tab:** `bridge::ask_tab` checks `agents::can_answer(&want.kind)` before creating or opening
  any tab. If false, it returns HTTP 400 with `agents::cannot_answer_error`, which explains why and suggests installed
  peers that can answer (excluding the caller itself).
- **Tasks desk:** `tasks::lifecycle` refuses task creation (`--agent aider`) because task completion requires running
  `ask-agent --task done`.
- **Marked in listings:** `AgentDef::can_answer` is reflected in `AgentInfo::can_answer`, `format_inventory` (`--models`),
  and `with_model` (`--list` and `--team`), appending `(cannot answer)`.

**Why:** user report (2026-11-07) noted some agents did not adapt to Holt. An audit revealed `ask-agent aider "..."` typed questions into an Aider tab where no `--reply` could ever be executed autonomously.
**How to apply:** any new CLI added to `AGENTS` must have `can_answer: true` only if it has an autonomous shell execution tool. Never let an inter-agent question open a tab for an agent that cannot send `--reply`.
