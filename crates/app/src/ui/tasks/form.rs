//! The new task form: fields, dropdowns and the validation of what was typed.

use super::widgets::*;
use super::*;

// ---------------------------------------------------------------------------
// New task
// ---------------------------------------------------------------------------

/// The segments of the effort selector, in the order of [`effort_of`].
pub(super) fn effort_labels() -> [&'static str; 4] {
    [t!("Default"), t!("Low"), t!("Mid"), t!("High")]
}

/// The segment of an effort (`None` = "Default").
pub(super) fn effort_index(effort: Option<Effort>) -> usize {
    effort.map_or(0, |e| 1 + Effort::ALL.iter().position(|x| *x == e).unwrap_or(0))
}

/// The effort of a segment.
pub(super) fn effort_of(index: usize) -> Option<Effort> {
    index.checked_sub(1).and_then(|i| Effort::ALL.get(i).copied())
}

/// Section/field label of the form (Inter SemiBold 10 px, UPPERCASE, `text_3`, `0.1em`).
pub(super) fn field_label(ui: &mut Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    let color = theme::pal().text_3;
    let g = galley_spaced(ui.painter(), &text.to_uppercase(), theme::label(10.0), color, 10.0 * theme::LABEL_SPACING_EM);
    place(ui.painter(), Rect::from_min_max(Pos2::new(r.min.x, r.min.y + 4.0), r.max), Align2::LEFT_CENTER, g, color);
}

pub(super) fn chip_width(painter: &egui::Painter, reference: &str) -> f32 {
    let t = text_w(painter, &util::chip_label(reference), theme::ui(11.5)).min(220.0);
    10.0 + t + 4.0 + (8.0 + text_w(painter, "×", theme::ui(13.0))) + 4.0 + 2.0
}

