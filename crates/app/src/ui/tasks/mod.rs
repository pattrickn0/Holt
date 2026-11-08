//! The right bar's "Tasks" tab, as a queue: the tasks grouped by who has the next move (you, an agent, nobody yet, done),
//! one row each, with the two actions of what needs you on the row itself; the detail of a task (the lifecycle track,
//! the explanation of its state, the report / changes / timeline / request panes and the actions pinned at the bottom)
//! and the new task form. The state and the calls into the core live in `tasks_ui.rs`; here we only paint and *collect*
//! what was clicked ([`Act`]), applied at the end of the frame with the whole `Deck`.

use egui::{Align2, Area, Color32, Id, Key, Margin, Modifiers, Order, Pos2, Rect, Sense, Shape, Stroke, Ui, UiBuilder, Vec2};

use holt_core::capability::Level;
use holt_core::models::{self, Effort};
use holt_core::usage::ModelUsage;
use holt_core::{t, FeedKind, MergeBlock, Task, TaskStatus};

use super::{
    button as ui_button, button_danger, button_width, clickable, frame, galley, galley_spaced, galley_trunc, place, segmented,
    separator as line_sep, shadow, text_in, text_w, tip, ButtonKind,
};
use crate::deck::Deck;
use crate::icons::Icon;
use crate::tasks_ui::{
    self, actions_for, age_short, can_show_diff, clip_text, lanes, lifecycle, mark_of, parse_diff_stat, state_sentence, state_word,
    status_age, status_name, task_date_ddmm, waiting_since, Confirm, DetailTab, FormMenu, HistoryFilter, Lane, Mark, Moment, StopState,
    TaskAct, TasksView,
};
use crate::theme;
use crate::util;

mod detail;
mod form;
mod list;
mod widgets;

use detail::detail_view;
use form::form_view;
pub(crate) use form::MenuItem;
use list::list_view;

const HEAD_H: f32 = theme::TOPBAR_H;
const BTN_H: f32 = 28.0;
/// Buttons on a row of the list.
const ROW_BTN_H: f32 = 24.0;
const CHIP_H: f32 = 21.0;
const PAD: f32 = 12.0;
const LIST_PAD: f32 = 8.0;
const FOOTER_H: f32 = 45.0;
/// The glyph column of a row and the gap after it.
const GLYPH_W: f32 = 18.0;
/// Rows of the "Finished" lane.
const DONE_ROW_H: f32 = 24.0;
/// How many characters of the request and the report go to the screen.
const TEXT_MAX: usize = 20_000;
/// Lines of `git diff --stat` shown in the detail.
const STAT_MAX_LINES: usize = 40;
/// Files named in the "touched outside the scope" notice.
const OUTSIDE_SHOWN: usize = 8;
/// Timeline items shown at once (events and messages, newest first).
const MOMENTS_MAX: usize = 60;

/// What was clicked in this frame.
enum Act {
    New,
    Back,
    Open(String),
    ToggleHistoryMenu,
    CloseHistoryMenu,
    /// Cancels the automatic merge of a task whose conflict is with the agent.
    CancelAutoMerge(String),
    SetHistoryFilter(HistoryFilter),
    SetTab(DetailTab),
    /// Opens or collapses a timeline message (task id, message time).
    ToggleMessage(String, u64),
    Task(String, TaskAct),
    /// "View diff" / "What changed": opens the diff in the center.
    OpenDiff(String),
    /// "Open in Documents" on the request (`false`) or on the report (`true`).
    OpenText(String, bool),
    DismissNotice,
    /// "Stash my changes and merge" in the refusal notice.
    MergeStash(String),
    /// "Ask the agent to commit" / "…resolve the conflict".
    AskAgent(String),
    /// "See what is uncommitted" / "Hide the list".
    ToggleNoticeFiles,
    ConfirmYes,
    ConfirmNo,
    SendReturn(String),
    CancelReturn,
    /// The detail's "⋯" menu.
    ToggleMore,
    CloseMore,
    ToggleMenu(FormMenu),
    CloseMenu,
    SetKind(String),
    SetModel(Option<String>),
    SetEffort(Option<Effort>),
    SetLevel(Level),
    /// Step of the model menu: the provider whose models are on show (`None` = list of providers).
    SetProvider(Option<String>),
    ToggleStartNow,
    ToggleFormMore,
    PickAttachments,
    RemoveAttachment(usize),
    Submit,
}

/// The detail's "⋯" menu is open (ephemeral: closes with the detail).
fn more_id() -> Id {
    Id::new("tasks-more-open")
}

pub fn draw(deck: &mut Deck, ui: &mut Ui, rect: Rect) {
    let mut acts = Vec::new();
    let now = holt_core::util::now_ms();
    match deck.tasks.view.clone() {
        TasksView::List => list_view(deck, ui, rect, now, &mut acts),
        TasksView::Detail(id) => match deck.tasks.task(&id).cloned() {
            Some(task) => detail_view(deck, ui, rect, &task, now, &mut acts),
            None => list_view(deck, ui, rect, now, &mut acts),
        },
        TasksView::New => form_view(deck, ui, rect, &mut acts),
    }
    if !matches!(deck.tasks.view, TasksView::Detail(_)) {
        ui.ctx().data_mut(|d| d.remove::<bool>(more_id()));
    }
    confirm_dialog(deck, ui.ctx(), rect, &mut acts);
    // The times ("5 min") advance by themselves.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(20));
    for act in acts {
        apply(deck, ui.ctx(), act);
    }
}

