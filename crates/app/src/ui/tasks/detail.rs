//! One task in detail: the track, the callouts, the panes and the return form.

use super::widgets::*;
use super::*;

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

/// The pinned footer's buttons for the task's state: the main move first. Discard and the tab go to the "⋯" menu.
pub(super) fn footer_actions(task: &Task, tab_open: bool) -> Vec<(String, Act, BtnKind, &'static str)> {
    let id = || task.id.clone();
    let tab = if tab_open { TaskAct::GoToTab } else { TaskAct::ReopenTab };
    let act = |a: TaskAct, kind: BtnKind| (a.label(task), Act::Task(id(), a), kind, a.tip());
    let diff = || (tasks_ui::diff_button_label(task).to_owned(), Act::OpenDiff(id()), BtnKind::Ghost, tasks_ui::diff_button_tip(task));
    let mut v = match mark_of(task) {
        Mark::Draft => vec![act(TaskAct::Start, BtnKind::Primary)],
        Mark::Queued => Vec::new(),
        Mark::Running | Mark::Conflict => vec![act(tab, BtnKind::Secondary)],
        Mark::Review | Mark::Stopped => vec![
            act(TaskAct::Merge, BtnKind::Primary),
            (t!("Return…").to_owned(), Act::Task(id(), TaskAct::Return), BtnKind::Secondary, TaskAct::Return.tip()),
        ],
        Mark::Validate => vec![
            act(TaskAct::Validate, BtnKind::Primary),
            (t!("Reject…").to_owned(), Act::Task(id(), TaskAct::Reject), BtnKind::Secondary, TaskAct::Reject.tip()),
        ],
        Mark::Failed if task.report.is_some() => {
            vec![
                act(TaskAct::Retry, BtnKind::Primary),
                (t!("Return…").to_owned(), Act::Task(id(), TaskAct::Return), BtnKind::Secondary, TaskAct::Return.tip()),
            ]
        }
        Mark::Failed => vec![act(TaskAct::Retry, BtnKind::Primary)],
        Mark::Done | Mark::Discarded => Vec::new(),
    };
    if can_show_diff(task) {
        v.push(diff());
    }
    v
}

/// The items of the "⋯" menu: (label, act, danger).
pub(super) fn more_items(task: &Task, tab_open: bool) -> Vec<(String, Act, bool)> {
    let id = || task.id.clone();
    let all = actions_for(task, tab_open);
    let mut v = Vec::new();
    for a in [TaskAct::GoToTab, TaskAct::ReopenTab] {
        // Working, the tab is already a footer button.
        if all.contains(&a) && !matches!(task.status, TaskStatus::Running) {
            v.push((a.label(task), Act::Task(id(), a), false));
        }
    }
    if !task.brief.trim().is_empty() {
        v.push((t!("Open the request in Documents").to_owned(), Act::OpenText(id(), false), false));
    }
    if task.report.as_deref().is_some_and(|r| !r.trim().is_empty()) {
        v.push((t!("Open the report in Documents").to_owned(), Act::OpenText(id(), true), false));
    }
    if all.contains(&TaskAct::Discard) {
        v.push((t!("Discard…").to_owned(), Act::Task(id(), TaskAct::Discard), true));
    }
    v
}