/// "Attach files" button and chips of the files attached to the new task's request.
pub(super) fn form_attachments(deck: &mut Deck, ui: &mut Ui, acts: &mut Vec<Act>) {
    let avail = ui.available_width();
    let attachments = &deck.tasks.form.attachments;
    let busy = deck.tasks.form.busy;
    let p = theme::pal();

    if !attachments.is_empty() {
        ui.add_space(6.0);
        let painter = ui.painter().clone();
        let mut i = 0;
        while i < attachments.len() {
            let mut end = i;
            let mut w = 0.0;
            while end < attachments.len() {
                let cw = chip_width(&painter, &attachments[end].to_string_lossy()).min(avail);
                let next = w + cw + if end > i { 6.0 } else { 0.0 };
                if end > i && next > avail {
                    break;
                }
                w = next;
                end += 1;
            }
            let (row, _) = ui.allocate_exact_size(Vec2::new(avail, CHIP_H + 4.0), Sense::hover());
            let mut cx = row.min.x;
            for (idx, a) in attachments.iter().enumerate().take(end).skip(i) {
                let cw = chip_width(&painter, &a.to_string_lossy()).min(avail);
                let r = Rect::from_min_size(Pos2::new(cx, row.min.y + 2.0), Vec2::new(cw, CHIP_H));
                cx = r.max.x + 6.0;
                frame(ui.painter(), r, 5.0, p.recess, p.separator);
                let xw = 8.0 + text_w(ui.painter(), "×", theme::ui(13.0));
                let x_rect = Rect::from_min_size(Pos2::new(r.max.x - 5.0 - xw, r.min.y + 1.0), Vec2::new(xw, r.height() - 2.0));
                let tr = Rect::from_min_max(Pos2::new(r.min.x + 10.0, r.min.y), Pos2::new(x_rect.min.x - 4.0, r.max.y));
                let g = galley_trunc(ui.painter(), &util::chip_label(&a.to_string_lossy()), theme::ui(11.5), p.text_2, tr.width());
                place(ui.painter(), tr, Align2::LEFT_CENTER, g, p.text_2);
                let chip_resp = ui.interact(r, Id::new(("task-chip", idx)), Sense::hover());
                tip(chip_resp, a.display().to_string());
                let xr = if !busy {
                    clickable(ui, x_rect, Id::new(("task-chip-x", idx)))
                } else {
                    ui.interact(x_rect, Id::new(("task-chip-x", idx)), Sense::hover())
                };
                text_in(
                    ui.painter(),
                    x_rect,
                    Align2::CENTER_CENTER,
                    "×",
                    theme::ui(13.0),
                    if xr.hovered() && !busy { p.text } else { p.text_2 },
                );
                if tip(xr, t!("Remove attachment")).clicked() && !busy {
                    acts.push(Act::RemoveAttachment(idx));
                }
            }
            i = end;
        }
    }

    ui.add_space(6.0);
    let label = t!("Attach files");
    let lw = text_w(ui.painter(), label, theme::ui(11.5));
    let bw = 10.0 + 14.0 + 6.0 + lw + 12.0;
    let (row, _) = ui.allocate_exact_size(Vec2::new(avail, 26.0), Sense::hover());
    let btn = Rect::from_min_size(row.min, Vec2::new(bw.min(avail), 26.0));
    let resp =
        if !busy { clickable(ui, btn, Id::new("tasks-new-attach")) } else { ui.interact(btn, Id::new("tasks-new-attach"), Sense::hover()) };
    let hot = !busy && resp.hovered();
    frame(ui.painter(), btn, 5.0, theme::with_alpha(p.text_strong, if hot { 0.08 } else { 0.04 }), theme::with_alpha(p.text_strong, 0.14));
    let fg = if !busy {
        if hot {
            p.text
        } else {
            p.text_2
        }
    } else {
        p.text_3
    };
    deck.icons.paint(ui.painter(), Icon::Attach, Pos2::new(btn.min.x + 16.0, btn.center().y), 13.5, fg, 0.0);
    text_in(
        ui.painter(),
        Rect::from_min_max(Pos2::new(btn.min.x + 28.0, btn.min.y), Pos2::new(btn.max.x - 8.0, btn.max.y)),
        Align2::LEFT_CENTER,
        label,
        theme::ui(11.5),
        fg,
    );
    if tip(resp, t!("Attach files or images to the request (you can also drag and drop onto the form)")).clicked() && !busy {
        acts.push(Act::PickAttachments);
    }
}

/// Field-shaped button that opens a menu: text (with the agent's dot, if any) and "▾".
#[allow(clippy::too_many_arguments)]
pub(super) fn dropdown_button(
    ui: &mut Ui,
    r: Rect,
    id: Id,
    label: &str,
    dot: Option<Color32>,
    enabled: bool,
    open: bool,
    tooltip: &str,
) -> (Rect, bool) {
    let resp = if enabled { clickable(ui, r, id) } else { ui.interact(r, id, Sense::hover()) };
    let p = ui.painter();
    let pal = theme::pal();
    let hot = enabled && (resp.hovered() || open);
    frame(p, r, 6.0, if hot { pal.hover } else { pal.recess }, if open { theme::accent() } else { pal.separator });
    let mut x = r.min.x + 10.0;
    if let Some(c) = dot {
        crate::ui::dot(p, Pos2::new(x + 4.0, r.center().y), c);
        x += 16.0;
    }
    let color = if enabled { pal.text } else { pal.text_3 };
    let g = galley_trunc(p, label, theme::ui(12.0), color, (r.max.x - 28.0 - x).max(10.0));
    place(p, Rect::from_min_max(Pos2::new(x, r.min.y), Pos2::new(r.max.x - 26.0, r.max.y)), Align2::LEFT_CENTER, g, color);
    text_in(p, Rect::from_min_max(Pos2::new(r.max.x - 26.0, r.min.y), r.max), Align2::CENTER_CENTER, "▾", theme::ui(11.5), pal.text_2);
    let clicked = tip(resp, tooltip).clicked() && enabled;
    (r, clicked)
}

