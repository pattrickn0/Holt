//! One task of each status, for the screenshots of `HOLT_DEBUG_UI=tasks-*`.

use super::*;

// ---------------------------------------------------------------------------
// Fake states (screenshots, `HOLT_DEBUG_UI=tasks-*`)
// ---------------------------------------------------------------------------

/// One task of each status, with realistic data.
pub fn demo_tasks() -> Vec<Task> {
    let now = holt_core::util::now_ms();
    let min = 60_000u64;
    let ev = |ago_min: u64, text: &str| TaskEvent { at_ms: now - ago_min * min, text: text.to_owned() };
    let msg = |ago_min: u64, kind: FeedKind, from: &str, to: &str, text: &str| TaskMessage {
        at_ms: now - ago_min * min,
        kind,
        from: Some(from.to_owned()),
        to: Some(to.to_owned()),
        text: text.to_owned(),
    };
    let base = |id: &str, title: &str, status: TaskStatus, kind: &str, model: Option<&str>| Task {
        id: id.to_owned(),
        title: title.to_owned(),
        status,
        base: "dev".into(),
        branch: format!("task/{id}-{}", holt_core::tasks::slugify(title, 40)),
        worktree: format!(r"C:\Users\you\holt-dev\.holt\worktrees\{id}").into(),
        kind: kind.to_owned(),
        model: model.map(str::to_owned),
        instance: format!("{kind}@{id}"),
        ..Task::default()
    };

    let review = Task {
        brief: "Show a **Tasks** tab in the right bar with the list grouped by status.\n\n- card with the title, the agent and the time\n- detail with the report and the diff\n- merge, return and discard buttons".into(),
        created_ms: now - 95 * min,
        started_ms: Some(now - 90 * min),
        finished_ms: Some(now - 6 * min),
        report: Some("I built the whole Tasks tab.\n\nFiles: crates/app/src/ui/tasks.rs (painting), crates/app/src/tasks_ui.rs (state and actions), right_panel.rs (icon and badge).\n\nTests: cargo test --workspace passes; the pure functions (groups, diff summary, actions per status) have tests.\n\nMissing: check the tab on Windows with several tasks at the same time.".into()),
        diff_stat: Some(" crates/app/src/tasks_ui.rs       | 412 +++++++++++++++++++++++++++\n crates/app/src/ui/tasks.rs      | 688 ++++++++++++++++++++++++++++++++++++\n crates/app/src/ui/right_panel.rs |  14 +-\n crates/core/src/tasks.rs        |  23 ++\n 4 files changed, 1129 insertions(+), 8 deletions(-)\n".into()),
        events: vec![
            ev(95, "created"),
            ev(90, "started: worktree C:\\Users\\you\\holt-dev\\.holt\\worktrees\\t4 on branch task/t4-tasks-tab-in-the-right-bar"),
            ev(90, "brief delivered to the agent"),
            ev(6, "report received; in review"),
        ],
        effort: Some(Effort::High),
        ..base("t4", "Tasks tab in the right bar", TaskStatus::Review, "claude", Some("claude-opus-5-5"))
    };
    let review = Task {
        messages: vec![
            msg(92, FeedKind::Ask, "claude@t4", "antigravity", "How does egui clip the text of a galley that was already measured? I need an ellipsis at the end without measuring again on every frame."),
            msg(91, FeedKind::Answer, "antigravity", "claude@t4", "Use `galley_trunc` from ui/mod.rs: it already cuts with an ellipsis and returns the final galley.\n\nIf the text changes on every frame, cache the galley by the text's fingerprint.\n\nThe big cost is measuring; painting is cheap. For long lists, only measure the visible rows (ScrollArea::show_rows) and keep a fixed height per row.\n\nExample:\n  let g = galley_trunc(painter, text, font, color, max_w);\n  place(painter, rect, Align2::LEFT_CENTER, g, color);"),
            msg(40, FeedKind::Ask, "claude@t4", "opencode", "Can you review the app crate diff?"),
            msg(39, FeedKind::Error, "opencode", "claude@t4", "No quota: the deepseek-v4.1-flash-free model used up the week."),
        ],
        ..review
    };
    let review2 = Task {
        created_ms: now - 50 * min,
        started_ms: Some(now - 48 * min),
        finished_ms: Some(now - 2 * 60 * min),
        report: Some("Fixed the handle leak in the PTY.".into()),
        diff_stat: Some(" crates/app/src/terminal/mod.rs | 6 +++---\n 1 file changed, 3 insertions(+), 3 deletions(-)\n".into()),
        events: vec![ev(150, "report received; in review"), ev(120, "merge refused: conflict in crates/app/src/terminal/mod.rs")],
        effort: Some(Effort::High),
        merge_stopped: Some(holt_core::tasks::MergeStop {
            reason: "conflict with dev in crates/app/src/terminal/mod.rs".into(),
            conflict_files: Some(vec!["crates/app/src/terminal/mod.rs".into()]),
        }),
        ..base("t5", "Fix handle leak when closing the tab", TaskStatus::Review, "antigravity", Some("gemini-4-argon-high"))
    };
    let running = Task {
        created_ms: now - 25 * min,
        started_ms: Some(now - 24 * min),
        events: vec![ev(25, "created"), ev(24, "started: worktree … on branch task/t6")],
        messages: vec![msg(
            2,
            FeedKind::Ask,
            "claude@t6",
            "antigravity",
            "Which status codes does the bridge return for a task action from another agent?",
        )],
        effort: Some(Effort::Medium),
        scope: vec!["crates/core".into(), "memory/ARCHITECTURE.md".into()],
        ..base("t6", "Document the bridge /tasks routes", TaskStatus::Running, "claude", Some("claude-sonnet-5-5"))
    };
    let queued = Task {
        brief: "Replace the git executor with one that reuses the process.".into(),
        created_ms: now - 12 * min,
        queued_ms: Some(now - 12 * min),
        waiting_for: Some("t6".into()),
        scope: vec!["crates/core/src/git.rs".into()],
        events: vec![ev(12, "created"), ev(12, "queued: the scope overlaps t6's; starts when it finishes")],
        ..base("t9", "Reuse the git process", TaskStatus::Queued, "claude", Some("claude-sonnet-5-5"))
    };
    let draft = Task {
        brief: "Run clippy with -D warnings and fix whatever shows up.".into(),
        created_ms: now - 3 * 60 * min,
        events: vec![ev(180, "created")],
        ..base("t7", "Clean up the clippy warnings in the app crate", TaskStatus::Draft, "opencode", None)
    };
    let failed = Task {
        created_ms: now - 5 * 60 * min,
        error: Some("could not create the worktree: branch task/t3-new-branch already exists in this repository".into()),
        events: vec![
            ev(300, "created"),
            ev(299, "error: could not create the worktree: branch task/t3-new-branch already exists in this repository"),
        ],
        ..base("t3", "Swap the report's markdown parser", TaskStatus::Failed, "opencode", Some("opencode/deepseek-v4.1-flash-free"))
    };
    let validating = Task {
        brief: "Show the merge status in the detail header.".into(),
        created_ms: now - 3 * 60 * min,
        started_ms: Some(now - 170 * min),
        finished_ms: Some(now - 8 * min),
        report: Some(
            "The detail header shows the merge.\n\nFiles: crates/app/src/ui/tasks.rs.\n\nTests: cargo test --workspace passes.".into(),
        ),
        diff_stat: Some(" crates/app/src/ui/tasks.rs | 18 ++++++++++++++----\n 1 file changed, 14 insertions(+), 4 deletions(-)\n".into()),
        merge_commit: Some("a1b2c3d4e5f60123456789abcdef0123456789ab".into()),
        events: vec![
            ev(180, "created"),
            ev(170, "started: worktree … on branch task/t8"),
            ev(20, "report received; in review"),
            ev(8, "merged into dev (a1b2c3d); awaiting validation"),
        ],
        ..base("t8", "Merge status in the detail header", TaskStatus::Validating, "claude", Some("claude-sonnet-5-5"))
    };
    let merged = Task {
        created_ms: now - 26 * 60 * min,
        started_ms: Some(now - 26 * 60 * min),
        finished_ms: Some(now - 24 * 60 * min),
        events: vec![ev(24 * 60, "validated; completed; worktree and branch removed")],
        ..base("t2", "/tasks routes on the bridge", TaskStatus::Merged, "claude", Some("claude-opus-5-5"))
    };
    let discarded = Task {
        created_ms: now - 50 * 60 * min,
        finished_ms: Some(now - 48 * 60 * min),
        events: vec![ev(48 * 60, "discarded; worktree and branch removed")],
        ..base("t1", "Galley cache experiment", TaskStatus::Discarded, "claude", None)
    };
    let conflict = Task {
        brief: "Long messages between agents are saved to a file and only a notice is typed.".into(),
        created_ms: now - 150 * min,
        started_ms: Some(now - 148 * min),
        finished_ms: Some(now - 20 * min),
        report: Some("Mail is saved under data/mail and the tab gets a one-line notice.".into()),
        diff_stat: Some(" crates/core/src/mail.rs   | 210 ++++++++++++++++++++\n crates/core/src/bridge.rs |  34 +++---\n 2 files changed, 230 insertions(+), 14 deletions(-)\n".into()),
        merge_approved: true,
        conflict_asks: 1,
        events: vec![
            ev(150, "created"),
            ev(148, "started: worktree … on branch task/t10"),
            ev(20, "report received; in review"),
            ev(3, "merge conflict with dev in crates/core/src/mail.rs; the agent was asked to resolve it (1 of 3)"),
        ],
        messages: vec![
            msg(60, FeedKind::Ask, "claude@t10", "antigravity", "Is a rename atomic on Windows if the target exists?"),
            msg(58, FeedKind::Answer, "antigravity", "claude@t10", "Use the inbox file per instance; renames are atomic on NTFS when both paths are on the same volume."),
        ],
        ..base("t10", "Durable mail between agents", TaskStatus::Running, "claude", Some("claude-opus-5-5"))
    };
    vec![discarded, merged, failed, review, running, queued, draft, review2, validating, conflict]
}

