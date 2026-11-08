//! The actions of the tab: they call the core on a thread and re-read the list when an answer arrives.

use super::*;

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

impl Deck {
    /// The project on screen changed: forgets the previous one's list, detail and form and ignores the
    /// answers that still come from it. The "Finished" filter stays.
    pub fn tasks_reset_for_project(&mut self) {
        let generation = self.tasks.generation + 1;
        let history_filter = self.tasks.history_filter;
        self.tasks = TasksUi { generation, history_filter, ..TasksUi::default() };
        // A task that ended may have left its worktree behind (the removal failed when it finished): the sweep
        // cleans it now, before the list is read. Cheap when there is nothing to remove.
        let project = self.active_project.clone();
        if !project.is_empty() {
            self.spawn_bg("worktree-sweep", move |core| {
                core.sweep_task_worktrees(&project);
                None
            });
        }
    }

    /// Re-reads the list (one at a time; a request in the middle repeats the read when it finishes).
    pub fn tasks_refresh(&mut self) {
        if self.tasks.debug_hold || self.active_project.is_empty() {
            return;
        }
        if self.tasks.refreshing {
            self.tasks.dirty = true;
            return;
        }
        self.tasks.refreshing = true;
        let (generation, project) = (self.tasks.generation, self.active_project.clone());
        self.spawn_bg("tasks-list", move |core| Some(UiMsg::Tasks(generation, TaskReply::List(core.tasks(&project)))));
    }

    /// The core saved the tasks of `project`.
    pub fn tasks_changed(&mut self, project: &str) {
        if project == self.active_project {
            self.tasks_refresh();
        }
    }

    pub fn tasks_open_detail(&mut self, id: &str) {
        self.tasks.view = TasksView::Detail(id.to_owned());
        self.tasks_clear_detail_state();
        self.tasks_clear_notice();
    }

    /// Takes the notice off the screen, with the refusal reason and the list of files that went with it.
    pub fn tasks_clear_notice(&mut self) {
        self.tasks.notice = None;
        self.tasks.notice_info = false;
        self.tasks.notice_block = None;
        self.tasks.notice_stash = None;
        self.tasks.notice_files = false;
    }

    /// Shows a plain error (red strip, no way-out button).
    fn tasks_set_error(&mut self, msg: String) {
        self.tasks_clear_notice();
        self.tasks.notice = Some(msg);
    }

    fn tasks_clear_detail_state(&mut self) {
        self.tasks.detail_tab = None;
        self.tasks.confirm = None;
        self.tasks.return_open = false;
        self.tasks.feedback.clear();
    }

    /// "←": from the detail and from the form to the list.
    pub fn tasks_back(&mut self) {
        self.tasks.view = TasksView::List;
        self.tasks_clear_detail_state();
        self.tasks_clear_notice();
        self.tasks.history_menu = false;
        self.tasks.form.menu = None;
        self.tasks.form.provider = None;
    }

    /// Agents a task can use: installed, with a tab (no plain terminal).
    pub fn tasks_agent_choices(&self) -> Vec<String> {
        self.agents.available.iter().filter(|a| a.installed && a.peer && a.id != "shell").map(|a| a.id.clone()).collect()
    }

    /// Opens the form: default agent `claude` (or the first available) and the current branch as the base.
    pub fn tasks_open_new(&mut self) {
        let choices = self.tasks_agent_choices();
        let form = &mut self.tasks.form;
        form.menu = None;
        form.provider = None;
        form.error = None;
        if form.kind_is_auto || form.kind.is_empty() || (!choices.is_empty() && !choices.contains(&form.kind)) {
            form.kind = choices.first().cloned().unwrap_or_else(|| "claude".into());
            form.kind_is_auto = true;
            form.model = None;
            form.effort = None;
            form.level = Level::default();
        }
        if form.base.trim().is_empty() {
            form.base = self.git.status().and_then(|st| st.branch.clone()).unwrap_or_default();
        }
        self.tasks_form_refresh();
        let kind = self.tasks.form.kind.clone();
        self.request_models(&kind);
        self.tasks.form.focus_title = true;
        self.tasks.view = TasksView::New;
        self.tasks_clear_notice();
    }