pub(super) fn detail_view(deck: &mut Deck, ui: &mut Ui, rect: Rect, task: &Task, now: u64, acts: &mut Vec<Act>) {
    let tab_open = deck.tasks_tab_open(task);
    let more = more_items(task, tab_open);
    let (head, more_rect) = head_bar(ui, rect, Some(&task.id), &task.title, !more.is_empty(), acts);
    let more_open = ui.ctx().data(|d| d.get_temp::<bool>(more_id())).unwrap_or(false);
    if let Some(m) = more_rect {
        let r = clickable(ui, m, Id::new("tasks-more"));
        let p = theme::pal();
        if r.hovered() || more_open {
            ui.painter().rect_filled(m, 5.0, if more_open { p.selected } else { p.hover });
        }
        for dx in [-4.5, 0.0, 4.5] {
            ui.painter().circle_filled(m.center() + Vec2::new(dx, 0.0), 1.4, if r.hovered() || more_open { p.text } else { p.text_2 });
        }
        if tip(r, t!("More actions")).clicked() {
            acts.push(Act::ToggleMore);
        }
    }
    let busy = deck.tasks.busy.clone();
    let working = busy.is_some();
    let busy_here = busy.as_ref().filter(|b| b.id == task.id).map(|b| b.label);
    if busy_here.is_some() {
        crate::ui::git::busy_bar(ui, Rect::from_min_max(Pos2::new(rect.min.x, head.max.y - 2.0), Pos2::new(rect.max.x, head.max.y)));
    }
    let return_open = deck.tasks.return_open && matches!(task.status, TaskStatus::Review | TaskStatus::Validating | TaskStatus::Failed);
    let buttons = footer_actions(task, tab_open);

    // The pinned footer: the return/reject field and its buttons, or the state's actions (with the busy line).
    let footer_h = if return_open {
        return_form_height(ui, task)
    } else if buttons.is_empty() && busy_here.is_none() {
        0.0
    } else {
        FOOTER_H + if busy_here.is_some() { 22.0 } else { 0.0 }
    };
    let footer = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - footer_h), rect.max);
    let body = Rect::from_min_max(Pos2::new(rect.min.x, head.max.y), Pos2::new(rect.max.x, footer.min.y));

    let mut child = ui.new_child(UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(body);
    egui::ScrollArea::vertical().id_salt(("tasks-detail", &task.id)).auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        detail_top(deck, ui, task, now);
        track(ui, task, now);
        ui.add_space(2.0);
        callouts(deck, ui, task, working, acts);
        ui.add_space(12.0);
        let tab = deck.tasks.detail_tab.unwrap_or_else(|| DetailTab::default_for(task));
        detail_tabs(ui, task, tab, acts);
        ui.add_space(10.0);
        match tab {
            DetailTab::Report => {
                report_pane(ui, task, acts);
                ui.add_space(14.0);
                pane_label(ui, t!("Changes"));
                changes_pane(ui, task, acts);
            }
            DetailTab::Timeline => timeline_pane(deck, ui, task, acts),
            DetailTab::Request => request_pane(ui, task, acts),
        }
        ui.add_space(14.0);
    });

    if footer_h > 0.0 {
        let painter = ui.painter_at(footer);
        line_sep(&painter, Pos2::new(footer.min.x, footer.min.y + 0.5), Pos2::new(footer.max.x, footer.min.y + 0.5));
        let mut fui =
            ui.new_child(UiBuilder::new().max_rect(footer.shrink2(Vec2::new(0.0, 8.0))).layout(egui::Layout::top_down(egui::Align::Min)));
        fui.spacing_mut().item_spacing = Vec2::ZERO;
        if return_open {
            return_form(deck, &mut fui, task, working, acts);
        } else {
            if let Some(label) = busy_here {
                let (r, _) = fui.allocate_exact_size(Vec2::new(fui.available_width(), 22.0), Sense::hover());
                egui::Spinner::new()
                    .size(14.0)
                    .color(theme::pal().text_2)
                    .paint_at(&fui, Rect::from_center_size(Pos2::new(r.min.x + PAD + 7.0, r.center().y), Vec2::splat(14.0)));
                text_in(
                    fui.painter(),
                    Rect::from_min_max(Pos2::new(r.min.x + PAD + 22.0, r.min.y), r.max),
                    Align2::LEFT_CENTER,
                    label,
                    theme::ui(12.0),
                    theme::pal().text_2,
                );
            }
            let row: Vec<(String, BtnKind, bool, &str)> = buttons
                .iter()
                .map(|(label, act, kind, tooltip)| (label.clone(), *kind, !working || matches!(act, Act::OpenDiff(_)), *tooltip))
                .collect();
            if let Some(i) = button_flow(&mut fui, &row, "detail-footer") {
                if let Some((_, act, ..)) = buttons.into_iter().nth(i) {
                    acts.push(act);
                }
            }
        }
    }

    if more_open {
        if let Some(anchor) = more_rect {
            more_menu(ui.ctx(), anchor, &more, working, acts);
        }
    }
}

/// Title, the state sentence, the agent and the branch.
pub(super) fn detail_top(deck: &mut Deck, ui: &mut Ui, task: &Task, now: u64) {
    let p = theme::pal();
    let mark = mark_of(task);
    ui.add_space(12.0);
    let w = ui.available_width() - 2.0 * PAD;
    let g = ui.painter().layout(task.title.clone(), theme::ui_semibold(15.0), p.text_strong, w);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), g.size().y + 8.0), Sense::hover());
    ui.painter().galley(Pos2::new(r.min.x + PAD, r.min.y), g, p.text_strong);

    // ◉ Waiting for your review · ready 6 min ago
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    paint_mark(ui, Pos2::new(r.min.x + PAD + 8.0, r.center().y), mark, deck.agents.tab_color(&task.kind));
    let color = mark_color(mark);
    let g = galley(ui.painter(), state_sentence(task), theme::ui_semibold(12.0), color);
    let s = place(ui.painter(), Rect::from_min_max(Pos2::new(r.min.x + PAD + 24.0, r.min.y), r.max), Align2::LEFT_CENTER, g, color);
    let when = format!("· {}", status_age(task, now));
    let g = galley_trunc(ui.painter(), &when, theme::ui(12.0), p.text_2, (r.max.x - PAD - s.max.x - 6.0).max(10.0));
    place(
        ui.painter(),
        Rect::from_min_max(Pos2::new(s.max.x + 6.0, r.min.y), Pos2::new(r.max.x - PAD, r.max.y)),
        Align2::LEFT_CENTER,
        g,
        p.text_2,
    );
    ui.add_space(6.0);

    // Agent · model · effort, then the branch (mono) and the scope.
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let painter = ui.painter().clone();
    if task.manual {
        let g = galley_trunc(&painter, t!("Manual · no agent"), theme::ui(11.5), p.text_2, (r.width() - 2.0 * PAD - 18.0).max(10.0));
        place(&painter, Rect::from_min_max(Pos2::new(r.min.x + PAD + 18.0, r.min.y), r.max), Align2::LEFT_CENTER, g, p.text_2);
    } else {
        paint_agent(deck, &painter, Pos2::new(r.min.x + PAD + 6.0, r.center().y), &task.kind);
        let mut who = deck.agents.name(&task.kind);
        if let Some(m) = tasks_ui::model_effort_text(task) {
            who.push_str(" · ");
            who.push_str(&m);
        }
        let g = galley_trunc(&painter, &who, theme::ui(11.5), p.text_2, (r.width() - 2.0 * PAD - 18.0).max(10.0));
        place(&painter, Rect::from_min_max(Pos2::new(r.min.x + PAD + 18.0, r.min.y), r.max), Align2::LEFT_CENTER, g, p.text_2);
    }
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    deck.icons.paint(&painter, Icon::GitBranch, Pos2::new(r.min.x + PAD + 6.0, r.center().y), 12.0, p.text_3, 0.0);
    let g = galley_trunc(&painter, &tasks_ui::branch_line(task), theme::mono(11.0), p.text_2, (r.width() - 2.0 * PAD - 18.0).max(10.0));
    place(&painter, Rect::from_min_max(Pos2::new(r.min.x + PAD + 18.0, r.min.y), r.max), Align2::LEFT_CENTER, g, p.text_2);
    tip(
        resp,
        format!(
            "{}\n{}",
            tasks_ui::branch_line(task),
            t!("Task branch → branch it will be merged into (and the merge commit, if there was one)")
        ),
    );
    if !task.scope.is_empty() {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
        let text = t!("Scope: {scope}", scope = task.scope.join(", "));
        let g = galley_trunc(&painter, &text, theme::mono(11.0), p.text_3, (r.width() - 2.0 * PAD - 18.0).max(10.0));
        place(&painter, Rect::from_min_max(Pos2::new(r.min.x + PAD + 18.0, r.min.y), r.max), Align2::LEFT_CENTER, g, p.text_3);
        tip(resp, task.scope.join("\n"));
    }
}