#[cfg(test)]
mod tests {
    use super::*;
    use holt_core::i18n::{with_lang, Lang};

    fn task(id: &str, status: TaskStatus, activity: u64) -> Task {
        Task { id: id.into(), status, created_ms: 1, events: vec![TaskEvent { at_ms: activity, text: "x".into() }], ..Task::default() }
    }

    #[test]
    fn the_diff_stat_becomes_a_one_line_summary() {
        let stat = " a.rs | 10 +++++++---\n b.rs | 2 +-\n 3 files changed, 120 insertions(+), 8 deletions(-)\n";
        let s = parse_diff_stat(stat).expect("summary");
        assert_eq!(s, DiffSummary { files: 3, insertions: 120, deletions: 8 });
        // Singular and only one of the sides.
        let one = DiffSummary { files: 1, insertions: 1, deletions: 0 };
        assert_eq!(parse_diff_stat(" a.rs | 1 +\n 1 file changed, 1 insertion(+)\n"), Some(one));
        assert_eq!(parse_diff_stat(" 2 files changed, 4 deletions(-)"), Some(DiffSummary { files: 2, insertions: 0, deletions: 4 }));
        // No differences (the core's text, in both languages) and garbage.
        let none = DiffSummary { files: 0, insertions: 0, deletions: 0 };
        assert_eq!(parse_diff_stat("(no changes against the base)"), Some(none));
        assert_eq!(parse_diff_stat("(nenhuma alteração em relação à base)"), Some(none));
        assert_eq!(parse_diff_stat(""), None);
        assert_eq!(parse_diff_stat("something else"), None);
    }