/// Quiet mark on the right of a form menu row.
#[derive(Clone, Copy)]
pub(crate) enum Detail {
    /// "free": a model with no per-token cost.
    Free,
    /// "N models": how many the provider has.
    Count(usize),
    /// Current use of the official CLI's quota; `None` means there is no reading yet.
    Quota(Option<u8>),
}

/// Items of a form or filter menu.
#[derive(Clone)]
pub(crate) struct MenuItem {
    pub label: String,
    pub dot: Option<Color32>,
    pub selected: bool,
    pub enabled: bool,
    /// Quiet text on the right: "12 models" on the provider step, "free" on the no-cost models.
    pub detail: Option<Detail>,
    /// Opens another step: draws the "›" arrow on the right.
    pub submenu: bool,
}

/// [`dropdown_popup`] under `anchor`, closing through the tasks' own acts.
pub(super) fn dropdown_menu(
    ctx: &egui::Context,
    id: &str,
    anchor: Rect,
    items: &[MenuItem],
    acts: &mut Vec<Act>,
    is_history_menu: bool,
) -> Option<usize> {
    let (chosen, closed) = dropdown_popup(ctx, id, anchor, items, 0.0, false);
    if closed {
        acts.push(if is_history_menu { Act::CloseHistoryMenu } else { Act::CloseMenu });
    }
    chosen
}

/// Menu next to `anchor` (opaque `popup` background, `glass_border`, corners 8, 26 px items with `hover`), at least
/// `min_w` wide, under the anchor or above it. Returns the chosen index and whether it asks to close (a click outside
/// or Esc, with nothing chosen).
pub(crate) fn dropdown_popup(
    ctx: &egui::Context,
    id: &str,
    anchor: Rect,
    items: &[MenuItem],
    min_w: f32,
    above: bool,
) -> (Option<usize>, bool) {
    let row = 26.0;
    let h = (items.len() as f32 * row).min(260.0) + 8.0;
    let w = anchor.width().max(min_w);
    let y = if above { anchor.min.y - 2.0 - h } else { anchor.max.y + 2.0 };
    let pos = crate::ui::menus::keep_on_screen(ctx, Pos2::new(anchor.min.x, y), Vec2::new(w, h));
    let menu_rect = Rect::from_min_size(pos, Vec2::new(w, h));
    let mut chosen = None;
    let p = theme::pal();
    Area::new(Id::new(id)).order(Order::Foreground).fixed_pos(pos).constrain(false).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        shadow(ui.painter(), rect, 8, 8, 24, p.shadow);
        frame(ui.painter(), rect, 8.0, p.popup, p.glass_border);
        let list = rect.shrink(4.0);
        let mut child = ui.new_child(UiBuilder::new().max_rect(list));
        child.set_clip_rect(list);
        egui::ScrollArea::vertical().id_salt((id, "list")).auto_shrink([false, false]).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for (i, item) in items.iter().enumerate() {
                let (r, _) = ui.allocate_exact_size(Vec2::new(list.width(), row), Sense::hover());
                let resp = if item.enabled { clickable(ui, r, Id::new((id, i))) } else { ui.interact(r, Id::new((id, i)), Sense::hover()) };
                if item.enabled && resp.hovered() {
                    ui.painter().rect_filled(r, 5.0, p.hover);
                }
                let mut x = r.min.x + 10.0;
                if let Some(c) = item.dot {
                    crate::ui::dot(ui.painter(), Pos2::new(x + 4.0, r.center().y), c);
                    x += 16.0;
                }
                let color = if !item.enabled {
                    p.text_3
                } else if item.selected || resp.hovered() {
                    p.text
                } else {
                    p.text_2
                };
                let mut right = r.max.x - 10.0;
                if item.submenu {
                    let ar = Rect::from_min_max(Pos2::new(right - 12.0, r.min.y), Pos2::new(right, r.max.y));
                    text_in(ui.painter(), ar, Align2::CENTER_CENTER, "›", theme::ui(13.0), p.text_2);
                    right -= 16.0;
                }
                if let Some(detail) = &item.detail {
                    let (text, dcolor) = match detail {
                        Detail::Free => (t!("free").to_owned(), p.success),
                        Detail::Count(1) => (t!("1 model").to_owned(), p.text_2),
                        Detail::Count(count) => (t!("{count} models", count), p.text_2),
                        Detail::Quota(Some(used)) => (t!("{used}% used", used), if *used >= 80 { p.warning } else { p.success }),
                        Detail::Quota(None) => (t!("no reading").to_owned(), p.text_3),
                    };
                    let g = galley(ui.painter(), &text, theme::ui(11.0), dcolor);
                    let w = g.size().x;
                    place(
                        ui.painter(),
                        Rect::from_min_max(Pos2::new(right - w, r.min.y), Pos2::new(right, r.max.y)),
                        Align2::RIGHT_CENTER,
                        g,
                        dcolor,
                    );
                    right -= w + 8.0;
                }
                let label = if item.selected { format!("✓ {}", item.label) } else { item.label.clone() };
                let g = galley_trunc(ui.painter(), &label, theme::ui(12.0), color, (right - x).max(10.0));
                place(ui.painter(), Rect::from_min_max(Pos2::new(x, r.min.y), Pos2::new(right, r.max.y)), Align2::LEFT_CENTER, g, color);
                if item.enabled && resp.clicked() {
                    chosen = Some(i);
                }
            }
        });
    });
    let closed =
        chosen.is_none() && (ctx.input(|i| i.key_pressed(Key::Escape)) || crate::ui::menus::closes_on_outside(ctx, menu_rect, anchor));
    (chosen, closed)
}