/// The lifecycle track: five stops on a line, the reached ones filled, the current one in its colour with a halo,
/// the label and the time under each.
pub(super) fn track(ui: &mut Ui, task: &Task, now: u64) {
    let p = theme::pal();
    let stops = lifecycle(task, now);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
    let painter = ui.painter();
    let inner = Rect::from_min_max(Pos2::new(r.min.x + PAD, r.min.y + 12.0), Pos2::new(r.max.x - PAD, r.max.y));
    let col = inner.width() / 5.0;
    let cx = |i: usize| inner.min.x + col * (i as f32 + 0.5);
    let y = inner.min.y + 6.0;
    let last_reached = stops.iter().rposition(|s| s.state != StopState::Later).unwrap_or(0);
    painter.line_segment([Pos2::new(cx(0), y), Pos2::new(cx(4), y)], Stroke::new(2.0, p.separator));
    if last_reached > 0 {
        painter.line_segment([Pos2::new(cx(0), y), Pos2::new(cx(last_reached), y)], Stroke::new(2.0, theme::with_alpha(p.text_2, 0.7)));
    }
    for (i, s) in stops.iter().enumerate() {
        let c = Pos2::new(cx(i), y);
        let (fill, ring, halo) = match s.state {
            StopState::Done => (p.text_2, p.text_2, None),
            StopState::Now => (theme::accent(), theme::accent(), Some(theme::accent_soft(theme::accent()))),
            StopState::Warn => (p.warning, p.warning, Some(theme::with_alpha(p.warning, 0.22))),
            StopState::Fail => (p.danger, p.danger, Some(theme::with_alpha(p.danger, 0.22))),
            StopState::Later => (p.bg, p.separator, None),
        };
        if let Some(h) = halo {
            painter.circle_filled(c, 9.0, h);
        }
        painter.circle_filled(c, 6.0, ring);
        painter.circle_filled(c, 4.0, fill);
        let current = !matches!(s.state, StopState::Done | StopState::Later);
        let (font, color) = if current { (theme::ui_semibold(10.5), p.text) } else { (theme::ui(10.5), p.text_3) };
        let g = galley_trunc(painter, s.label, font, color, col - 2.0);
        place(painter, Rect::from_center_size(Pos2::new(c.x, y + 17.0), Vec2::new(col, 14.0)), Align2::CENTER_CENTER, g, color);
        let when = if s.when.is_empty() { "—" } else { s.when.as_str() };
        let g = galley_trunc(painter, when, theme::meta(10.0), p.text_faint, col - 2.0);
        place(painter, Rect::from_center_size(Pos2::new(c.x, y + 31.0), Vec2::new(col, 13.0)), Align2::CENTER_CENTER, g, p.text_faint);
    }
}

