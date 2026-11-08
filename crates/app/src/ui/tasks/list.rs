//! The queue: the lanes, the rows and their inline actions.

use super::form::dropdown_menu;
use super::widgets::*;
use super::*;

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

pub(super) fn list_view(deck: &mut Deck, ui: &mut Ui, rect: Rect, now: u64, acts: &mut Vec<Act>) {
    // Fixed footer at the bottom: separator + primary "+ New task" button, full width.
    let footer = Rect::from_min_max(Pos2::new(rect.min.x, (rect.max.y - FOOTER_H).max(rect.min.y)), rect.max);
    let body = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, footer.min.y));

    if deck.tasks.tasks.is_empty() {
        // With no open project nobody reads the list: it does not stay on "Reading the tasks…" forever.
        empty_state(ui, body, deck.tasks.loaded || deck.active_project.is_empty());
        draw_footer(ui, footer, acts);
        return;
    }

    let tasks = deck.tasks.tasks.clone();
    let filter = deck.tasks.history_filter;
    let lanes = lanes(&tasks, filter);
    // The oldest task waiting for you gets the accent frame: it is the next one to handle.
    let next_up = lanes.iter().find(|(l, _)| *l == Lane::NeedsYou).and_then(|(_, list)| list.first()).map(|t| t.id.clone());
    let mut filter_anchor = None;

    let mut child = ui.new_child(UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(body);
    egui::ScrollArea::vertical().id_salt("tasks-list").auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(2.0);
        for (lane, list) in &lanes {
            let count = list.len();
            if let Some(f) = lane_header(deck, ui, *lane, count, acts) {
                filter_anchor = Some(f);
            }
            if *lane == Lane::Finished {
                if list.is_empty() {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), DONE_ROW_H), Sense::hover());
                    text_in(
                        ui.painter(),
                        Rect::from_min_max(Pos2::new(r.min.x + PAD, r.min.y), r.max),
                        Align2::LEFT_CENTER,
                        t!("No tasks in this filter."),
                        theme::ui(11.5),
                        theme::pal().text_3,
                    );
                }
                for task in list {
                    done_row(deck, ui, task, acts);
                }
            } else {
                for (i, task) in list.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(2.0);
                    }
                    task_row(deck, ui, task, next_up.as_deref() == Some(task.id.as_str()), now, acts);
                }
            }
        }
        ui.add_space(LIST_PAD + 2.0);
    });

    draw_footer(ui, footer, acts);

    // Drop-down menu of the "Finished" filter.
    if deck.tasks.history_menu {
        if let Some(anchor) = filter_anchor {
            let items: Vec<MenuItem> = HistoryFilter::ALL
                .iter()
                .map(|&f| MenuItem {
                    label: f.label().to_owned(),
                    dot: None,
                    selected: f == filter,
                    enabled: true,
                    detail: None,
                    submenu: false,
                })
                .collect();
            let menu_w = 186.0_f32.max(anchor.width());
            let menu_anchor = Rect::from_min_size(Pos2::new(anchor.max.x - menu_w, anchor.min.y), Vec2::new(menu_w, anchor.height()));
            if let Some(i) = dropdown_menu(ui.ctx(), "tasks-history-filter-menu", menu_anchor, &items, acts, true) {
                acts.push(Act::SetHistoryFilter(HistoryFilter::ALL[i]));
            }
        }
    }
}

pub(super) fn draw_footer(ui: &Ui, footer: Rect, acts: &mut Vec<Act>) {
    let painter = ui.painter_at(footer);
    line_sep(&painter, Pos2::new(footer.min.x, footer.min.y + 0.5), Pos2::new(footer.max.x, footer.min.y + 0.5));
    let btn_rect =
        Rect::from_min_size(Pos2::new(footer.min.x + LIST_PAD, footer.min.y + 8.5), Vec2::new(footer.width() - 2.0 * LIST_PAD, BTN_H));
    if tip(ui_button(ui, btn_rect, Id::new("tasks-new"), t!("+ New task"), ButtonKind::Primary), t!("Create a task for an agent")).clicked()
    {
        acts.push(Act::New);
    }
}