pub(super) fn form_view(deck: &mut Deck, ui: &mut Ui, rect: Rect, acts: &mut Vec<Act>) {
    let (head, _) = head_bar(ui, rect, None, t!("New task"), false, acts);
    let footer = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - FOOTER_H), rect.max);
    let body = Rect::from_min_max(Pos2::new(rect.min.x, head.max.y), Pos2::new(rect.max.x, footer.min.y));
    let ctx = ui.ctx().clone();

    // What the menus and the button need from the Deck, before the closure borrows the fields.
    let kind = deck.tasks.form.kind.clone();
    let choices = deck.tasks_agent_choices();
    let quota_used = deck.core.quota_used();
    let form_model = deck.tasks.form.model.clone();
    let agent_label = deck.agents.name(&kind);
    let agent_dot = deck.agents.tab_color(&kind);
    let picks_model = crate::ui::menus::picks_model(&kind);
    let (known, loading) = {
        let (m, l) = deck.models_of(&kind);
        (m.to_vec(), l)
    };
    let agent_items: Vec<MenuItem> = choices
        .iter()
        .map(|k| MenuItem {
            label: deck.agents.name(k),
            dot: Some(deck.agents.tab_color(k)),
            selected: *k == kind,
            enabled: true,
            detail: if ["claude", "antigravity", "codex", "opencode"].contains(&k.as_str()) {
                // The selected agent shows the quota of the chosen model's group; the others, the agent's.
                let used =
                    if *k == kind { deck.core.model_usage(k, form_model.as_deref()).map(|m| m.used) } else { quota_used.get(k).copied() };
                Some(Detail::Quota(used.map(|used| (used * 100.0).round().clamp(0.0, 100.0) as u8)))
            } else {
                None
            },
            submenu: false,
        })
        .collect();
    // OpenCode lists every connected provider (`opencode`, `opencode-go`…): the menu
    // asks for the provider first and shows only its models. The other agents have a single list.
    let groups = models::by_provider(&known);
    let sole = models::sole_provider(&groups).map(str::to_owned);
    let step = deck.tasks.form.provider.clone().filter(|p| groups.iter().any(|(name, _)| name == p)).or(sole.clone());
    let model_label = deck.tasks.form.model.clone().unwrap_or_else(|| t!("Default").to_owned());
    // Effort: what the agent (and the chosen model) allows; without it the selector is off, with the tooltip.
    let effort_off = models::effort_unavailable(&kind, deck.tasks.form.model.as_deref());
    let effort_at = if effort_off.is_some() { 0 } else { effort_index(deck.tasks.form.effort) };
    let rows: Vec<(MenuItem, Act)> = if let Some(provider) = &step {
        let mut rows = Vec::new();
        // With a single provider there is no previous step: the menu opens straight on its models.
        if sole.is_none() {
            rows.push((
                MenuItem { label: t!("‹ Providers").into(), dot: None, selected: false, enabled: true, detail: None, submenu: false },
                Act::SetProvider(None),
            ));
        }
        rows.push((
            MenuItem {
                label: t!("Default").into(),
                dot: None,
                selected: deck.tasks.form.model.is_none(),
                enabled: true,
                detail: None,
                submenu: false,
            },
            Act::SetModel(None),
        ));
        if let Some((_, list)) = groups.iter().find(|(name, _)| name == provider) {
            rows.extend(list.iter().map(|m| {
                let item = MenuItem {
                    label: models::short_model(m).to_owned(),
                    dot: None,
                    selected: deck.tasks.form.model.as_deref() == Some(m.as_str()),
                    enabled: true,
                    detail: models::is_free(m).then_some(Detail::Free),
                    submenu: false,
                };
                (item, Act::SetModel(Some(m.clone())))
            }));
        }
        rows
    } else if groups.is_empty() {
        let mut rows = vec![(
            MenuItem {
                label: t!("Default").into(),
                dot: None,
                selected: deck.tasks.form.model.is_none(),
                enabled: true,
                detail: None,
                submenu: false,
            },
            Act::SetModel(None),
        )];
        rows.extend(models::free_first(&known).iter().map(|m| {
            let item = MenuItem {
                label: m.clone(),
                dot: None,
                selected: deck.tasks.form.model.as_deref() == Some(m.as_str()),
                enabled: true,
                detail: models::is_free(m).then_some(Detail::Free),
                submenu: false,
            };
            (item, Act::SetModel(Some(m.clone())))
        }));
        if known.is_empty() {
            let text = if loading { t!("Loading models…") } else { t!("No model list: default only") };
            rows.push((
                MenuItem { label: text.into(), dot: None, selected: false, enabled: false, detail: None, submenu: false },
                Act::CloseMenu,
            ));
        }
        rows
    } else {
        let mut rows = vec![(
            MenuItem {
                label: t!("Default").into(),
                dot: None,
                selected: deck.tasks.form.model.is_none(),
                enabled: true,
                detail: None,
                submenu: false,
            },
            Act::SetModel(None),
        )];
        rows.extend(groups.iter().map(|(provider, list)| {
            let item = MenuItem {
                label: provider.clone(),
                dot: None,
                selected: false,
                enabled: true,
                detail: Some(Detail::Count(list.len())),
                submenu: true,
            };
            (item, Act::SetProvider(Some(provider.clone())))
        }));
        rows
    };
    let model_items: Vec<MenuItem> = rows.iter().map(|(item, _)| item.clone()).collect();
    let open_menu = deck.tasks.form.menu;
    let (title_id, brief_id, base_id) = (Id::new("tasks-new-title"), Id::new("tasks-new-brief"), Id::new("tasks-new-base"));
    let scope_id = Id::new("tasks-new-scope");
    let any_focus = [title_id, brief_id, base_id, scope_id].iter().any(|id| ctx.memory(|m| m.has_focus(*id)));
    let ctrl_enter = any_focus && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
    let focus_title = std::mem::take(&mut deck.tasks.form.focus_title);
    let can_submit = !deck.tasks.form.busy && !deck.tasks.form.title.trim().is_empty() && !choices.is_empty();
    let start_now = deck.tasks.form.start_now;
    let busy = deck.tasks.form.busy;
    let error = deck.tasks.form.error.clone();

    let more_open = deck.tasks.form.more_open;
    let base_label =
        if deck.tasks.form.base.trim().is_empty() { t!("current branch").to_owned() } else { deck.tasks.form.base.trim().to_owned() };
    let scope_count = tasks_ui::parse_scope(&deck.tasks.form.scope).len();
    // The quiet line under the agent: a quota or capacity warning (amber), or how much of the quota is used.
    let advisory = deck.core.quota_advisory(&kind, &choices).map(|note| t!("Quota warning: {note}. The choice is still available.", note));
    let capacity = deck.tasks.form.capacity_note.clone().map(|note| t!("Capacity warning: {note}. The choice is still available.", note));
    // The quota of the selected model's group; an unknown model falls back to the agent number and the hint says so.
    let model_usage = deck.core.model_usage(&kind, deck.tasks.form.model.as_deref());
    let used = model_usage.as_ref().map(|m| (m.used * 100.0).round().clamp(0.0, 100.0) as u8);
    let used_text = |u: u8| match &model_usage {
        // No model chosen: the CLI default, whose group the quota reading does not name.
        Some(m) if m.fallback && deck.tasks.form.model.is_none() => t!("{used}% of the agent's quota used", used = u),
        Some(m) if m.fallback => t!("{used}% of the agent's quota used (no quota group known for this model)", used = u),
        Some(ModelUsage { group: Some(group), .. }) => t!("{used}% of the {group} quota used", used = u, group = group),
        _ => t!("{used}% of the quota used", used = u),
    };
    let agent_hint: Option<(String, Color32)> = match (advisory.or(capacity), used) {
        (Some(w), _) => Some((w, theme::pal().warning)),
        (None, Some(u)) if deck.tasks.form.kind_is_auto => {
            Some((t!("Recommended for this size · {hint}", hint = used_text(u)), theme::pal().text_3))
        }
        (None, Some(u)) => Some((used_text(u), theme::pal().text_3)),
        (None, None) if deck.tasks.form.kind_is_auto => Some((t!("Recommended for this size").to_owned(), theme::pal().text_3)),
        (None, None) => None,
    };

    let mut anchors: [Option<Rect>; 2] = [None, None];
    let mut child = ui.new_child(UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(body);
    egui::ScrollArea::vertical().id_salt("tasks-form").auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(6.0);
        padded(ui, |ui| {
            field_label(ui, t!("Title"));
            text_field(ui, title_id, &mut deck.tasks.form.title, t!("What the agent should do, in one line"), None);
            if focus_title {
                ui.ctx().memory_mut(|m| m.request_focus(title_id));
            }
            ui.add_space(8.0);
            field_label(ui, t!("Request"));
            text_field(ui, brief_id, &mut deck.tasks.form.brief, t!("Details, files, acceptance criteria… (optional)"), Some(6));
            form_attachments(deck, ui, acts);
            ui.add_space(8.0);
            field_label(ui, t!("Size of the work"));
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
            let level_labels = Level::ALL.map(Level::label);
            let level_at = Level::ALL.iter().position(|l| *l == deck.tasks.form.level).unwrap_or(1);
            if let Some(i) = segmented(ui, r, Id::new("tasks-new-level"), &level_labels, level_at, true) {
                acts.push(Act::SetLevel(Level::ALL[i]));
            }
            tip(
                ui.interact(r, Id::new("tasks-new-level-tip"), Sense::hover()),
                t!("The size of the work (not the effort): decides which models can handle it. If you do not choose, Medium applies"),
            );
            ui.add_space(8.0);
            field_label(ui, t!("Who does it"));
            let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
            let half = (row.width() - 8.0) / 2.0;
            let (_, click) = dropdown_button(
                ui,
                Rect::from_min_size(row.min, Vec2::new(half, row.height())),
                Id::new("tasks-new-agent"),
                &agent_label,
                Some(agent_dot),
                !choices.is_empty(),
                open_menu == Some(FormMenu::Agent),
                t!("The agent that will run the task"),
            );
            anchors[0] = Some(row);
            if click {
                acts.push(Act::ToggleMenu(FormMenu::Agent));
            }
            let (_, click) = dropdown_button(
                ui,
                Rect::from_min_max(Pos2::new(row.max.x - half, row.min.y), row.max),
                Id::new("tasks-new-model"),
                &model_label,
                None,
                picks_model,
                open_menu == Some(FormMenu::Model),
                if !picks_model {
                    t!("This agent does not allow choosing the model")
                } else if groups.is_empty() || sole.is_some() {
                    t!("The agent's LLM model")
                } else {
                    t!("Choose the provider and then the model")
                },
            );
            anchors[1] = Some(row);
            if click {
                acts.push(Act::ToggleMenu(FormMenu::Model));
            }
            if let Some((text, color)) = &agent_hint {
                ui.add_space(5.0);
                let g = ui.painter().layout(text.clone(), theme::ui(11.0), *color, ui.available_width());
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), g.size().y), Sense::hover());
                ui.painter().galley(r.min, g, *color);
            }
            ui.add_space(8.0);
            field_label(ui, t!("Effort"));
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
            let picked = ui
                .scope(|ui| {
                    if effort_off.is_some() {
                        ui.set_opacity(0.4);
                    }
                    segmented(ui, r, Id::new("tasks-new-effort"), &effort_labels(), effort_at, true)
                })
                .inner;
            match effort_off {
                Some(why) => {
                    // Covers the selector: no click or hover highlight, only the tooltip.
                    tip(ui.interact(r, Id::new("tasks-new-effort-off"), Sense::hover()), why);
                }
                None => {
                    if let Some(i) = picked {
                        acts.push(Act::SetEffort(effort_of(i)));
                    }
                }
            }
            // "▸ Branch and scope · from dev, no scope": folded, with the current values.
            ui.add_space(12.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
            let rr = clickable(ui, r, Id::new("tasks-new-more"));
            let pal = theme::pal();
            deck.icons.paint(
                ui.painter(),
                Icon::TreeChevron,
                Pos2::new(r.min.x + 5.0, r.center().y),
                10.0,
                pal.text_2,
                if more_open { std::f32::consts::FRAC_PI_2 } else { 0.0 },
            );
            let lr = text_in(
                ui.painter(),
                Rect::from_min_max(Pos2::new(r.min.x + 16.0, r.min.y), r.max),
                Align2::LEFT_CENTER,
                t!("Branch and scope"),
                theme::ui(11.5),
                if rr.hovered() { pal.text } else { pal.text_2 },
            );
            let summary = match scope_count {
                0 => t!("from {base}, no scope", base = base_label),
                1 => t!("from {base}, 1 path", base = base_label),
                n => t!("from {base}, {n} paths", base = base_label, n),
            };
            let g = galley_trunc(ui.painter(), &format!("· {summary}"), theme::ui(11.5), pal.text_3, (r.max.x - lr.max.x - 6.0).max(10.0));
            place(ui.painter(), Rect::from_min_max(Pos2::new(lr.max.x + 6.0, r.min.y), r.max), Align2::LEFT_CENTER, g, pal.text_3);
            if tip(rr, t!("The branch the task starts from and the files it will touch")).clicked() {
                acts.push(Act::ToggleFormMore);
            }
            if more_open {
                ui.add_space(2.0);
                field_label(ui, t!("Base"));
                text_field(ui, base_id, &mut deck.tasks.form.base, t!("current branch"), None);
                ui.add_space(8.0);
                field_label(ui, t!("Scope"));
                text_field(
                    ui,
                    scope_id,
                    &mut deck.tasks.form.scope,
                    t!("Files or folders it will touch, one per line (optional)"),
                    Some(3),
                );
            }
            ui.add_space(10.0);
            // "Start now".
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
            let rr = clickable(ui, r, Id::new("tasks-new-start"));
            let cb = Rect::from_min_size(Pos2::new(r.min.x + 1.0, r.center().y - 7.0), Vec2::splat(14.0));
            ui.painter().rect(
                cb,
                3.0,
                if start_now { theme::accent() } else { theme::pal().recess },
                Stroke::new(1.0, if start_now { theme::accent() } else { theme::pal().separator }),
                egui::StrokeKind::Inside,
            );
            if start_now {
                text_in(ui.painter(), cb, Align2::CENTER_CENTER, "✓", theme::ui_bold(11.0), theme::pal().on_accent);
            }
            text_in(
                ui.painter(),
                Rect::from_min_max(Pos2::new(cb.max.x + 8.0, r.min.y), r.max),
                Align2::LEFT_CENTER,
                t!("Start now"),
                theme::ui(12.0),
                if rr.hovered() { theme::pal().text } else { theme::pal().text_2 },
            );
            if tip(rr, t!("Creates the worktree and opens the agent right away. Off, the task stays a draft to start later")).clicked() {
                acts.push(Act::ToggleStartNow);
            }
        });
        if let Some(e) = &error {
            ui.add_space(8.0);
            error_box(ui, e, false);
        }
        ui.add_space(14.0);
    });

    // Pinned footer: Create, Cancel and the shortcut.
    let painter = ui.painter_at(footer);
    line_sep(&painter, Pos2::new(footer.min.x, footer.min.y + 0.5), Pos2::new(footer.max.x, footer.min.y + 0.5));
    let label = if busy {
        t!("Creating…")
    } else if start_now {
        t!("Create and start")
    } else {
        t!("Create draft")
    };
    let by = footer.min.y + 8.5;
    let cw = btn_width(&painter, label, BtnKind::Primary);
    let create = Rect::from_min_size(Pos2::new(footer.min.x + LIST_PAD, by), Vec2::new(cw, BTN_H));
    let create_tip = if can_submit { t!("Creates the task (Ctrl+Enter)") } else { t!("Write the task title") };
    let clicked = button(ui, create, Id::new("tasks-create"), label, BtnKind::Primary, can_submit, create_tip);
    if clicked || (ctrl_enter && can_submit) {
        acts.push(Act::Submit);
    }
    let xw = btn_width(&painter, t!("Cancel"), BtnKind::Secondary);
    let cancel = Rect::from_min_size(Pos2::new(create.max.x + 6.0, by), Vec2::new(xw, BTN_H));
    if button(ui, cancel, Id::new("tasks-cancel"), t!("Cancel"), BtnKind::Secondary, !busy, t!("Goes back to the list without creating")) {
        acts.push(Act::Back);
    }
    let hint = galley(&painter, "Ctrl+Enter", theme::meta(10.5), theme::pal().text_3);
    if hint.size().x + 12.0 < footer.max.x - LIST_PAD - cancel.max.x {
        place(
            &painter,
            Rect::from_min_max(Pos2::new(cancel.max.x, by), Pos2::new(footer.max.x - PAD, by + BTN_H)),
            Align2::RIGHT_CENTER,
            hint,
            theme::pal().text_3,
        );
    }

    match (open_menu, anchors) {
        (Some(FormMenu::Agent), [Some(a), _]) => {
            if let Some(i) = dropdown_menu(&ctx, "tasks-menu-agent", a, &agent_items, acts, false) {
                acts.push(Act::SetKind(choices[i].clone()));
            }
        }
        (Some(FormMenu::Model), [_, Some(a)]) => {
            if let Some(i) = dropdown_menu(&ctx, "tasks-menu-model", a, &model_items, acts, false) {
                if let Some((_, act)) = rows.into_iter().nth(i) {
                    acts.push(act);
                }
            }
        }
        _ => {}
    }
}