    #[test]
    fn the_stat_lists_one_line_per_file() {
        let stat = " crates/app/src/tasks_ui.rs | 412 +++++++++\n a b/c.rs => d.rs |  3 +-\n logo.png | Bin 0 -> 1234 bytes\n 3 files changed, 415 insertions(+), 1 deletion(-)\n";
        let lines = parse_stat_lines(stat);
        let shape: Vec<(&str, &str)> = lines.iter().map(|l| (l.path.as_str(), l.changes.as_str())).collect();
        assert_eq!(shape, [("crates/app/src/tasks_ui.rs", "412"), ("a b/c.rs => d.rs", "3"), ("logo.png", "Bin 0 -> 1234 bytes")]);
        // The +/− split comes from the graph; a binary file has none.
        let shares: Vec<Option<f32>> = lines.iter().map(|l| l.plus).collect();
        assert_eq!(shares, [Some(1.0), Some(0.5), None]);
        assert!(parse_stat_lines("(no changes against the base)").is_empty());
        assert!(parse_stat_lines("").is_empty());
    }

    #[test]
    fn ages_read_naturally() {
        let now = 10_000_000_000;
        assert_eq!(age_text(now, now), "now");
        assert_eq!(age_text(now, now + 5_000), "now");
        assert_eq!(age_text(now, now - 59_000), "now");
        assert_eq!(age_text(now, now - 60_000), "1 min ago");
        assert_eq!(age_text(now, now - 59 * 60_000), "59 min ago");
        assert_eq!(age_text(now, now - 3_600_000), "1 h ago");
        assert_eq!(age_text(now, now - 23 * 3_600_000), "23 h ago");
        assert_eq!(age_text(now, now - 3 * 86_400_000), "3 d ago");
    }

