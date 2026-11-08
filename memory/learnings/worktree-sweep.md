---
title: A finished task's worktree may outlive it; the sweep is the safety net
summary: Validation and discard remove the worktree once; if that fails (file held open, permissions, crash), the folder is swept when the project opens.
touches: [crates/core/src/tasks/finish.rs, crates/app/src/tasks_ui.rs]
---
`remove_task_worktree` already retries (`REMOVE_TRIES`), falls back to `remove_dir_all`, and reports the leftovers
through `file_lock::leftovers` (which files remained and who holds them). Even so, a folder can survive: a process
started in the worktree, a permission problem, or the app dying between the status change and the removal. Nothing
would ever try again, so the folder stayed in `.holt/worktrees/` forever (reported by the user on `t14`).

**Why:** `Merged`/`Discarded` are final states and cleanup only happened in the transition; there was no second path.

**How to apply:** `Core::sweep_task_worktrees(project)` removes the worktree of tasks already `Merged` or
`Discarded` whose folder still exists (only inside `.holt/worktrees`, and it prunes git's records). The Tasks desk
calls it in the background when a project opens (`Deck::tasks_reset_for_project`), so it costs nothing when there is
nothing to remove. Never sweep folders that no task references: another Holt data folder can share the same project.