fn apply(deck: &mut Deck, ctx: &egui::Context, act: Act) {
    match act {
        Act::New => deck.tasks_open_new(),
        Act::Back => deck.tasks_back(),
        Act::Open(id) => deck.tasks_open_detail(&id),
        Act::ToggleHistoryMenu => deck.tasks.history_menu = !deck.tasks.history_menu,
        Act::CloseHistoryMenu => deck.tasks.history_menu = false,
        Act::SetHistoryFilter(f) => {
            deck.tasks.history_filter = f;
            deck.tasks.history_menu = false;
        }
        Act::SetTab(tab) => deck.tasks.detail_tab = Some(tab),
        Act::ToggleMessage(id, at_ms) => {
            if !deck.tasks.open_messages.remove(&(id.clone(), at_ms)) {
                deck.tasks.open_messages.insert((id, at_ms));
            }
        }
        Act::Task(id, act) => deck.tasks_act(&id, act),
        Act::OpenDiff(id) => deck.docs_open_task_diff(&id),
        Act::OpenText(id, report) => deck.docs_open_task_text(&id, report),
        Act::DismissNotice => deck.tasks_clear_notice(),
        Act::MergeStash(id) => deck.tasks_merge_stash(&id),
        Act::AskAgent(id) => deck.tasks_ask_agent(&id),
        Act::CancelAutoMerge(id) => deck.tasks_cancel_auto_merge(&id),
        Act::ToggleNoticeFiles => deck.tasks.notice_files = !deck.tasks.notice_files,
        Act::ConfirmYes => deck.tasks_confirm_accepted(),
        Act::ConfirmNo => deck.tasks.confirm = None,
        Act::SendReturn(id) => deck.tasks_send_return(&id),
        Act::CancelReturn => {
            deck.tasks.return_open = false;
            deck.tasks.feedback.clear();
        }
        Act::ToggleMore => {
            let open = ctx.data(|d| d.get_temp::<bool>(more_id())).unwrap_or(false);
            ctx.data_mut(|d| d.insert_temp(more_id(), !open));
        }
        Act::CloseMore => {
            ctx.data_mut(|d| d.insert_temp(more_id(), false));
        }
        Act::ToggleMenu(m) => {
            let open = deck.tasks.form.menu != Some(m);
            deck.tasks.form.menu = open.then_some(m);
            if open && m == FormMenu::Model {
                // The menu opens on the provider of the chosen model; with no model, on the list of providers.
                deck.tasks.form.provider = deck.tasks.form.model.as_deref().and_then(models::provider).map(str::to_owned);
            }
        }
        Act::CloseMenu => {
            deck.tasks.form.menu = None;
            deck.tasks.form.provider = None;
        }
        Act::SetKind(k) => deck.tasks_form_set_kind(k),
        Act::SetLevel(l) => deck.tasks_form_set_level(l),
        Act::SetProvider(p) => deck.tasks.form.provider = p,
        Act::SetModel(m) => {
            deck.tasks.form.provider = None;
            deck.tasks.form.model = m;
            deck.tasks.form.menu = None;
            deck.tasks_form_refresh();
        }
        Act::SetEffort(e) => {
            deck.tasks.form.effort = e;
            deck.tasks_form_refresh();
        }
        Act::ToggleStartNow => deck.tasks.form.start_now = !deck.tasks.form.start_now,
        Act::ToggleFormMore => deck.tasks.form.more_open = !deck.tasks.form.more_open,
        Act::PickAttachments => deck.tasks_pick_attachments(),
        Act::RemoveAttachment(i) => deck.tasks_remove_attachment(i),
        Act::Submit => deck.tasks_submit_form(),
    }
}

// ---------------------------------------------------------------------------
// Confirmation
// ---------------------------------------------------------------------------