    #[test]
    fn the_status_line_says_what_happened_and_when() {
        let now = 1_000_000_000;
        let mut t = task("t1", TaskStatus::Review, now - 1000);
        t.finished_ms = Some(now - 3 * 60_000);
        assert_eq!(status_age(&t, now), "ready 3 min ago");
        t.status = TaskStatus::Running;
        t.started_ms = Some(now - 2 * 3_600_000);
        assert_eq!(status_age(&t, now), "started 2 h ago");
        t.status = TaskStatus::Draft;
        t.created_ms = now - 10_000;
        assert_eq!(status_age(&t, now), "created now");
        t.status = TaskStatus::Failed;
        t.created_ms = now - 3_600_000;
        t.events = vec![TaskEvent { at_ms: now - 120_000, text: "error".into() }];
        assert_eq!(status_age(&t, now), "failed 2 min ago");
        t.status = TaskStatus::Validating;
        t.finished_ms = Some(now - 5 * 60_000);
        assert_eq!(status_age(&t, now), "merged 5 min ago");
        t.status = TaskStatus::Merged;
        assert_eq!(status_age(&t, now), "completed 5 min ago");
        // The same lines in Portuguese.
        with_lang(Lang::PtBr, || {
            assert_eq!(status_age(&t, now), "concluída há 5 min");
            t.status = TaskStatus::Draft;
            assert_eq!(status_age(&t, now), "criada há 1 h");
            assert_eq!(age_text(now, now), "agora");
        });
    }

    #[test]
    fn effort_is_shown_next_to_the_model_unless_the_model_name_has_it() {
        let mut t = Task::default();
        assert_eq!((effort_text(&t), model_effort_text(&t)), (None, None));
        t.model = Some("opencode/glm-5.3".into());
        assert_eq!(model_effort_text(&t).as_deref(), Some("glm-5.3"));
        t.model = Some("sonnet".into());
        t.effort = Some(Effort::High);
        let high = Effort::High.label();
        assert_eq!(model_effort_text(&t), Some(format!("sonnet · {high}")));
        t.model = None;
        assert_eq!(model_effort_text(&t).as_deref(), Some(high));
        // Antigravity: the effort is already in the model name.
        t.model = Some("gemini-4-argon-medium".into());
        t.effort = Some(Effort::Medium);
        assert_eq!((effort_text(&t), model_effort_text(&t).as_deref()), (None, Some("gemini-4-argon-medium")));
    }

    #[test]
    fn models_are_shown_without_the_provider() {
        let mut t = Task::default();
        assert_eq!(model_text(&t), None);
        t.model = Some("opencode/glm-5.3".into());
        assert_eq!(model_text(&t), Some("glm-5.3"));
        t.model = Some("opus".into());
        assert_eq!(model_text(&t), Some("opus"));
    }