pub(super) fn empty_state(ui: &mut Ui, body: Rect, loaded: bool) {
    let painter = ui.painter_at(body);
    let w = (body.width() - 2.0 * PAD - 8.0).max(80.0);
    let mut y = body.min.y + 24.0;
    let x = body.min.x + PAD + 4.0;
    if !loaded {
        text_in(
            &painter,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 20.0)),
            Align2::LEFT_CENTER,
            t!("Reading the tasks…"),
            theme::ui(12.0),
            theme::pal().text_2,
        );
        return;
    }
    let title = painter.layout(t!("No tasks yet").to_owned(), theme::ui_semibold(13.0), theme::pal().text_strong, w);
    painter.galley(Pos2::new(x, y), title.clone(), theme::pal().text_strong);
    y += title.size().y + 6.0;
    let text = t!(
        "A task is a request that an agent carries out on an isolated copy of the project (a branch with its own folder). \
When it finishes, you review the report and the diff, merge, test and validate. If something fails, you send it back or reject it with the reason."
    );
    let g = painter.layout(text.to_owned(), theme::ui(12.0), theme::pal().text_2, w);
    painter.galley(Pos2::new(x, y), g, theme::pal().text_2);
}

/// The note under a row and its colour: why a merge stopped, the failure, a conflict being resolved.
pub(super) fn row_note(task: &Task) -> Option<(String, Color32)> {
    let p = theme::pal();
    match mark_of(task) {
        Mark::Stopped => task.merge_stopped.as_ref().map(|s| (s.reason.clone(), p.warning)),
        Mark::Failed => task.error.clone().map(|e| (e, p.danger_text)),
        Mark::Conflict => Some((t!("Merges into {base} by itself when the agent delivers.", base = task.base), p.text_2)),
        _ => None,
    }
}

/// The actions on a row that needs you: the main move and, at most, one quiet look (the diff). What needs typing (Return,
/// Reject) or is rarely wanted from the list (Discard) lives in the detail, so a row never becomes a wall of buttons.
pub(super) fn row_actions(task: &Task) -> Vec<(String, Act, ButtonKind, &'static str)> {
    let id = || task.id.clone();
    let diff = |label: &str| (label.to_owned(), Act::OpenDiff(id()), ButtonKind::Ghost, tasks_ui::diff_button_tip(task));
    let mut v = match mark_of(task) {
        Mark::Review => vec![(TaskAct::Merge.label(task), Act::Task(id(), TaskAct::Merge), ButtonKind::Primary, TaskAct::Merge.tip())],
        Mark::Validate => {
            vec![(t!("Validate").to_owned(), Act::Task(id(), TaskAct::Validate), ButtonKind::Primary, TaskAct::Validate.tip())]
        }
        Mark::Stopped if task.merge_stopped.as_ref().is_some_and(|s| s.conflict_files.is_some()) => vec![(
            t!("Ask agent to resolve").to_owned(),
            Act::AskAgent(id()),
            ButtonKind::Primary,
            t!("Sends the task back to the agent asking it to merge the base into its branch (git merge) and resolve the conflicting files"),
        )],
        Mark::Stopped => vec![(TaskAct::Merge.label(task), Act::Task(id(), TaskAct::Merge), ButtonKind::Primary, TaskAct::Merge.tip())],
        Mark::Failed => vec![(TaskAct::Retry.label(task), Act::Task(id(), TaskAct::Retry), ButtonKind::Primary, TaskAct::Retry.tip())],
        _ => Vec::new(),
    };
    if matches!(mark_of(task), Mark::Review | Mark::Validate | Mark::Stopped) && can_show_diff(task) {
        v.push(diff(if task.status == TaskStatus::Validating { t!("What changed") } else { t!("Diff") }));
    }
    v
}