    /// Recalculates what depends on the form's level, agent, model and effort: the recommended agent
    /// (while the agent is the automatic one: the one capable of the level with the most quota to spare) and the capacity notice.
    pub fn tasks_form_refresh(&mut self) {
        let choices = self.tasks_agent_choices();
        let (level, effort) = (self.tasks.form.level, self.tasks.form.effort);
        if self.tasks.form.kind_is_auto {
            let recommended = capability::select(&self.core.capability_candidates(&choices, effort), level).map(|s| s.kind);
            if let Some(kind) = recommended.filter(|k| *k != self.tasks.form.kind) {
                self.tasks.form.kind = kind.clone();
                self.tasks.form.model = None;
                self.request_models(&kind);
            }
        }
        let f = &self.tasks.form;
        let effort = f.effort.filter(|_| models::effort_unavailable(&f.kind, f.model.as_deref()).is_none());
        let note = self.core.capability_advisory(&f.kind, f.model.as_deref(), f.level, effort);
        self.tasks.form.capacity_note = note;
    }

    /// Chose another size of work in the form.
    pub fn tasks_form_set_level(&mut self, level: Level) {
        self.tasks.form.level = level;
        self.tasks_form_refresh();
    }

    /// Chose another agent in the form: the model and the effort go back to the default.
    pub fn tasks_form_set_kind(&mut self, kind: String) {
        if self.tasks.form.kind != kind {
            self.tasks.form.model = None;
            self.tasks.form.effort = None;
            self.tasks.form.kind = kind.clone();
        }
        self.tasks.form.kind_is_auto = false;
        self.tasks.form.menu = None;
        self.tasks.form.provider = None;
        self.request_models(&kind);
        self.tasks_form_refresh();
    }

    /// "Attach files" button of the "New task" form.
    pub fn tasks_pick_attachments(&self) {
        self.pick_attachments_for(AttachTarget::TaskForm(self.active_project.clone()));
    }

    /// "×" of the attachment chip in the "New task" form: removes it from the list and deletes the copy in `data/attachments`.
    pub fn tasks_remove_attachment(&mut self, i: usize) {
        if i >= self.tasks.form.attachments.len() {
            return;
        }
        let path = self.tasks.form.attachments.remove(i);
        self.delete_attachment_copy(path);
    }

    /// Creates the task (and starts it, with "Start now") in a thread.
    pub fn tasks_submit_form(&mut self) {
        let f = &mut self.tasks.form;
        if f.busy || f.title.trim().is_empty() || self.active_project.is_empty() {
            return;
        }
        f.busy = true;
        f.error = None;
        f.menu = None;
        f.provider = None;
        let spec = TaskSpec {
            title: f.title.clone(),
            brief: crate::util::append_brief_attachments(&f.brief, &f.attachments),
            kind: f.kind.clone(),
            level: f.level,
            model: f.model.clone(),
            // The effort chosen before switching model may no longer apply (Antigravity without per-effort versions).
            effort: f.effort.filter(|_| models::effort_unavailable(&f.kind, f.model.as_deref()).is_none()),
            base: Some(f.base.trim().to_owned()).filter(|b| !b.is_empty()),
            creator: None,
            scope: parse_scope(&f.scope),
        };
        let start_now = f.start_now;
        let (generation, project) = (self.tasks.generation, self.active_project.clone());
        self.spawn_bg("task-create", move |core| {
            let result = Box::new(core.task_create(&project, spec));
            let started = match (&*result, start_now) {
                (Ok(task), true) => Some(Box::new(core.task_start(&project, &task.id))),
                _ => None,
            };
            Some(UiMsg::Tasks(generation, TaskReply::Created { result, started }))
        });
    }

    /// Runs a lifecycle operation in a thread.
    pub fn tasks_run(&mut self, id: String, op: TaskOp) {
        if self.tasks.busy.is_some() || self.active_project.is_empty() {
            return;
        }
        self.tasks.busy = Some(Busy { id: id.clone(), label: op.busy_label() });
        self.tasks_clear_notice();
        self.tasks.confirm = None;
        let (generation, project) = (self.tasks.generation, self.active_project.clone());
        self.spawn_bg("task-op", move |core| {
            let mut merge = Box::new(MergeExtra::default());
            let result = Box::new(match &op {
                TaskOp::Start => core.task_start(&project, &id),
                TaskOp::Merge | TaskOp::MergeStash => match core.task_merge_assisted(&project, &id, matches!(op, TaskOp::MergeStash)) {
                    Ok(done) => {
                        merge.note = done.note;
                        Ok(done.task)
                    }
                    Err(refusal) => {
                        merge.block = refusal.block;
                        merge.note = refusal.stash_note;
                        Err(refusal.error)
                    }
                },
                TaskOp::Validate => core.task_validate(&project, &id),
                TaskOp::Discard => core.task_discard(&project, &id),
                TaskOp::Return(feedback) => core.task_return(&project, &id, feedback),
                TaskOp::Reject(feedback) => core.task_reject(&project, &id, feedback),
                TaskOp::CancelAutoMerge => core.task_cancel_auto_merge(&project, &id),
            });
            Some(UiMsg::Tasks(generation, TaskReply::Done { id, op, result, merge }))
        });
    }

