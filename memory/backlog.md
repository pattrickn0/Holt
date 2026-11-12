# Pending items

The **next steps**, at most 10, in the order they will be done (reviewed with the user on 2026-10-24). When completing
an item, **remove it from here** and add a line of what was done to [history.md](history.md) (document rules in
[OVERVIEW.md](OVERVIEW.md)). This file does not keep completed items.

What is not among the next steps stays in [ideas.md](ideas.md): undated ideas and known limitations. An idea
only enters here when another item leaves (the list never exceeds 10). Validation is by use (the user runs the app and
reviews what is critical); a defect found that way becomes an item here. Product direction and decision-makers are in
[HOLT.md](HOLT.md).

## Now

1. [ ] **Validation in use (user).** The terminal without blank lines after resizing (`t37`; if it returns,
   "Record the terminal (diagnostics)" in the tab menu and send `data/pty/`), the document editor (`t39`), quota refusal
   (`ask-agent --models antigravity`, `--team`), and choice by model capability (`t52`: task level, "capability" block in
   `--models`). When the metric has enough tasks, review the `capability.json` table and default models per agent
   (`defaults`). Optionally rename the local folder to `~/Holt` (then rebuild so the menu entry points at the new path);
   steps in [MAINTENANCE.md](MAINTENANCE.md), section 6.
2. [ ] **Measured memory, then the graph** (T3, then T7). Automatic measurement over completed tasks: ground
   truth is learnings from files the task actually changed; prediction is what each method would deliver reading only the
   brief (today; today + one-line index of all; graph); score = hits and delivered size, with learnings as they were at the
   task's base. The graph begins with co-changes from `git log` (no merges, no large commits, no `memory/**`), which also
   suggests task scopes; only adopted for memory if it outperforms the index.

## Later (on the list, not yet)

3. [ ] **Installer for Windows** (0.4).