/// The explanations of the state, each where it belongs: the queue, the files outside the scope, the conflict being
/// resolved (with "Cancel automatic merge"), the merge that stopped (with its way out), the failure and the last
/// action's notice.
pub(super) fn callouts(deck: &mut Deck, ui: &mut Ui, task: &Task, working: bool, acts: &mut Vec<Act>) {
    let notice = deck.tasks.notice.clone();
    let notice_info = deck.tasks.notice_info;
    let notice_block = deck.tasks.notice_block.clone();
    let notice_stash = deck.tasks.notice_stash.clone();
    let notice_files = deck.tasks.notice_files;
    if let (TaskStatus::Queued, Some(holder)) = (task.status, task.waiting_for.as_deref()) {
        let text = t!(
            "Queued: the scope overlaps task {holder}'s scope. It starts by itself when that task is merged, discarded or fails, branching from {base} at that moment.",
            holder,
            base = task.base
        );
        notice_box(ui, &text, false, false);
    }
    if task.status == TaskStatus::Review && !task.outside_scope.is_empty() {
        let shown: Vec<&str> = task.outside_scope.iter().take(OUTSIDE_SHOWN).map(String::as_str).collect();
        let more = task.outside_scope.len().saturating_sub(OUTSIDE_SHOWN);
        let files = shown.join(", ");
        let text = if more > 0 {
            t!("Touched outside the scope: {files} and {more} more", files, more)
        } else {
            t!("Touched outside the scope: {files}", files)
        };
        notice_box(ui, &text, false, false);
    }
    if task.status == TaskStatus::Running && task.merge_approved {
        let text = t!(
            "You approved the merge, but {base} moved and conflicts. The agent is resolving it; the task merges by itself when it delivers.",
            base = task.base
        );
        notice_box(ui, &text, false, false);
        let b = [(
            t!("Cancel automatic merge").to_owned(),
            BtnKind::Secondary,
            !working,
            t!("The task will wait in review when the agent delivers, and you merge it yourself"),
        )];
        if button_flow(ui, &b, "cancel-auto-merge").is_some() {
            acts.push(Act::CancelAutoMerge(task.id.clone()));
        }
    }
    if let Some(stop) = task.merge_stopped.as_ref().filter(|_| task.status == TaskStatus::Review && notice.is_none()) {
        let text = t!("The task was not merged by itself: {reason}", reason = &stop.reason);
        error_box(ui, &text, false);
        if let Some(files) = &stop.conflict_files {
            refusal_buttons(ui, task, &MergeBlock::Conflict(files.clone()), false, !working, acts);
        }
    }
    if let Some(err) = task.error.as_deref().filter(|_| task.status == TaskStatus::Failed) {
        error_box(ui, err, false);
    }
    if let Some(n) = &notice {
        // A merge refusal with a way out: the plain-language explanation in place of git's text, and the way-out button.
        // The stash notice goes along: it is the only sentence that says where the user's changes went.
        let shown = match (&notice_block, &notice_stash) {
            (Some(b), Some(note)) => format!("{} {note}", b.explain(task)),
            (Some(b), None) => b.explain(task),
            (None, _) => n.clone(),
        };
        if notice_box(ui, &shown, true, !notice_info) {
            acts.push(Act::DismissNotice);
        }
        if let Some(block) = &notice_block {
            refusal_buttons(ui, task, block, notice_files, !working, acts);
        }
    }
}

/// The count shown on a pane's tab (files, timeline items), when there is one.
pub(super) fn tab_count(task: &Task, tab: DetailTab) -> Option<usize> {
    match tab {
        DetailTab::Timeline => Some(task.events.len() + task.messages.len()).filter(|n| *n > 0),
        DetailTab::Report | DetailTab::Request => None,
    }
}

/// The pane switch: a 26 px track (`recess`) with four segments, the label and its count.
pub(super) fn detail_tabs(ui: &mut Ui, task: &Task, current: DetailTab, acts: &mut Vec<Act>) {
    let p = theme::pal();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
    let track = Rect::from_min_max(Pos2::new(r.min.x + PAD, r.min.y), Pos2::new(r.max.x - PAD, r.max.y));
    let painter = ui.painter().clone();
    painter.rect_filled(track, 6.0, p.recess);
    let tabs = DetailTab::ALL;
    // Each segment is as wide as its text needs, the rest shared equally (the labels never cut in Portuguese).
    let widths: Vec<f32> = tabs
        .iter()
        .map(|t| {
            let label = text_w(&painter, t.label(), theme::ui_semibold(11.5));
            let count = tab_count(task, *t).map_or(0.0, |n| 4.0 + text_w(&painter, &n.to_string(), theme::meta(10.0)));
            label + count + 12.0
        })
        .collect();
    let spare = ((track.width() - 4.0 - 6.0) - widths.iter().sum::<f32>()).max(0.0) / tabs.len() as f32;
    let mut x = track.min.x + 2.0;
    for (i, tab) in tabs.iter().enumerate() {
        let seg = Rect::from_min_size(Pos2::new(x, track.min.y + 2.0), Vec2::new(widths[i] + spare, track.height() - 4.0));
        x = seg.max.x + 2.0;
        let on = *tab == current;
        let resp = clickable(ui, seg, Id::new(("tasks-pane", i)));
        if on {
            painter.rect_filled(seg, 5.0, p.selected);
        } else if resp.hovered() {
            painter.rect_filled(seg, 5.0, p.hover);
        }
        let (font, color) = if on { (theme::ui_semibold(11.5), p.text_strong) } else { (theme::ui(11.5), p.text_2) };
        let label = galley(&painter, tab.label(), font, color);
        let count = tab_count(task, *tab).map(|n| galley(&painter, &n.to_string(), theme::meta(10.0), p.text_3));
        let total = label.size().x + count.as_ref().map_or(0.0, |c| 4.0 + c.size().x);
        let lx = seg.center().x - total / 2.0;
        let lr = place(
            &painter,
            Rect::from_min_max(Pos2::new(lx, seg.min.y), Pos2::new(seg.max.x, seg.max.y)),
            Align2::LEFT_CENTER,
            label,
            color,
        );
        if let Some(c) = count {
            place(
                &painter,
                Rect::from_min_max(Pos2::new(lr.max.x + 4.0, seg.min.y), Pos2::new(seg.max.x, seg.max.y)),
                Align2::LEFT_CENTER,
                c,
                p.text_3,
            );
        }
        if resp.clicked() && !on {
            acts.push(Act::SetTab(*tab));
        }
    }
}