    /// Click on a button of the detail.
    pub fn tasks_act(&mut self, id: &str, act: TaskAct) {
        let Some(task) = self.tasks.task(id).cloned() else { return };
        if act.needs_confirm(&task) {
            self.tasks.confirm =
                Some(if act == TaskAct::Discard { Confirm::Discard(id.to_owned()) } else { Confirm::Merge(id.to_owned()) });
            return;
        }
        match act {
            TaskAct::Start => self.tasks_run(id.to_owned(), TaskOp::Start),
            TaskAct::Retry => {
                let op = if task.report.is_some() { TaskOp::Merge } else { TaskOp::Start };
                self.tasks_run(id.to_owned(), op);
            }
            TaskAct::Merge => self.tasks_run(id.to_owned(), TaskOp::Merge),
            TaskAct::Validate => self.tasks_run(id.to_owned(), TaskOp::Validate),
            TaskAct::Discard => self.tasks_run(id.to_owned(), TaskOp::Discard),
            TaskAct::Return | TaskAct::Reject => {
                self.tasks.return_open = !self.tasks.return_open;
            }
            TaskAct::GoToTab | TaskAct::ReopenTab => {
                if let Err(e) = self.open_task_tab(&task) {
                    self.tasks_set_error(e);
                }
            }
        }
    }

    /// "Stash my changes, merge and give them back" in the refusal notice: with no other confirmation, the click is already the request.
    pub fn tasks_merge_stash(&mut self, id: &str) {
        self.tasks_run(id.to_owned(), TaskOp::MergeStash);
    }

    /// "Cancel automatic merge": the task stays with the agent and waits for the review when it delivers.
    pub fn tasks_cancel_auto_merge(&mut self, id: &str) {
        self.tasks_run(id.to_owned(), TaskOp::CancelAutoMerge);
    }

    /// "Ask the agent to commit" / "…resolve the conflict": sends the task back with the request ready from the refusal reason.
    pub fn tasks_ask_agent(&mut self, id: &str) {
        let Some(task) = self.tasks.task(id) else { return };
        let feedback = self.tasks.notice_block.as_ref().and_then(|b| b.return_feedback(task)).or_else(|| {
            task.merge_stopped
                .as_ref()
                .and_then(|s| s.conflict_files.as_ref())
                .and_then(|files| MergeBlock::Conflict(files.clone()).return_feedback(task))
        });
        let Some(feedback) = feedback else { return };
        self.tasks_run(id.to_owned(), TaskOp::Return(feedback));
    }

    /// The "yes" of the confirmation panel.
    pub fn tasks_confirm_accepted(&mut self) {
        match self.tasks.confirm.take() {
            Some(Confirm::Merge(id)) => self.tasks_run(id, TaskOp::Merge),
            Some(Confirm::Discard(id)) => self.tasks_run(id, TaskOp::Discard),
            None => {}
        }
    }

    /// Sends the form's feedback: "Return" (in review) or "Reject" (awaiting validation).
    pub fn tasks_send_return(&mut self, id: &str) {
        let feedback = self.tasks.feedback.trim().to_owned();
        let Some(task) = self.tasks.task(id) else { return };
        if feedback.is_empty() {
            return;
        }
        let op = if task.status == TaskStatus::Validating { TaskOp::Reject(feedback) } else { TaskOp::Return(feedback) };
        self.tasks_run(id.to_owned(), op);
    }

    /// Is the task's agent tab open?
    pub fn tasks_tab_open(&self, task: &Task) -> bool {
        self.tabs.iter().any(|t| t.project == self.active_project && t.ident.id() == task.instance)
    }

