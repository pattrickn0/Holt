# Pending items

The **next steps**, at most 10, in the order they will be done (reviewed with the user on 2026-10-24). When completing
an item, **remove it from here** and add a line of what was done to [history.md](history.md) (document rules in
[OVERVIEW.md](OVERVIEW.md)). This file does not keep completed items.

What is not among the next steps stays in [ideas.md](ideas.md): undated ideas and known limitations. An idea
only enters here when another item leaves (the list never exceeds 10). **Manual tests** stay in [manual-tests.md](manual-tests.md); a
test that fails becomes an item here. Product direction and decision-makers are in [HOLT.md](HOLT.md).

## Now

1. [ ] **Validation in use (user).** The Windows package of `v0.3.0` (delayed: the user is on openSUSE for now; a friend
   validated the Linux package on Big Linux), the terminal without blank lines after resizing (`t37`; if it returns,
   "Record the terminal (diagnostics)" in the tab menu and send `data/pty/`), the document editor (`t39`), quota refusal
   (`ask-agent --models antigravity`, `--team`), and choice by model capability (`t52`: task level, "capability" block in
   `--models`; see manual-tests.md). When the metric has enough tasks, review the `capability.json` table and default
   models per agent (`defaults`). Optionally rename the local folder to `~/Holt` (then rebuild so the menu entry points at
   the new path); steps in [MAINTENANCE.md](MAINTENANCE.md), section 6.
2. [ ] **Task phase and "idle for N min"** (T5). Claude terminal tabs: `PostToolUse` hook (the last tool used becomes
   "editing", "testing"…); chat tabs: the tool calls of the event stream, no hook needed; other CLIs show working/idle and
   for how long.
3. [ ] **Measured memory, then the graph** (T3, then T7), after item 2. Automatic measurement over completed tasks: ground
   truth is learnings from files the task actually changed; prediction is what each method would deliver reading only the
   brief (today; today + one-line index of all; graph); score = hits and delivered size, with learnings as they were at the
   task's base. The graph begins with co-changes from `git log` (no merges, no large commits, no `memory/**`), which also
   suggests task scopes; only adopted for memory if it outperforms the index.
4. [ ] **Linux release package in the app menu.** The `tar.gz` runs but adds no menu entry or icon (only a source build
   does, `crates/app/build.rs`): Holt creates `~/.local/share/applications/holt.desktop` and the icon on first run from
   the package, pointing at its own binary (or ship an AppImage).
5. [ ] **Worktree folder not removed upon validation: monitor.** No new case seen (user, 2026-11-01). Logging exists
   (`file_lock.rs`): on every `remove_task_worktree` failure, `holt.log` and the task warning report which files remained
   and which process holds them open, or if only empty directories remained. On the next occurrence, read the notice
   (search for "worktree" in the log) and fix the cause.

## Later (on the list, not yet)

6. [ ] **Installer for Windows** (0.4).
