---
title: --effort is only valid for agents that support effort in their tab
summary: `--task new --effort` is accepted for Claude, Codex, and Antigravity models with effort suffixes; OpenCode (tab mode) and other CLIs reject it before creating the task.
touches: [crates/core/src/models.rs, crates/core/src/tasks/**]
---
Claude accepts `--effort`, Codex accepts `-c model_reasoning_effort=…`, and Antigravity swaps the model name suffix (`-low`, `-medium`, `-high`; `agy_base` also groups `-xhigh`). OpenCode only supports effort in headless mode (not in its interactive tab), and other CLIs do not support effort. With an unsupported agent/model, `ask-agent --task new --effort` fails before creating the task.

**Why:** on 2026-10-24, creating tasks with `--effort` on unsupported targets failed and had to be redone.
**How to apply:** pass `--effort` only for Claude, Codex, or Antigravity models that have `-low`/`-medium`/`-high` variants; omit `--effort` for OpenCode and other CLIs.