    #[test]
    fn a_queued_task_works_in_the_agents_lane_with_only_the_discard_button() {
        let q = Task {
            status: TaskStatus::Queued,
            waiting_for: Some("t2".into()),
            queued_ms: Some(5_000),
            created_ms: 1_000,
            ..Task::default()
        };
        assert_eq!(actions_for(&q, false), [TaskAct::Discard]);
        assert_eq!((Lane::of(TaskStatus::Queued), mark_of(&q), state_word(&q)), (Lane::Working, Mark::Queued, "Queued"));
        assert_eq!(status_name(TaskStatus::Queued), "Queued");
        // How long it has been in the queue counts from entering it, not from the creation.
        assert_eq!(status_age(&q, 65_000), "queued 1 min ago");
        assert_eq!(waiting_since(&q), 5_000);
    }
    #[test]
    fn the_scope_field_takes_one_path_per_line_or_commas() {
        assert!(parse_scope("").is_empty() && parse_scope(" \n , \n").is_empty());
        assert_eq!(
            parse_scope(" crates/core \n\nmemory/OVERVIEW.md, crates/cli\r\nCargo.lock"),
            ["crates/core", "memory/OVERVIEW.md", "crates/cli", "Cargo.lock"]
        );
    }

    #[test]
    fn each_status_has_its_buttons() {
        let t = |status, report: bool| Task { status, base: "dev".into(), report: report.then(|| "done".to_owned()), ..Task::default() };
        assert_eq!(actions_for(&t(TaskStatus::Draft, false), false), [TaskAct::Start, TaskAct::Discard]);
        assert_eq!(actions_for(&t(TaskStatus::Running, false), true), [TaskAct::GoToTab, TaskAct::Discard]);
        assert_eq!(actions_for(&t(TaskStatus::Running, false), false), [TaskAct::ReopenTab, TaskAct::Discard]);
        assert_eq!(actions_for(&t(TaskStatus::Review, true), true), [TaskAct::Merge, TaskAct::Return, TaskAct::GoToTab, TaskAct::Discard]);
        assert_eq!(
            actions_for(&t(TaskStatus::Validating, true), true),
            [TaskAct::Validate, TaskAct::Reject, TaskAct::GoToTab, TaskAct::Discard]
        );
        assert_eq!(
            actions_for(&t(TaskStatus::Validating, true), false),
            [TaskAct::Validate, TaskAct::Reject, TaskAct::ReopenTab, TaskAct::Discard]
        );
        assert_eq!(actions_for(&t(TaskStatus::Failed, false), false), [TaskAct::Retry, TaskAct::Discard]);
        // A merge that failed earlier (Failed with a report): it is also sent back to the agent.
        assert_eq!(actions_for(&t(TaskStatus::Failed, true), true), [TaskAct::Retry, TaskAct::Return, TaskAct::GoToTab, TaskAct::Discard]);
        assert!(actions_for(&t(TaskStatus::Merged, true), false).is_empty());
        assert!(actions_for(&t(TaskStatus::Discarded, false), false).is_empty());
        // Merge shows the base.
        assert_eq!(
            (TaskAct::Validate.label(&t(TaskStatus::Validating, true)), TaskAct::Reject.label(&t(TaskStatus::Validating, true))),
            ("Validate".to_owned(), "Reject".to_owned())
        );
        assert_eq!(TaskAct::Merge.label(&t(TaskStatus::Review, true)), "Merge into dev");
        with_lang(Lang::PtBr, || assert_eq!(TaskAct::Merge.label(&t(TaskStatus::Review, true)), "Mesclar em dev"));
    }