/// A label inside a pane (the app's section label), for the second block of the Report pane.
pub(super) fn pane_label(ui: &mut Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    let p = theme::pal();
    let g = galley_spaced(ui.painter(), &text.to_uppercase(), theme::label(10.0), p.text_3, 10.0 * theme::LABEL_SPACING_EM);
    place(ui.painter(), Rect::from_min_max(Pos2::new(r.min.x + PAD, r.min.y), r.max), Align2::LEFT_CENTER, g, p.text_3);
}

/// "Open in Documents" (secondary), under the request and the report.
pub(super) fn open_text_button(ui: &mut Ui, task: &Task, report: bool, acts: &mut Vec<Act>) {
    let tip = if report {
        t!("Opens the report formatted, with the full width of the center")
    } else {
        t!("Opens the request formatted, with the full width of the center")
    };
    let b = [(t!("Open in Documents").to_owned(), BtnKind::Secondary, true, tip)];
    ui.add_space(4.0);
    if button_flow(ui, &b, if report { "open-report" } else { "open-brief" }).is_some() {
        acts.push(Act::OpenText(task.id.clone(), report));
    }
}

pub(super) fn report_pane(ui: &mut Ui, task: &Task, acts: &mut Vec<Act>) {
    let p = theme::pal();
    match task.report.as_deref().filter(|r| !r.trim().is_empty()) {
        Some(report) => {
            paragraph(ui, &clip_text(report.trim(), TEXT_MAX), theme::ui(12.0), p.text);
            open_text_button(ui, task, true, acts);
        }
        None if matches!(task.status, TaskStatus::Running | TaskStatus::Queued | TaskStatus::Draft) => {
            muted_line(ui, t!("The agent has not delivered the report yet."))
        }
        None => muted_line(ui, t!("No report.")),
    }
}

pub(super) fn request_pane(ui: &mut Ui, task: &Task, acts: &mut Vec<Act>) {
    let p = theme::pal();
    if task.brief.trim().is_empty() {
        muted_line(ui, t!("No request besides the title: the agent gets only the title."));
    } else {
        paragraph(ui, &clip_text(task.brief.trim(), TEXT_MAX), theme::ui(12.0), p.text);
        open_text_button(ui, task, false, acts);
    }
}

/// The diff stat, one file per row: the path (cut from the start), the +/− split as a 44 px bar and the changed lines.
pub(super) fn changes_pane(ui: &mut Ui, task: &Task, acts: &mut Vec<Act>) {
    let p = theme::pal();
    let summary = task.diff_stat.as_deref().and_then(parse_diff_stat);
    let lines = task.diff_stat.as_deref().map(tasks_ui::parse_stat_lines).unwrap_or_default();
    if lines.is_empty() {
        let text = match summary {
            Some(s) if s.files == 0 => t!("No changes against the base."),
            _ if can_show_diff(task) => t!("The summary arrives with the report."),
            _ => t!("No changes recorded."),
        };
        muted_line(ui, text);
    } else {
        let painter = ui.painter().clone();
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
        let line = Rect::from_min_max(Pos2::new(r.min.x + PAD + 6.0, r.min.y), Pos2::new(r.max.x - PAD - 6.0, r.max.y));
        let what = if tasks_ui::shows_merged_diff(task) {
            t!("what the merge brought into {base}", base = task.base)
        } else {
            t!("against {base}", base = task.base)
        };
        let left = summary.map_or(line.max.x, |s| paint_diff_counts(&painter, line, s.insertions, s.deletions) - 8.0);
        let g = galley_trunc(&painter, &what, theme::meta(10.5), p.text_3, (left - line.min.x).max(10.0));
        place(&painter, Rect::from_min_max(line.min, Pos2::new(left, line.max.y)), Align2::LEFT_CENTER, g, p.text_3);
        ui.add_space(2.0);
        let shown = lines.len().min(STAT_MAX_LINES);
        for l in lines.iter().take(shown) {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
            let row = Rect::from_min_max(Pos2::new(r.min.x + PAD, r.min.y), Pos2::new(r.max.x - PAD, r.max.y));
            if resp.hovered() {
                painter.rect_filled(row, 5.0, p.hover);
            }
            let count = galley(&painter, &l.changes, theme::meta(10.5), p.text_3);
            let cw = count.size().x.max(24.0);
            let cr = place(
                &painter,
                Rect::from_min_max(Pos2::new(row.max.x - 6.0 - cw, row.min.y), Pos2::new(row.max.x - 6.0, row.max.y)),
                Align2::RIGHT_CENTER,
                count,
                p.text_3,
            );
            let bar =
                Rect::from_center_size(Pos2::new(cr.min.x.min(row.max.x - 6.0 - cw) - 8.0 - 22.0, row.center().y), Vec2::new(44.0, 4.0));
            match l.plus {
                Some(share) => {
                    painter.rect_filled(bar, 2.0, p.danger);
                    if share > 0.0 {
                        let plus = Rect::from_min_size(bar.min, Vec2::new(bar.width() * share, bar.height()));
                        painter.rect_filled(plus, 2.0, p.success);
                    }
                }
                None => {
                    painter.rect_filled(bar, 2.0, p.separator);
                }
            }
            let g = tail_galley(&painter, &l.path, theme::mono(11.0), p.text_2, (bar.min.x - 8.0 - row.min.x - 6.0).max(20.0));
            place(
                &painter,
                Rect::from_min_max(Pos2::new(row.min.x + 6.0, row.min.y), Pos2::new(bar.min.x - 8.0, row.max.y)),
                Align2::LEFT_CENTER,
                g,
                p.text_2,
            );
            tip(resp, l.path.clone());
        }
        let extra = lines.len() - shown;
        if extra > 0 {
            let more = if extra == 1 { t!("… and 1 more file").to_owned() } else { t!("… and {extra} more files", extra) };
            muted_line(ui, &more);
        }
    }
    if can_show_diff(task) {
        ui.add_space(6.0);
        let b = [(tasks_ui::diff_button_label(task).to_owned(), BtnKind::Secondary, true, tasks_ui::diff_button_tip(task))];
        if button_flow(ui, &b, "diff").is_some() {
            acts.push(Act::OpenDiff(task.id.clone()));
        }
    }
}