/// Confirmation modal for "Merge" and "Discard", inside the panel, over the dimmed background.
fn confirm_dialog(deck: &mut Deck, ctx: &egui::Context, panel: Rect, acts: &mut Vec<Act>) {
    let Some(confirm) = deck.tasks.confirm.clone() else { return };
    let (id, merge) = match &confirm {
        Confirm::Merge(id) => (id, true),
        Confirm::Discard(id) => (id, false),
    };
    let Some(task) = deck.tasks.task(id).cloned() else {
        deck.tasks.confirm = None;
        return;
    };
    let (title, detail, yes) = if merge {
        (
            t!("Merge \"{title}\" into {base}?", title = task.title, base = task.base),
            t!(
                "Branch {branch} goes into {base} with a merge commit (git merge --no-ff). The agent, the worktree and the branch stay until you test and validate the task.",
                branch = task.branch,
                base = task.base
            ),
            t!("Merge"),
        )
    } else {
        (
            t!("Discard \"{title}\"?", title = task.title),
            if task.status == TaskStatus::Validating {
                t!(
                    "The agent's tab closes and the task's worktree and branch are deleted. What was already merged stays in {base}: discarding does not undo the merge.",
                    base = task.base
                )
            } else {
                t!("The agent's tab closes and the task's worktree and branch are deleted, along with everything the agent did that was not merged. This cannot be undone.").to_owned()
            },
            t!("Discard"),
        )
    };
    super::modal_backdrop(deck, ctx, "tasks-confirm-backdrop");
    let w = (panel.width() - 24.0).clamp(220.0, 360.0);
    let pos = Pos2::new(panel.center().x - w / 2.0, panel.min.y + 70.0);
    Area::new(Id::new("tasks-confirm")).order(Order::Tooltip).fixed_pos(pos).constrain(false).show(ctx, |ui| {
        let p = theme::pal();
        let g1 = ui.painter().layout(title.clone(), theme::ui_semibold(13.0), p.text_strong, w - 28.0);
        let g2 = ui.painter().layout(detail.clone(), theme::ui(12.0), p.text_2, w - 28.0);
        let h = 14.0 + g1.size().y + 6.0 + g2.size().y + 14.0 + BTN_H + 14.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
        shadow(ui.painter(), rect, 10, 8, 24, p.shadow);
        frame(ui.painter(), rect, 10.0, p.popup, p.glass_border);
        let mut y = rect.min.y + 14.0;
        ui.painter().galley(Pos2::new(rect.min.x + 14.0, y), g1.clone(), p.text_strong);
        y += g1.size().y + 6.0;
        ui.painter().galley(Pos2::new(rect.min.x + 14.0, y), g2, p.text_2);
        let by = rect.max.y - 14.0 - BTN_H;
        let dw = button_width(ui.painter(), yes, ButtonKind::Primary);
        let d = Rect::from_min_size(Pos2::new(rect.max.x - 14.0 - dw, by), Vec2::new(dw, BTN_H));
        let cw = button_width(ui.painter(), t!("Cancel"), ButtonKind::Secondary);
        let c = Rect::from_min_size(Pos2::new(d.min.x - 8.0 - cw, by), Vec2::new(cw, BTN_H));
        let dr = if merge {
            ui_button(ui, d, Id::new("tasks-confirm-yes"), yes, ButtonKind::Primary)
        } else {
            button_danger(ui, d, Id::new("tasks-confirm-yes"), yes)
        };
        let cr = ui_button(ui, c, Id::new("tasks-confirm-no"), t!("Cancel"), ButtonKind::Secondary);
        if dr.clicked() {
            acts.push(Act::ConfirmYes);
        } else if cr.clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
            acts.push(Act::ConfirmNo);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use detail::footer_actions;
    use holt_core::{MergeStop, Task, TaskStatus};
    use list::row_actions;

    fn sample_task(status: TaskStatus) -> Task {
        Task { id: "t1".into(), status, started_ms: Some(1000), report: Some("Report".into()), ..Task::default() }
    }

    #[test]
    fn stopped_task_actions_include_merge_and_diff() {
        // Stopped with conflict: row offers "Ask agent to resolve" and diff; footer offers Merge and Return.
        let mut stopped_conflict = sample_task(TaskStatus::Review);
        stopped_conflict.merge_stopped = Some(MergeStop { reason: "conflicted".into(), conflict_files: Some(vec!["conflict.rs".into()]) });

        let row_acts = row_actions(&stopped_conflict);
        assert!(row_acts.iter().any(|(_, act, _, _)| matches!(act, Act::AskAgent(_))));
        assert!(row_acts.iter().any(|(_, act, _, _)| matches!(act, Act::OpenDiff(_))));

        let foot_acts = footer_actions(&stopped_conflict, true);
        assert!(foot_acts.iter().any(|(_, act, _, _)| matches!(act, Act::Task(_, TaskAct::Merge))));
        assert!(foot_acts.iter().any(|(_, act, _, _)| matches!(act, Act::Task(_, TaskAct::Return))));

        // Stopped without conflict (e.g. main repo was dirty): row offers Merge and diff; footer offers Merge and Return.
        let mut stopped_dirty = sample_task(TaskStatus::Review);
        stopped_dirty.merge_stopped = Some(MergeStop { reason: "main repo dirty".into(), conflict_files: None });

        let row_acts_dirty = row_actions(&stopped_dirty);
        assert!(row_acts_dirty.iter().any(|(_, act, _, _)| matches!(act, Act::Task(_, TaskAct::Merge))));
        assert!(row_acts_dirty.iter().any(|(_, act, _, _)| matches!(act, Act::OpenDiff(_))));

        let foot_acts_dirty = footer_actions(&stopped_dirty, true);
        assert!(foot_acts_dirty.iter().any(|(_, act, _, _)| matches!(act, Act::Task(_, TaskAct::Merge))));
        assert!(foot_acts_dirty.iter().any(|(_, act, _, _)| matches!(act, Act::Task(_, TaskAct::Return))));
    }
}
