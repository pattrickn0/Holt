---
title: Each CLI receives Holt instructions through its own channel
summary: Claude, Codex, OpenCode, Aider, and Interpreter via CLI flags/config; Antigravity and Gemini via data/ context folders; Qwen via flag; Amazon Q via chat message; Goose, Copilot, and other CLIs via project AGENTS.md.
touches: [crates/core/src/agents/**, crates/core/src/models.rs, crates/core/src/workspace.rs, crates/core/src/headless.rs, AGENTS.md]
---
Holt instructions (rule against internal subagents, `ask-agent`, shared memory) cannot depend on the
user's `AGENTS.md`: a new project lacks one, and an agent lacking instructions launches its CLI's built-in subagents.

- **Claude:** `--append-system-prompt`. **Codex:** `-c developer_instructions=<TOML>`; in a `.cmd` shim quotes are lost
  (`cmd_safe_arg`), so raw text is passed, which Codex accepts as a string when not valid TOML. Not installed on the
  development machine: unverified in practice.
- **OpenCode:** `{"instructions": ["<file>"]}` in `OPENCODE_CONFIG_CONTENT` (merged with user configuration). Verified: with
  `opencode run --standalone` file rules were respected. Only the tab's dedicated server reads this variable, so the TUI
  now **always** starts with `--standalone`, with or without a model.
- **Aider:** `--read <file>`. **Interpreter:** `--custom_instructions` (unverified). Aider cannot run shell tools on its
  own, so the bridge refuses inter-agent questions to it before opening a tab (see `memory/learnings/cli-without-autonomous-shell-cannot-answer.md`).
- **Antigravity:** no system option available, but `agy` reads `AGENTS.md` in each folder passed via `--add-dir`, **combined** with
  the project's file. Verified on 2026-10-24 (`agy` 1.4.7, Flash): a rule in an external `AGENTS.md` applied only when
  `--add-dir` was present, alongside rules from the project `AGENTS.md`. The app writes `data/agy/<instance>/AGENTS.md` (with name and
  role, similar to Claude) and passes that folder (`write_agy_instructions`): nothing added to the user's repository.
- **Gemini:** `--include-directories` pointing to `data/gemini/<instance>` containing `GEMINI.md` (`write_gemini_instructions`).
  The CLI scans for `GEMINI.md` in all included workspace folders. Unverified on this machine (inspected in 0.67 bundle; pending manual test).
- **Qwen Code:** `--append-system-prompt <text>` (present from 0.15+, inspected in 0.27 bundle). Unverified on this machine (pending manual test).
- **Amazon Q Developer:** passed as the first message of `q chat "<rules>"`. Unverified on this machine (pending manual test).
- **Goose, Copilot, and other CLIs:** only read the project `AGENTS.md`. The app generates one containing solely the marker block when the folder lacks one, notifies in
  Chat, and never modifies an existing file (`Core::ensure_agents_file`). Unverified (not installed locally).
- Generic instructions (file and block) do not list installed agents: otherwise the file would vary across machines and
  dirty `git status`.

**Why:** new user report (23/10): agents were not using `ask-agent` and were spawning CLI subagents. Audit (07/11): Gemini, Qwen, and Amazon Q fell through instruction args and received no Holt rules at all.
**How to apply:** for a new CLI = find its instruction flag (`--help`, documentation, bundle inspection) and add to `instruction_args` or context folder helper;
before falling back to `reads_only_agents_md`, test whether it reads a context file from an added directory (like `agy` and `gemini`: use an easily observable rule like "start answer with PINEAPPLE-7" in a context file outside the project, with and without the option). What quota limits or a defect found in use goes to `memory/backlog.md`.