/// Cuts the start of the text (`…ui/tasks.rs`) to fit `max_w`: in a path, the end is what identifies it.
pub(super) fn tail_galley(
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_w: f32,
) -> std::sync::Arc<egui::Galley> {
    let full = galley(painter, text, font.clone(), color);
    if full.size().x <= max_w {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let cut: String = std::iter::once('…').chain(chars[mid..].iter().copied()).collect();
        if galley(painter, &cut, font.clone(), color).size().x <= max_w {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let cut: String = std::iter::once('…').chain(chars[lo..].iter().copied()).collect();
    galley(painter, &cut, font, color)
}

/// Events and messages in one stream, newest first, along a vertical guide: an event is a hollow dot with its time and
/// text; a message is a dot in the speaker's colour (red for an error), `from → to` and the text in a bubble, collapsed
/// to its first lines (a click opens it whole).
pub(super) fn timeline_pane(deck: &mut Deck, ui: &mut Ui, task: &Task, acts: &mut Vec<Act>) {
    let p = theme::pal();
    let moments = tasks_ui::timeline(task);
    if moments.is_empty() {
        muted_line(ui, t!("Nothing recorded yet."));
        return;
    }
    let guide_x = ui.max_rect().min.x + PAD + 4.0;
    let text_x = guide_x + 13.0;
    let text_w_max = (ui.max_rect().max.x - PAD - text_x).max(60.0);
    let top = ui.cursor().min.y;
    let mut last_dot = top;
    let painter = ui.painter().clone();
    // The guide goes behind the dots: its slot is reserved first and filled at the end.
    let guide = painter.add(Shape::Noop);
    let time_w = text_w(&painter, "00/00 00:00", theme::meta(10.5));
    for m in moments.iter().take(MOMENTS_MAX) {
        let when = tasks_ui::event_time(m.at_ms());
        match m {
            Moment::Event(e) => {
                let g = painter.layout(
                    clip_text(&e.text, 1_000).into_owned(),
                    theme::ui(11.5),
                    p.text_2,
                    (text_w_max - time_w - 8.0).max(40.0),
                );
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), g.size().y + 8.0), Sense::hover());
                let first = g.rows.first().map_or(15.0, |row| row.height());
                let c = Pos2::new(guide_x, r.min.y + 2.0 + first / 2.0);
                painter.circle_filled(c, 4.0, theme::pal().bg);
                painter.circle_stroke(c, 3.5, Stroke::new(1.4, p.text_3));
                last_dot = c.y;
                let t = galley(&painter, &when, theme::meta(10.5), p.text_faint);
                place(
                    &painter,
                    Rect::from_min_size(Pos2::new(text_x, r.min.y + 2.0), Vec2::new(time_w, first)),
                    Align2::LEFT_CENTER,
                    t,
                    p.text_faint,
                );
                painter.galley(Pos2::new(text_x + time_w + 8.0, r.min.y + 2.0), g, p.text_2);
            }
            Moment::Message(msg) => {
                let open = deck.tasks.open_messages.contains(&(task.id.clone(), msg.at_ms));
                let (preview, more) = tasks_ui::message_preview(&msg.text);
                let shown = if open { clip_text(msg.text.trim(), TEXT_MAX).into_owned() } else { preview };
                let error = msg.kind == FeedKind::Error;
                let body_color = if error { p.danger_text } else { p.text_2 };
                let body = painter.layout(shown, theme::ui(12.0), body_color, text_w_max - 18.0);
                let head_h = 17.0;
                let foot_h = if more { 15.0 } else { 0.0 };
                let bubble_h = 7.0 + body.size().y + foot_h + 7.0;
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), head_h + 3.0 + bubble_h + 8.0), Sense::click());
                let from = msg.from.as_deref().unwrap_or("…");
                let kind = from.split('@').next().unwrap_or(from);
                let dot_color = if error { p.danger } else { deck.agents.tab_color(kind) };
                let c = Pos2::new(guide_x, r.min.y + head_h / 2.0);
                painter.circle_filled(c, 4.0, dot_color);
                last_dot = c.y;
                // time · from → to · kind
                let t = galley(&painter, &when, theme::meta(10.5), p.text_faint);
                let tr = place(
                    &painter,
                    Rect::from_min_size(Pos2::new(text_x, r.min.y), Vec2::new(time_w, head_h)),
                    Align2::LEFT_CENTER,
                    t,
                    p.text_faint,
                );
                let mut job = egui::text::LayoutJob {
                    wrap: egui::text::TextWrapping {
                        max_width: (r.max.x - PAD - tr.max.x - 8.0).max(20.0),
                        max_rows: 1,
                        break_anywhere: true,
                        overflow_character: Some('…'),
                    },
                    ..Default::default()
                };
                job.append(from, 0.0, egui::TextFormat { font_id: theme::ui_semibold(11.0), color: p.text, ..Default::default() });
                job.append(
                    &format!(" → {}", msg.to.as_deref().unwrap_or("…")),
                    0.0,
                    egui::TextFormat { font_id: theme::ui(11.0), color: p.text_3, ..Default::default() },
                );
                job.append(
                    &format!("  {}", tasks_ui::message_kind_label(msg.kind)),
                    0.0,
                    egui::TextFormat {
                        font_id: theme::ui(11.0),
                        color: if error { p.danger_text } else { p.text_3 },
                        ..Default::default()
                    },
                );
                let hg = painter.layout_job(job);
                place(
                    &painter,
                    Rect::from_min_max(Pos2::new(tr.max.x + 8.0, r.min.y), Pos2::new(r.max.x - PAD, r.min.y + head_h)),
                    Align2::LEFT_CENTER,
                    hg,
                    p.text,
                );
                let bubble = Rect::from_min_size(Pos2::new(text_x, r.min.y + head_h + 3.0), Vec2::new(text_w_max, bubble_h));
                let border = if error { theme::with_alpha(p.danger, 0.3) } else { theme::with_alpha(p.text_strong, 0.06) };
                let fill = if more && resp.hovered() { p.hover } else { theme::with_alpha(p.text_strong, 0.035) };
                frame(&painter, bubble, 7.0, fill, border);
                painter.galley(Pos2::new(bubble.min.x + 9.0, bubble.min.y + 7.0), body.clone(), body_color);
                if more {
                    text_in(
                        &painter,
                        Rect::from_min_size(
                            Pos2::new(bubble.min.x + 9.0, bubble.min.y + 7.0 + body.size().y),
                            Vec2::new(bubble.width() - 18.0, foot_h),
                        ),
                        Align2::LEFT_CENTER,
                        if open { t!("collapse") } else { t!("see all") },
                        theme::meta(10.5),
                        p.text_3,
                    );
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if tip(resp.clone(), if open { t!("Collapses the message") } else { t!("Opens the whole message") }).clicked() {
                        acts.push(Act::ToggleMessage(task.id.clone(), msg.at_ms));
                    }
                }
            }
        }
    }
    // The guide, from the first dot to the last, behind them.
    painter.set(guide, Shape::line_segment([Pos2::new(guide_x, top + 6.0), Pos2::new(guide_x, last_dot)], Stroke::new(1.0, p.separator)));
    if moments.len() > MOMENTS_MAX {
        let older = moments.len() - MOMENTS_MAX;
        let text = if older == 1 { t!("… and 1 older item.").to_owned() } else { t!("… and {older} older items.", older) };
        muted_line(ui, &text);
    }
}