    /// Phase or idle time of the task's agent tab, when the tab is open in the active project.
    pub fn task_tab_status(&self, task: &Task, now: std::time::Instant) -> Option<String> {
        if self.tasks.debug_hold {
            return match task.id.as_str() {
                "t6" => Some(holt_core::Phase::Writing.label().to_owned()),
                "t10" => Some(holt_core::Phase::Editing.label().to_owned()),
                _ => None,
            };
        }
        self.tabs.iter().find(|t| t.project == self.active_project && t.ident.id() == task.instance).and_then(|t| t.status_label(now))
    }

    pub fn on_tasks(&mut self, generation: u64, reply: TaskReply) {
        if generation != self.tasks.generation || self.tasks.debug_hold {
            return; // requested in the previous project (or a fake screenshot state)
        }
        match reply {
            TaskReply::List(list) => {
                self.tasks.refreshing = false;
                // On the project's first read: if something stayed in the queue (the app closed between the task's turn
                // arriving and it starting), the core checks whether it can start now.
                if !self.tasks.loaded && list.iter().any(|t| t.status == TaskStatus::Queued) {
                    let project = self.active_project.clone();
                    self.spawn_bg("tasks-queue", move |core| {
                        core.tasks_advance_queue(&project);
                        None
                    });
                }
                self.tasks.loaded = true;
                self.tasks.tasks = list;
                if std::mem::take(&mut self.tasks.dirty) {
                    self.tasks_refresh();
                }
                self.tasks_drop_stale_view();
            }
            TaskReply::Created { result, started } => {
                self.tasks.form.busy = false;
                match *result {
                    Err(e) => self.tasks.form.error = Some(e.message().to_owned()),
                    Ok(task) => {
                        let id = task.id.clone();
                        match started.map(|s| *s) {
                            Some(Ok(running)) => upsert(&mut self.tasks.tasks, running),
                            Some(Err(e)) => {
                                upsert(&mut self.tasks.tasks, task);
                                self.tasks.notice = Some(e.message().to_owned());
                            }
                            None => upsert(&mut self.tasks.tasks, task),
                        }
                        // Done: the form goes back empty (agent, base and "Start now" stay).
                        let form = &mut self.tasks.form;
                        form.title.clear();
                        form.brief.clear();
                        form.scope.clear();
                        form.attachments.clear();
                        form.error = None;
                        let notice = self.tasks.notice.take();
                        self.tasks_open_detail(&id);
                        self.tasks.notice = notice;
                        self.tasks_refresh();
                    }
                }
            }
            TaskReply::Done { id, op, result, merge } => {
                if self.tasks.busy.as_ref().is_some_and(|b| b.id == id) {
                    self.tasks.busy = None;
                }
                match *result {
                    Ok(task) => {
                        upsert(&mut self.tasks.tasks, task);
                        self.tasks_clear_notice();
                        // The merge went through, but there is something to say about the stashed changes (they came back, or stayed in the stash).
                        if let Some(note) = merge.note {
                            if self.tasks.view != TasksView::Detail(id.clone()) {
                                self.tasks_open_detail(&id);
                            }
                            self.tasks.notice = Some(note);
                            self.tasks.notice_info = true;
                        }
                        if matches!(op, TaskOp::Return(_) | TaskOp::Reject(_)) {
                            self.tasks.return_open = false;
                            self.tasks.feedback.clear();
                        }
                    }
                    Err(e) => {
                        // The notice only shows in the task's detail: an action refused through the card's button (in the
                        // list), or while looking at another task or the form, opens its detail, otherwise the
                        // refusal goes by unseen (or shows on the wrong task).
                        if self.tasks.view != TasksView::Detail(id.clone()) {
                            self.tasks_open_detail(&id);
                        }
                        self.tasks.notice = Some(e.message().to_owned());
                        self.tasks.notice_block = merge.block;
                        self.tasks.notice_stash = merge.note;
                    }
                }
                self.tasks_refresh();
            }
        }
    }

    /// The open detail points to a task that disappeared from the list: back to the list.
    fn tasks_drop_stale_view(&mut self) {
        if let TasksView::Detail(id) = &self.tasks.view {
            if self.tasks.task(id).is_none() {
                self.tasks.view = TasksView::List;
                self.tasks_clear_detail_state();
            }
        }
    }

    /// Called when the right bar starts showing the Tasks tab.
    pub fn tasks_on_open(&mut self) {
        self.tasks_refresh();
    }
}