    #[test]
    fn destructive_actions_ask_first() {
        let review = Task { status: TaskStatus::Review, report: Some("x".into()), ..Task::default() };
        assert!(TaskAct::Discard.needs_confirm(&review) && TaskAct::Merge.needs_confirm(&review));
        assert!(!TaskAct::Return.needs_confirm(&review) && !TaskAct::Start.needs_confirm(&review));
        assert!(!TaskAct::Validate.needs_confirm(&review) && !TaskAct::Reject.needs_confirm(&review));
        assert!(!TaskAct::GoToTab.needs_confirm(&review) && !TaskAct::ReopenTab.needs_confirm(&review));
        // Try again: starting does not ask; merging (it already has a report) does.
        let failed_start = Task { status: TaskStatus::Failed, ..Task::default() };
        let failed_merge = Task { status: TaskStatus::Failed, report: Some("x".into()), ..Task::default() };
        assert!(!TaskAct::Retry.needs_confirm(&failed_start));
        assert!(TaskAct::Retry.needs_confirm(&failed_merge));
    }

    #[test]
    fn the_diff_is_available_while_the_branch_exists_or_after_the_merge_commit_is_kept() {
        let t = |status, started: bool| Task { status, started_ms: started.then_some(1), ..Task::default() };
        let merged = |status| Task { merge_commit: Some("0123456789abcdef0123456789abcdef01234567".into()), ..t(status, true) };
        assert!(can_show_diff(&t(TaskStatus::Review, true)));
        assert!(can_show_diff(&t(TaskStatus::Running, true)));
        assert!(can_show_diff(&t(TaskStatus::Failed, true)));
        assert!(!can_show_diff(&t(TaskStatus::Failed, false)));
        assert!(!can_show_diff(&t(TaskStatus::Draft, false)));
        assert!(!can_show_diff(&t(TaskStatus::Discarded, true)));
        // Without the merge commit (an old task), awaiting validation or completed has no diff: the branch is the base itself.
        assert!(!can_show_diff(&t(TaskStatus::Validating, true)));
        assert!(!can_show_diff(&t(TaskStatus::Merged, true)));
        // With the commit, both statuses show what was merged.
        assert!(can_show_diff(&merged(TaskStatus::Validating)) && can_show_diff(&merged(TaskStatus::Merged)));
        // Discarded still has no diff, even if it was merged before.
        assert!(!can_show_diff(&merged(TaskStatus::Discarded)));
    }

    #[test]
    fn the_diff_button_says_what_it_shows_in_each_state() {
        let hash = "0123456789abcdef0123456789abcdef01234567";
        let t = |status, commit: Option<&str>| Task {
            status,
            started_ms: Some(1),
            id: "t5".into(),
            base: "dev".into(),
            branch: "task/t5-x".into(),
            merge_commit: commit.map(str::to_owned),
            ..Task::default()
        };
        for status in [TaskStatus::Running, TaskStatus::Review, TaskStatus::Failed] {
            assert_eq!(diff_button_label(&t(status, None)), "View full diff");
            assert_eq!(diff_title(&t(status, None)), "t5 · changes against dev");
        }
        // Rejected and back in review, the old commit is still stored: the diff is the branch's.
        assert_eq!(diff_button_label(&t(TaskStatus::Review, Some(hash))), "View full diff");
        for status in [TaskStatus::Validating, TaskStatus::Merged] {
            assert_eq!(diff_button_label(&t(status, Some(hash))), "View what was merged");
            assert_eq!(diff_title(&t(status, Some(hash))), "t5 · what the merge brought into dev");
        }
        // The detail shows the short hash next to `branch → base`.
        assert_eq!(branch_line(&t(TaskStatus::Validating, Some(hash))), "task/t5-x → dev · 0123456");
        assert_eq!(branch_line(&t(TaskStatus::Running, None)), "task/t5-x → dev");
        assert_eq!(merge_short(&t(TaskStatus::Merged, Some(""))), None);
    }