/// The "⋯" menu of the detail, under its button.
pub(super) fn more_menu(ctx: &egui::Context, anchor: Rect, items: &[(String, Act, bool)], working: bool, acts: &mut Vec<Act>) {
    let p = theme::pal();
    let row = 26.0;
    let w = 230.0;
    let h = items.len() as f32 * row + 8.0;
    let pos = crate::ui::menus::keep_on_screen(ctx, Pos2::new(anchor.max.x - w, anchor.max.y + 2.0), Vec2::new(w, h));
    let menu_rect = Rect::from_min_size(pos, Vec2::new(w, h));
    let mut chosen = None;
    Area::new(Id::new("tasks-more-menu")).order(Order::Foreground).fixed_pos(pos).constrain(false).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        shadow(ui.painter(), rect, 8, 8, 24, p.shadow);
        frame(ui.painter(), rect, 8.0, p.popup, p.glass_border);
        for (i, (label, _, danger)) in items.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(rect.min.x + 4.0, rect.min.y + 4.0 + i as f32 * row), Vec2::new(w - 8.0, row));
            let enabled = !working || !danger;
            let resp = if enabled {
                clickable(ui, r, Id::new(("tasks-more-item", i)))
            } else {
                ui.interact(r, Id::new(("tasks-more-item", i)), Sense::hover())
            };
            if enabled && resp.hovered() {
                ui.painter().rect_filled(r, 5.0, p.hover);
            }
            let color = if !enabled {
                p.text_3
            } else if *danger {
                p.danger_text
            } else if resp.hovered() {
                p.text
            } else {
                p.text_2
            };
            let g = galley_trunc(ui.painter(), label, theme::ui(12.0), color, r.width() - 20.0);
            place(ui.painter(), Rect::from_min_max(Pos2::new(r.min.x + 10.0, r.min.y), r.max), Align2::LEFT_CENTER, g, color);
            if enabled && resp.clicked() {
                chosen = Some(i);
            }
        }
    });
    match chosen {
        Some(i) => {
            acts.push(Act::CloseMore);
            if let Some((_, act, _)) = items.get(i) {
                acts.push(match act {
                    Act::Task(id, a) => Act::Task(id.clone(), *a),
                    Act::OpenText(id, r) => Act::OpenText(id.clone(), *r),
                    _ => Act::CloseMore,
                });
            }
        }
        None if ctx.input(|i| i.key_pressed(Key::Escape)) || crate::ui::menus::closes_on_outside(ctx, menu_rect, anchor) => {
            acts.push(Act::CloseMore)
        }
        None => {}
    }
}