/// A row of the queue:
/// - line 1: the state glyph, the title (12.5 px 600, up to 2 lines) and how long it waits (10.5 px, `text_3`);
/// - line 2: the state word in its colour, the agent's logo with the model (and effort), the id, and the diff on the right;
/// - optional note (why the merge stopped, the failure, the conflict being resolved, or the agent's last move);
/// - for what needs you, its actions (24 px).
pub(super) fn task_row(deck: &mut Deck, ui: &mut Ui, task: &Task, next_up: bool, now: u64, acts: &mut Vec<Act>) {
    let p = theme::pal();
    let mark = mark_of(task);
    let needs_you = Lane::of(task.status) == Lane::NeedsYou;
    let busy = deck.tasks.busy.as_ref().filter(|b| b.id == task.id).map(|b| b.label);
    let working = deck.tasks.busy.is_some();
    let avail_w = ui.available_width();
    let row_w = (avail_w - 2.0 * LIST_PAD).max(120.0);
    let x_text = LIST_PAD + 6.0 + GLYPH_W + 8.0;
    let inner_w = (avail_w - x_text - LIST_PAD - 8.0).max(60.0);
    let painter = ui.painter().clone();

    let age = age_short(now, waiting_since(task));
    let age_g = galley(&painter, &age, theme::ui(10.5), p.text_3);
    let mut title_job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: (inner_w - age_g.size().x - 8.0).max(40.0),
            max_rows: 2,
            break_anywhere: false,
            overflow_character: Some('…'),
        },
        ..Default::default()
    };
    title_job.append(
        &task.title,
        0.0,
        egui::TextFormat { font_id: theme::ui_semibold(12.5), color: p.text, line_height: Some(16.0), ..Default::default() },
    );
    let title_g = painter.layout_job(title_job);
    let title_h = title_g.size().y.max(16.0);

    let note = row_note(task).or_else(|| {
        (mark == Mark::Running)
            .then(|| tasks_ui::last_move(task))
            .flatten()
            .map(|(text, at)| (t!("Last: {what} · {age}", what = text, age = age_short(now, at)), p.text_3))
    });
    let note_g = note.as_ref().map(|(text, color)| {
        let mut job = egui::text::LayoutJob {
            wrap: egui::text::TextWrapping { max_width: inner_w, max_rows: 2, break_anywhere: false, overflow_character: Some('…') },
            ..Default::default()
        };
        job.append(text, 0.0, egui::TextFormat { font_id: theme::ui(11.5), color: *color, ..Default::default() });
        painter.layout_job(job)
    });
    let conflict_link = mark == Mark::Conflict;
    let actions = if needs_you { row_actions(task) } else { Vec::new() };

    let meta_h = 15.0;
    let note_h = note_g.as_ref().map_or(0.0, |g| 5.0 + g.size().y) + if conflict_link { 17.0 } else { 0.0 };
    let acts_h = if actions.is_empty() { 0.0 } else { 8.0 + ROW_BTN_H };
    let h = 8.0 + title_h + 3.0 + meta_h + note_h + acts_h + 8.0;

    let (outer, _) = ui.allocate_exact_size(Vec2::new(avail_w, h), Sense::hover());
    let rect = Rect::from_min_size(Pos2::new(outer.min.x + LIST_PAD, outer.min.y), Vec2::new(row_w, h));
    let resp = clickable(ui, rect, Id::new(("task-row", &task.id)));
    let hot = resp.hovered();
    if next_up {
        frame(&painter, rect, 7.0, theme::accent_wash(theme::accent()), theme::accent_edge(theme::accent()));
    } else if hot {
        painter.rect_filled(rect, 7.0, p.hover);
    } else if needs_you {
        painter.rect_filled(rect, 7.0, theme::with_alpha(p.text_strong, 0.035));
    }

    let x0 = outer.min.x + x_text;
    let right = rect.max.x - 8.0;
    let y1 = rect.min.y + 8.0;
    paint_mark(ui, Pos2::new(rect.min.x + 6.0 + GLYPH_W / 2.0, y1 + 8.0), mark, deck.agents.tab_color(&task.kind));
    painter.galley(Pos2::new(x0, y1), title_g, p.text);
    place(&painter, Rect::from_min_max(Pos2::new(x0, y1), Pos2::new(right, y1 + 16.0)), Align2::RIGHT_CENTER, age_g, p.text_3);

    // Line 2: state · logo model · id ……… diff
    let y2 = y1 + title_h + 3.0;
    let line = Rect::from_min_max(Pos2::new(x0, y2), Pos2::new(right, y2 + meta_h));
    let mut diff_left = right;
    if let Some(sum) = task.diff_stat.as_deref().and_then(parse_diff_stat).filter(|s| s.files > 0) {
        if mark != Mark::Validate {
            diff_left = paint_diff_counts(&painter, line, sum.insertions, sum.deletions) - 8.0;
        }
    }
    if mark == Mark::Validate {
        if let Some(h) = tasks_ui::merge_short(task) {
            let text = t!("in {base} · {hash}", base = task.base, hash = h);
            let g = galley(&painter, &text, theme::meta(10.5), p.text_3);
            diff_left = place(&painter, line, Align2::RIGHT_CENTER, g, p.text_3).min.x - 8.0;
        }
    }
    let state = busy.unwrap_or_else(|| state_word(task));
    let state_color = if busy.is_some() { theme::accent() } else { mark_color(mark) };
    let g = galley_trunc(&painter, state, theme::ui_semibold(11.0), state_color, (diff_left - x0).max(10.0));
    let mut x = place(&painter, Rect::from_min_max(line.min, Pos2::new(diff_left, line.max.y)), Align2::LEFT_CENTER, g, state_color).max.x;
    if let (TaskStatus::Queued, Some(holder)) = (task.status, task.waiting_for.as_deref()) {
        let text = t!("after {holder}", holder);
        let g = galley_trunc(&painter, &text, theme::ui(11.0), p.text_3, (diff_left - x - 6.0).max(0.0));
        x = place(
            &painter,
            Rect::from_min_max(Pos2::new(x + 6.0, line.min.y), Pos2::new(diff_left, line.max.y)),
            Align2::LEFT_CENTER,
            g,
            p.text_3,
        )
        .max
        .x;
    }
    if diff_left - x > 40.0 {
        paint_agent(deck, &painter, Pos2::new(x + 8.0 + 6.0, line.center().y), &task.kind);
        x += 8.0 + 12.0 + 5.0;
        let model = task.model.as_deref().map(crate::ui::projects::model_tag).unwrap_or_else(|| t!("default model").to_owned());
        let who = match tasks_ui::effort_text(task) {
            Some(e) => format!("{model} · {e}"),
            None => model,
        };
        let g = galley_trunc(&painter, &who, theme::ui(11.0), p.text_2, (diff_left - x).max(0.0));
        x = place(
            &painter,
            Rect::from_min_max(Pos2::new(x, line.min.y), Pos2::new(diff_left, line.max.y)),
            Align2::LEFT_CENTER,
            g,
            p.text_2,
        )
        .max
        .x;
        if tasks_ui::Lane::of(task.status) == tasks_ui::Lane::Working {
            if let Some(status) = deck.task_tab_status(task, std::time::Instant::now()) {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(60));
                let id_w = galley(&painter, &task.id, theme::meta(10.5), p.text_3).size().x + 6.0;
                let avail = (diff_left - id_w - x - 6.0).max(0.0);
                if avail >= 24.0 {
                    let sg = galley_trunc(&painter, &status, theme::meta(10.0), p.text_3, avail);
                    x = place(
                        &painter,
                        Rect::from_min_max(Pos2::new(x + 6.0, line.min.y), Pos2::new(diff_left, line.max.y)),
                        Align2::LEFT_CENTER,
                        sg,
                        p.text_3,
                    )
                    .max
                    .x;
                }
            }
        }
    }
    if diff_left - x > 24.0 {
        let g = galley(&painter, &task.id, theme::meta(10.5), p.text_3);
        if g.size().x + 6.0 <= diff_left - x {
            place(
                &painter,
                Rect::from_min_max(Pos2::new(x + 6.0, line.min.y), Pos2::new(diff_left, line.max.y)),
                Align2::LEFT_CENTER,
                g,
                p.text_3,
            );
        }
    }

    // Note and the conflict's link.
    let mut y = y2 + meta_h;
    let mut clicked_inside = false;
    if let (Some(g), Some((_, color))) = (note_g, note.as_ref()) {
        y += 5.0;
        let gh = g.size().y;
        painter.galley(Pos2::new(x0, y), g, *color);
        y += gh;
    }
    if conflict_link {
        let label = t!("Cancel automatic merge");
        let w = text_w(&painter, label, theme::ui_medium(11.5));
        let link = Rect::from_min_size(Pos2::new(x0, y + 2.0), Vec2::new(w, 15.0));
        let lr = if working {
            ui.interact(link, Id::new(("row-cancel-auto", &task.id)), Sense::hover())
        } else {
            clickable(ui, link, Id::new(("row-cancel-auto", &task.id)))
        };
        let color = if lr.hovered() && !working { p.text_strong } else { p.text };
        text_in(&painter, link, Align2::LEFT_CENTER, label, theme::ui_medium(11.5), color);
        painter.line_segment(
            [Pos2::new(link.min.x, link.max.y - 1.0), Pos2::new(link.max.x, link.max.y - 1.0)],
            Stroke::new(1.0, theme::with_alpha(color, 0.35)),
        );
        if tip(lr, t!("The task will wait in review when the agent delivers, and you merge it yourself")).clicked() && !working {
            acts.push(Act::CancelAutoMerge(task.id.clone()));
            clicked_inside = true;
        }
        y += 17.0;
    }

    // Actions.
    if !actions.is_empty() {
        let by = y + 8.0;
        let mut bx = x0;
        for (i, (label, act, kind, tooltip)) in actions.into_iter().enumerate() {
            let w = button_width(&painter, &label, kind);
            if i > 0 && bx + w > right + 2.0 {
                break;
            }
            let r = Rect::from_min_size(Pos2::new(bx, by), Vec2::new(w, ROW_BTN_H));
            bx = r.max.x + 6.0;
            let id = Id::new(("row-act", &task.id, i));
            if working {
                // While an operation runs the row's actions are off (dimmed, no click).
                ui.interact(r, id, Sense::hover());
                frame(&painter, r, 5.0, theme::with_alpha(p.text_strong, 0.02), p.separator);
                text_in(&painter, r, Align2::CENTER_CENTER, &label, theme::ui(11.5), p.text_3);
                continue;
            }
            if tip(ui_button(ui, r, id, &label, kind), tooltip).clicked() {
                acts.push(act);
                clicked_inside = true;
            }
        }
    }

    let tooltip = format!("{} · {}\n{} → {}", task.id, task.title, task.branch, task.base);
    if !clicked_inside && tip(resp, tooltip).clicked() {
        acts.push(Act::Open(task.id.clone()));
    }
}