    #[test]
    fn upsert_replaces_by_id() {
        let mut list = vec![task("t1", TaskStatus::Draft, 1)];
        upsert(&mut list, task("t1", TaskStatus::Running, 2));
        upsert(&mut list, task("t2", TaskStatus::Draft, 3));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].status, TaskStatus::Running);
    }

    #[test]
    fn long_texts_are_clipped_for_the_screen() {
        assert_eq!(clip_text("short", 10), "short");
        assert_eq!(clip_text("exact", 5), "exact");
        let clipped = clip_text("an action too long", 4);
        assert!(clipped.starts_with("an a\n\n[… text cut on screen"), "{clipped}");
        assert!(clipped.contains("4 of 18"), "{clipped}");
        // The local time has the form dd/mm hh:mm (the time zone depends on the machine).
        let t = event_time(1_760_000_000_000);
        assert_eq!((t.len(), t.as_bytes()[2], t.as_bytes()[8]), (11, b'/', b':'), "{t}");
    }

    #[test]
    fn the_demo_has_one_task_per_status() {
        let tasks = demo_tasks();
        for status in [
            TaskStatus::Draft,
            TaskStatus::Queued,
            TaskStatus::Running,
            TaskStatus::Review,
            TaskStatus::Validating,
            TaskStatus::Merged,
            TaskStatus::Discarded,
            TaskStatus::Failed,
        ] {
            assert!(tasks.iter().any(|t| t.status == status), "{status:?}");
        }
    }

    #[test]
    fn the_demo_falls_into_lanes_by_who_moves_next() {
        let tasks = demo_tasks();
        let shape = |filter| -> Vec<(Lane, Vec<String>)> {
            lanes(&tasks, filter).into_iter().map(|(l, list)| (l, list.iter().map(|t| t.id.clone()).collect())).collect()
        };
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // Needs you: the review first, then the test, the merge that stopped and the failure. Working: the conflict
        // being resolved, the plain work, the queue last.
        assert_eq!(
            shape(HistoryFilter::All),
            vec![
                (Lane::NeedsYou, ids(&["t4", "t8", "t5", "t3"])),
                (Lane::Working, ids(&["t10", "t6", "t9"])),
                (Lane::Drafts, ids(&["t7"])),
                (Lane::Finished, ids(&["t2", "t1"])),
            ]
        );
        // The filter only touches "Finished"; an empty filter keeps the lane (with its sentence).
        assert_eq!(shape(HistoryFilter::Done).last(), Some(&(Lane::Finished, ids(&["t2"]))));
        assert_eq!(shape(HistoryFilter::Discarded).last(), Some(&(Lane::Finished, ids(&["t1"]))));
        let open: Vec<Task> = tasks.iter().filter(|t| Lane::of(t.status) != Lane::Finished).cloned().collect();
        assert!(lanes(&open, HistoryFilter::All).iter().all(|(l, _)| *l != Lane::Finished));
        assert!(lanes(&[], HistoryFilter::All).is_empty());

        // The marks and words of each state.
        let by_id = |id: &str| tasks.iter().find(|t| t.id == id).expect("task");
        let marks: Vec<(Mark, &str)> = ["t4", "t8", "t5", "t3", "t10", "t6", "t9", "t7", "t2", "t1"]
            .iter()
            .map(|id| (mark_of(by_id(id)), state_word(by_id(id))))
            .collect();
        assert_eq!(
            marks,
            [
                (Mark::Review, "Review"),
                (Mark::Validate, "Test it"),
                (Mark::Stopped, "Not merged"),
                (Mark::Failed, "Failed"),
                (Mark::Conflict, "Resolving a conflict"),
                (Mark::Running, "Working"),
                (Mark::Queued, "Queued"),
                (Mark::Draft, "Draft"),
                (Mark::Done, "Completed"),
                (Mark::Discarded, "Discarded"),
            ]
        );
        with_lang(Lang::PtBr, || {
            assert_eq!(state_word(by_id("t8")), "Teste");
            assert_eq!(status_name(TaskStatus::Merged), "Concluída");
        });

        // The dd/mm date has 5 characters with a '/' in the middle.
        let ddmm = task_date_ddmm(&tasks[0]);
        assert_eq!((ddmm.len(), ddmm.as_bytes()[2]), (5, b'/'), "{ddmm}");
    }

    #[test]
    fn the_track_shows_where_each_task_is() {
        let tasks = demo_tasks();
        let now = holt_core::util::now_ms();
        let track = |id: &str| -> Vec<(&'static str, StopState)> {
            lifecycle(tasks.iter().find(|t| t.id == id).expect("task"), now).into_iter().map(|s| (s.label, s.state)).collect()
        };
        use StopState::*;
        assert_eq!(track("t7"), [("Created", Now), ("Worked", Later), ("Review", Later), ("Merged", Later), ("Validated", Later)]);
        assert_eq!(track("t9")[1], ("Queued", Now));
        assert_eq!(track("t6")[1], ("Working", Now));
        assert_eq!(track("t4")[2], ("Review", Now));
        assert_eq!(track("t5")[2], ("Review", Warn));
        assert_eq!(track("t10")[2], ("Conflict", Warn));
        assert_eq!(track("t3")[1], ("Failed", Fail));
        assert_eq!(track("t8")[3], ("Merged", Now));
        assert!(track("t2").iter().all(|(_, s)| *s == Done));
        // A discard takes the stop after the last one reached.
        assert_eq!(track("t1")[1], ("Discarded", Fail));
        // The work's stop carries how long it took.
        let t4 = tasks.iter().find(|t| t.id == "t4").expect("t4");
        assert_eq!(lifecycle(t4, now)[1].when, "1 h 24");
        assert_eq!(
            (duration_text(30_000), duration_text(12 * 60_000), duration_text(125 * 60_000)),
            ("<1 min".into(), "12 min".into(), "2 h 05".into())
        );
    }

    fn message(at_ms: u64, text: &str) -> TaskMessage {
        TaskMessage { at_ms, kind: FeedKind::Ask, from: Some("claude@t1".into()), to: None, text: text.into() }
    }

    #[test]
    fn the_timeline_mixes_events_and_messages_newest_first() {
        let t = Task {
            instance: "claude@t1".into(),
            messages: vec![message(30, "c"), message(10, "a")],
            events: vec![TaskEvent { at_ms: 20, text: "b".into() }, TaskEvent { at_ms: 30, text: "c-event".into() }],
            ..Task::default()
        };
        let texts: Vec<&str> = timeline(&t)
            .iter()
            .map(|m| match m {
                Moment::Event(e) => e.text.as_str(),
                Moment::Message(m) => m.text.as_str(),
            })
            .collect();
        // At the same time, the message goes after the event that caused it (newest first: above it).
        assert_eq!(texts, ["c", "c-event", "b", "a"]);
        assert_eq!(last_move(&t), Some(("asked …".to_owned(), 30)));
        let answer = TaskMessage {
            at_ms: 40,
            kind: FeedKind::Answer,
            from: Some("antigravity".into()),
            to: Some("claude@t1".into()),
            text: "x".into(),
        };
        let t = Task { messages: vec![answer], ..t };
        assert_eq!(last_move(&t), Some(("antigravity answered".to_owned(), 40)));
        assert_eq!(
            (message_kind_label(FeedKind::Ask), message_kind_label(FeedKind::Answer), message_kind_label(FeedKind::Error)),
            ("question", "answer", "error")
        );
        // The pane a task opens on.
        assert_eq!(DetailTab::default_for(&Task::default()), DetailTab::Request);
        assert_eq!(DetailTab::default_for(&Task { started_ms: Some(1), ..Task::default() }), DetailTab::Timeline);
        assert_eq!(DetailTab::default_for(&Task { report: Some("done".into()), ..Task::default() }), DetailTab::Report);
    }
    #[test]
    fn a_long_message_is_collapsed_by_size_or_by_lines() {
        assert_eq!(message_preview("  short \n"), ("short".into(), false));
        let (cut, more) = message_preview(&"word ".repeat(200));
        assert!(more && cut.ends_with('…') && cut.chars().count() <= MESSAGE_PREVIEW_CHARS + 1, "{cut}");
        let (cut, more) = message_preview("1\n2\n3\n4\n5\n6");
        assert_eq!((cut.as_str(), more), ("1\n2\n3\n4…", true));
        assert!(!message_preview("1\n2\n3\n4").1);
    }
}