/// The buttons of the way out of the merge refusal (and, for what is uncommitted, the full list of files).
pub(super) fn refusal_buttons(ui: &mut Ui, task: &Task, block: &MergeBlock, files_open: bool, enabled: bool, acts: &mut Vec<Act>) {
    let mergeable = matches!(task.status, TaskStatus::Review | TaskStatus::Failed);
    if !mergeable {
        return;
    }
    let (buttons, files): (Vec<(String, BtnKind, bool, &str)>, bool) = match block {
        MergeBlock::MainDirty(_) => (
            vec![
                (
                    t!("Stash my changes and merge").to_owned(),
                    BtnKind::Primary,
                    enabled,
                    t!("Stashes your uncommitted changes (git stash), merges the task and gives the changes back; if they clash with the task, they stay stashed and the notice says how to recover them"),
                ),
                (
                    if files_open { t!("Hide the list") } else { t!("See what is uncommitted") }.to_owned(),
                    BtnKind::Secondary,
                    true,
                    t!("Shows all the files with uncommitted changes in the main folder"),
                ),
            ],
            true,
        ),
        MergeBlock::TaskDirty(_) => (
            vec![(
                t!("Ask the agent to commit").to_owned(),
                BtnKind::Primary,
                enabled,
                t!("Sends the task back to the agent with the list of files and a request to commit"),
            )],
            false,
        ),
        MergeBlock::Conflict(_) => (
            vec![(
                t!("Ask the agent to resolve the conflict").to_owned(),
                BtnKind::Primary,
                enabled,
                t!("Sends the task back to the agent asking it to merge the base into its branch (git merge) and resolve the conflicting files"),
            )],
            false,
        ),
    };
    match button_flow(ui, &buttons, "refusal") {
        Some(0) if files => acts.push(Act::MergeStash(task.id.clone())),
        Some(0) => acts.push(Act::AskAgent(task.id.clone())),
        Some(_) => acts.push(Act::ToggleNoticeFiles),
        None => {}
    }
    if files_open && files {
        let list = block.files().join("\n");
        paragraph(ui, &clip_text(&list, TEXT_MAX), theme::mono(11.0), theme::pal().text_2);
    }
}

/// The footer's height with the return/reject field open (the field, its hint and the buttons).
pub(super) fn return_form_height(ui: &Ui, task: &Task) -> f32 {
    let w = ui.max_rect().width() - 2.0 * PAD;
    let hint = ui.painter().layout(return_hint(task).to_owned(), theme::ui(11.0), theme::pal().text_3, w).size().y;
    8.0 + 92.0 + 5.0 + hint + 8.0 + BTN_H + 6.0 + 8.0
}

/// What the agent gets with the feedback (the hint under the field).
pub(super) fn return_hint(task: &Task) -> &'static str {
    if task.status == TaskStatus::Validating {
        t!("The agent gets this with the note that the work is already merged. Ctrl+Enter sends.")
    } else {
        t!("The agent gets this as a request for changes. Ctrl+Enter sends.")
    }
}

/// Feedback field of "Return" / "Reject", pinned in the footer, with "Send" and "Cancel".
pub(super) fn return_form(deck: &mut Deck, ui: &mut Ui, task: &Task, working: bool, acts: &mut Vec<Act>) {
    // In review it is sent back with adjustments; awaiting validation, it is rejected with what did not work in the test.
    let (hint, send, send_tip, cancel_tip) = if task.status == TaskStatus::Validating {
        (
            t!("What did not work in the test?"),
            t!("Reject and send"),
            t!("Sends the task back to the same agent, saying the work was already merged and what failed in the test"),
            t!("Closes without rejecting"),
        )
    } else {
        (
            t!("What does the agent need to change?"),
            t!("Send to the agent"),
            t!("Sends the task back to the agent with this request for changes"),
            t!("Closes without returning"),
        )
    };
    let id = Id::new("tasks-return-feedback");
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    if !focused && deck.tasks.feedback.is_empty() {
        ui.ctx().memory_mut(|m| m.request_focus(id));
    }
    let ready = !working && !deck.tasks.feedback.trim().is_empty();
    let ctrl_enter = focused && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
    padded(ui, |ui| {
        text_field(ui, id, &mut deck.tasks.feedback, hint, Some(4));
        ui.add_space(5.0);
        let g = ui.painter().layout(return_hint(task).to_owned(), theme::ui(11.0), theme::pal().text_3, ui.available_width());
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), g.size().y), Sense::hover());
        ui.painter().galley(r.min, g, theme::pal().text_3);
    });
    let b = [(send.to_owned(), BtnKind::Primary, ready, send_tip), (t!("Cancel").to_owned(), BtnKind::Secondary, true, cancel_tip)];
    ui.add_space(8.0);
    match button_flow(ui, &b, "return") {
        Some(0) => acts.push(Act::SendReturn(task.id.clone())),
        Some(_) => acts.push(Act::CancelReturn),
        None if ctrl_enter && ready => acts.push(Act::SendReturn(task.id.clone())),
        None => {}
    }
}