/// A 24 px row of "Finished": the glyph (check or faint ring), the id, the title (struck through when discarded) and
/// the `dd/mm` date.
pub(super) fn done_row(deck: &mut Deck, ui: &mut Ui, task: &Task, acts: &mut Vec<Act>) {
    let p = theme::pal();
    let avail_w = ui.available_width();
    let (outer, _) = ui.allocate_exact_size(Vec2::new(avail_w, DONE_ROW_H), Sense::hover());
    let row = outer.shrink2(Vec2::new(LIST_PAD, 0.0));
    let resp = clickable(ui, row, Id::new(("task-hist", &task.id)));
    if resp.hovered() {
        ui.painter().rect_filled(row, 5.0, p.hover);
    }
    let painter = ui.painter().clone();
    paint_mark(ui, Pos2::new(row.min.x + 6.0 + GLYPH_W / 2.0, row.center().y), mark_of(task), deck.agents.tab_color(&task.kind));
    let x_text = outer.min.x + LIST_PAD + 6.0 + GLYPH_W + 8.0;
    let id_g = galley(&painter, &task.id, theme::meta(10.5), p.text_3);
    let id_r = place(&painter, Rect::from_min_max(Pos2::new(x_text, row.min.y), row.max), Align2::LEFT_CENTER, id_g, p.text_3);
    let date_g = galley(&painter, &task_date_ddmm(task), theme::meta(10.0), p.text_3);
    let date_r =
        place(&painter, Rect::from_min_max(row.min, Pos2::new(row.max.x - 8.0, row.max.y)), Align2::RIGHT_CENTER, date_g, p.text_3);
    let title_x0 = id_r.max.x.max(x_text + 24.0) + 6.0;
    let title_w = (date_r.min.x - 8.0 - title_x0).max(20.0);
    let discarded = task.status == TaskStatus::Discarded;
    let title_color = if discarded { p.text_3 } else { p.text };
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping { max_width: title_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') },
        ..Default::default()
    };
    job.append(
        &task.title,
        0.0,
        egui::TextFormat {
            font_id: theme::ui(11.5),
            color: title_color,
            strikethrough: if discarded { Stroke::new(1.0, p.text_3) } else { Stroke::NONE },
            ..Default::default()
        },
    );
    let title_g = painter.layout_job(job);
    place(
        &painter,
        Rect::from_min_size(Pos2::new(title_x0, row.min.y), Vec2::new(title_w, DONE_ROW_H)),
        Align2::LEFT_CENTER,
        title_g,
        title_color,
    );
    let tooltip = format!("{} · {}\n{}", task.id, task.title, status_name(task.status));
    if tip(resp, tooltip).clicked() {
        acts.push(Act::Open(task.id.clone()));
    }
}
