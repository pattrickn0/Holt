//! Shared pieces of the tab: marks, buttons, fields, boxes and section helpers.

use super::*;

// ---------------------------------------------------------------------------
// Common pieces
// ---------------------------------------------------------------------------

/// The colour of a state (the word on the row, the detail's sentence and the track): the accent for what waits for
/// you, amber for a conflict or a stopped merge, red for a failure, grey for the rest.
pub(super) fn mark_color(mark: Mark) -> Color32 {
    let p = theme::pal();
    match mark {
        Mark::Review | Mark::Validate => theme::accent(),
        Mark::Stopped | Mark::Conflict => p.warning,
        Mark::Failed => p.danger_text,
        Mark::Running | Mark::Queued | Mark::Draft => p.text_2,
        Mark::Done => p.success,
        Mark::Discarded => p.text_3,
    }
}

/// The 16 px state glyph centered on `c`: filled ring (review), ring with a check (test it), amber or red "!" (merge
/// stopped, failed), pulsing dot in the agent's colour (working) or amber (resolving a conflict), dashed ring (queued),
/// hollow ring (draft), green check (completed), faint ring (discarded).
pub(super) fn paint_mark(ui: &Ui, c: Pos2, mark: Mark, agent: Color32) {
    let p = theme::pal();
    let painter = ui.painter();
    let accent = theme::accent();
    let check = |color: Color32, s: f32| {
        let pts = vec![c + Vec2::new(-2.6 * s, 0.2 * s), c + Vec2::new(-0.7 * s, 2.0 * s), c + Vec2::new(2.8 * s, -2.0 * s)];
        painter.add(Shape::line(pts, Stroke::new(1.6, color)));
    };
    let bang = |fill: Color32, fg: Color32| {
        painter.circle_filled(c, 7.0, fill);
        painter.line_segment([c + Vec2::new(0.0, -3.4), c + Vec2::new(0.0, 1.0)], Stroke::new(1.7, fg));
        painter.circle_filled(c + Vec2::new(0.0, 3.4), 1.0, fg);
    };
    let pulse = |color: Color32| {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        let wave = (ui.ctx().input(|i| i.time) * 4.8).sin() as f32 * 0.5 + 0.5;
        painter.circle_filled(c, 6.0 + 2.5 * wave, theme::with_opacity(color, 0.30 * (1.0 - wave)));
        painter.circle_filled(c, 4.0, color);
    };
    match mark {
        Mark::Review => {
            painter.circle_stroke(c, 7.0, Stroke::new(1.5, accent));
            painter.circle_filled(c, 3.6, accent);
        }
        Mark::Validate => {
            painter.circle_stroke(c, 7.0, Stroke::new(1.5, accent));
            check(accent, 1.0);
        }
        Mark::Stopped => bang(p.warning, p.on_accent),
        Mark::Failed => bang(p.danger, p.text_strong),
        Mark::Running => pulse(agent),
        Mark::Conflict => pulse(p.warning),
        Mark::Queued => {
            // Dashed ring: 10 arcs with gaps.
            for i in 0..10 {
                let a0 = i as f32 / 10.0 * std::f32::consts::TAU;
                let a1 = a0 + std::f32::consts::TAU / 20.0;
                let pt = |a: f32| c + Vec2::new(a.cos(), a.sin()) * 6.4;
                painter.line_segment([pt(a0), pt(a1)], Stroke::new(1.5, p.text_3));
            }
        }
        Mark::Draft => {
            painter.circle_stroke(c, 5.8, Stroke::new(1.5, p.text_faint));
        }
        Mark::Done => check(p.success, 1.2),
        Mark::Discarded => {
            painter.circle_stroke(c, 5.0, Stroke::new(1.3, p.text_faint));
            painter.line_segment([c + Vec2::new(-4.0, 4.0), c + Vec2::new(4.0, -4.0)], Stroke::new(1.3, p.text_faint));
        }
    }
}

/// The agent's mark (model logo, or the dot) at 12 px.
pub(super) fn paint_agent(deck: &mut Deck, painter: &egui::Painter, c: Pos2, kind: &str) {
    let color = deck.agents.tab_color(kind);
    match crate::ui::projects::agent_icon(kind) {
        Some(icon) => deck.icons.paint(painter, icon, c, 12.0, color, 0.0),
        None => {
            painter.circle_filled(c, 3.0, color);
        }
    }
}

/// `+120 −8` in green and red (Inter 10.5), right-aligned in `rect`. Returns the left edge.
pub(super) fn paint_diff_counts(painter: &egui::Painter, rect: Rect, plus: u32, minus: u32) -> f32 {
    let p = theme::pal();
    let mut x = rect.max.x;
    let font = theme::meta(10.5);
    for (text, color) in [(format!("−{minus}"), p.danger_text), (format!("+{plus}"), p.success)] {
        if text.ends_with('0') && text.len() == 2 {
            continue;
        }
        let g = galley(painter, &text, font.clone(), color);
        let w = g.size().x;
        place(painter, Rect::from_min_max(Pos2::new(x - w, rect.min.y), Pos2::new(x, rect.max.y)), Align2::LEFT_CENTER, g, color);
        x -= w + 5.0;
    }
    x + 5.0
}

/// Top strip of the detail and of the form: bottom border, "←", a quiet prefix (the task id) and the title; `more`
/// reserves the "⋯" button on the right and returns its rectangle.
pub(super) fn head_bar(ui: &Ui, rect: Rect, prefix: Option<&str>, title: &str, more: bool, acts: &mut Vec<Act>) -> (Rect, Option<Rect>) {
    let head = Rect::from_min_size(rect.min, Vec2::new(rect.width(), HEAD_H));
    let p = ui.painter_at(head);
    let pal = theme::pal();
    line_sep(&p, Pos2::new(head.min.x, head.max.y - 0.5), Pos2::new(head.max.x, head.max.y - 0.5));
    let b = Rect::from_min_size(Pos2::new(head.min.x + 6.0, head.center().y - 11.0), Vec2::new(24.0, 22.0));
    let r = clickable(ui, b, Id::new("tasks-back"));
    if r.hovered() {
        p.rect_filled(b, 5.0, pal.hover);
    }
    text_in(&p, b, Align2::CENTER_CENTER, "←", theme::ui(14.0), if r.hovered() { pal.text } else { pal.text_2 });
    if tip(r, t!("Back to the task list")).clicked() {
        acts.push(Act::Back);
    }
    let mut x = b.max.x + 6.0;
    let more_rect = more.then(|| Rect::from_min_size(Pos2::new(head.max.x - 6.0 - 24.0, head.center().y - 11.0), Vec2::new(24.0, 22.0)));
    let right = more_rect.map_or(head.max.x - PAD, |m| m.min.x - 6.0);
    if let Some(prefix) = prefix {
        let g = galley(&p, prefix, theme::meta(11.0), pal.text_3);
        x = place(&p, Rect::from_min_max(Pos2::new(x, head.min.y), Pos2::new(right, head.max.y - 1.0)), Align2::LEFT_CENTER, g, pal.text_3)
            .max
            .x
            + 7.0;
    }
    let g = galley_trunc(&p, title, theme::ui_semibold(12.5), pal.text_strong, (right - x).max(20.0));
    place(&p, Rect::from_min_max(Pos2::new(x, head.min.y), Pos2::new(right, head.max.y - 1.0)), Align2::LEFT_CENTER, g, pal.text_strong);
    (head, more_rect)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BtnKind {
    Primary,
    Secondary,
    /// Text only, `hover` behind it: a third, quiet action.
    Ghost,
}

pub(super) fn btn_width(painter: &egui::Painter, label: &str, kind: BtnKind) -> f32 {
    let font = if kind == BtnKind::Primary { theme::ui_semibold(11.5) } else { theme::ui(11.5) };
    galley(painter, label, font, theme::pal().text).size().x + 22.0
}

/// Button with text (corners 5). When off, it is dimmed and does not react. Returns whether it was clicked.
pub(super) fn button(ui: &Ui, rect: Rect, id: Id, label: &str, kind: BtnKind, enabled: bool, tooltip: &str) -> bool {
    let r = if enabled { clickable(ui, rect, id) } else { ui.interact(rect, id, Sense::hover()) };
    let hot = enabled && r.hovered();
    let p = theme::pal();
    let painter = ui.painter();
    if !enabled {
        frame(painter, rect, 5.0, theme::with_alpha(p.text_strong, 0.02), p.separator);
        let g = galley_trunc(painter, label, theme::ui(11.5), p.text_3, rect.width() - 12.0);
        place(painter, rect, Align2::CENTER_CENTER, g, p.text_3);
    } else {
        match kind {
            BtnKind::Primary => {
                let fill = if hot { theme::brighten(theme::accent(), 1.1) } else { theme::accent() };
                painter.rect_filled(rect, 5.0, fill);
                let g = galley_trunc(painter, label, theme::ui_semibold(11.5), p.on_accent, rect.width() - 12.0);
                place(painter, rect, Align2::CENTER_CENTER, g, p.on_accent);
            }
            BtnKind::Secondary => {
                let fill = theme::with_alpha(p.text_strong, if hot { 0.08 } else { 0.04 });
                frame(painter, rect, 5.0, fill, theme::with_alpha(p.text_strong, 0.14));
                let g = galley_trunc(painter, label, theme::ui(11.5), p.text, rect.width() - 12.0);
                place(painter, rect, Align2::CENTER_CENTER, g, p.text);
            }
            BtnKind::Ghost => {
                if hot {
                    painter.rect_filled(rect, 5.0, p.hover);
                }
                let color = if hot { p.text } else { p.text_2 };
                let g = galley_trunc(painter, label, theme::ui(11.5), color, rect.width() - 12.0);
                place(painter, rect, Align2::CENTER_CENTER, g, color);
            }
        }
    }
    let r = if tooltip.is_empty() { r } else { tip(r, tooltip) };
    enabled && r.clicked()
}

/// Buttons in a row that wrap to the next row when they do not fit. Returns the index of the clicked one.
pub(super) fn button_flow(ui: &mut Ui, buttons: &[(String, BtnKind, bool, &str)], id_salt: &str) -> Option<usize> {
    let avail = ui.available_width();
    let max_w = (avail - 2.0 * PAD).max(60.0);
    let widths: Vec<f32> = buttons.iter().map(|(l, k, ..)| btn_width(ui.painter(), l, *k).min(max_w)).collect();
    let mut clicked = None;
    let mut i = 0;
    while i < buttons.len() {
        // How many fit on this row.
        let mut end = i;
        let mut w = 0.0;
        while end < buttons.len() {
            let next = w + widths[end] + if end > i { 6.0 } else { 0.0 };
            if end > i && next > max_w {
                break;
            }
            w = next;
            end += 1;
        }
        let (row, _) = ui.allocate_exact_size(Vec2::new(avail, BTN_H + 6.0), Sense::hover());
        let mut x = row.min.x + PAD;
        for (j, (label, kind, enabled, tooltip)) in buttons.iter().enumerate().take(end).skip(i) {
            let r = Rect::from_min_size(Pos2::new(x, row.min.y), Vec2::new(widths[j], BTN_H));
            x = r.max.x + 6.0;
            if button(ui, r, Id::new(("tasks-btn", id_salt, j)), label, *kind, *enabled, tooltip) {
                clicked = Some(j);
            }
        }
        i = end;
    }
    clicked
}

/// Paragraph with line wrapping, indented `PAD` on both sides. Returns the rectangle it occupies.
pub(super) fn paragraph(ui: &mut Ui, text: &str, font: egui::FontId, color: Color32) -> Rect {
    let w = (ui.available_width() - 2.0 * PAD).max(40.0);
    let g = ui.painter().layout(text.to_owned(), font, color, w);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), g.size().y + 6.0), Sense::hover());
    ui.painter().galley(Pos2::new(r.min.x + PAD, r.min.y + 2.0), g, color);
    r
}

/// Short, quiet message (empty states inside a section).
pub(super) fn muted_line(ui: &mut Ui, text: &str) {
    paragraph(ui, text, theme::ui(12.0), theme::pal().text_2);
}

/// Red strip with the error (and the optional "×"). Returns whether the "×" was clicked.
pub(super) fn error_box(ui: &mut Ui, text: &str, dismiss: bool) -> bool {
    notice_box(ui, text, dismiss, true)
}

/// Notice strip: red (`danger`) for an error, amber (`warning`) for what worked but the user needs to know.
pub(super) fn notice_box(ui: &mut Ui, text: &str, dismiss: bool, danger: bool) -> bool {
    let (tone, text_color) =
        if danger { (theme::pal().danger, theme::pal().danger_text) } else { (theme::pal().warning, theme::pal().text) };
    let w = ui.available_width() - 2.0 * PAD;
    let g = ui.painter().layout(text.to_owned(), theme::ui(12.0), text_color, (w - 22.0 - if dismiss { 18.0 } else { 0.0 }).max(40.0));
    let h = g.size().y + 16.0;
    let (outer, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h + 12.0), Sense::hover());
    let r = Rect::from_min_size(Pos2::new(outer.min.x + PAD, outer.min.y + 3.0), Vec2::new(w, h));
    ui.painter().rect_filled(r, 7.0, theme::with_alpha(tone, 0.12));
    ui.painter().rect_stroke(r, 7.0, Stroke::new(1.0, theme::with_alpha(tone, 0.45)), egui::StrokeKind::Inside);
    ui.painter().galley(Pos2::new(r.min.x + 10.0, r.min.y + 8.0), g, text_color);
    let mut closed = false;
    if dismiss {
        let x = Rect::from_min_size(Pos2::new(r.max.x - 24.0, r.min.y + 2.0), Vec2::splat(22.0));
        let xr = clickable(ui, x, Id::new(("tasks-error-x", text.len())));
        text_in(
            ui.painter(),
            x,
            Align2::CENTER_CENTER,
            "×",
            theme::ui(15.0),
            if xr.hovered() { theme::pal().text } else { theme::pal().text_2 },
        );
        closed = tip(xr, t!("Close the notice")).clicked();
    }
    closed
}

/// Text field in a 1 px frame (`recess` background, `separator` border and `accent` on focus); the frame grows with the text.
pub(super) fn text_field(ui: &mut Ui, id: Id, text: &mut String, hint: &str, rows: Option<usize>) -> egui::Response {
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    let p = theme::pal();
    egui::Frame::new()
        .fill(p.recess)
        .stroke(Stroke::new(1.0, if focused { theme::accent() } else { p.separator }))
        .corner_radius(6)
        .inner_margin(Margin { left: 9, right: 9, top: 6, bottom: 6 })
        .show(ui, |ui| {
            let te = match rows {
                Some(n) => egui::TextEdit::multiline(text).desired_rows(n),
                None => egui::TextEdit::singleline(text),
            };
            te.id(id)
                .frame(egui::Frame::NONE)
                .font(theme::ui(12.0))
                .text_color(p.text)
                .hint_text(egui::RichText::new(hint).font(theme::ui(12.0)).color(p.text_3))
                .desired_width(ui.available_width())
                .show(ui);
        })
        .response
}

/// Runs `f` in a column indented `PAD` on both sides and advances the `ui` cursor by what it occupied.
pub(super) fn padded<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_rect_before_wrap();
    let inner = Rect::from_min_max(Pos2::new(avail.min.x + PAD, avail.min.y), Pos2::new(avail.max.x - PAD, avail.max.y));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    child.spacing_mut().item_spacing = Vec2::ZERO;
    let out = f(&mut child);
    let used = child.min_rect();
    ui.allocate_rect(Rect::from_min_max(Pos2::new(avail.min.x, used.min.y), Pos2::new(avail.max.x, used.max.y)), Sense::hover());
    out
}

/// The lane's header: its name as the app's section label, the count beside it and, for "Finished", the filter on the
/// right (returns its rectangle).
pub(super) fn lane_header(deck: &Deck, ui: &mut Ui, lane: Lane, count: usize, acts: &mut Vec<Act>) -> Option<Rect> {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    let p = theme::pal();
    let painter = ui.painter().clone();
    let row = Rect::from_min_max(Pos2::new(r.min.x + PAD, r.min.y + 10.0), Pos2::new(r.max.x - LIST_PAD, r.max.y - 2.0));
    let g = galley_spaced(&painter, &lane.title().to_uppercase(), theme::label(10.0), p.text_3, 10.0 * theme::LABEL_SPACING_EM);
    let t = place(&painter, row, Align2::LEFT_CENTER, g, p.text_3);
    text_in(
        &painter,
        Rect::from_min_max(Pos2::new(t.max.x + 8.0, row.min.y), row.max),
        Align2::LEFT_CENTER,
        &count.to_string(),
        theme::meta(10.0),
        p.text_faint,
    );
    if lane != Lane::Finished {
        return None;
    }
    let filter_text = format!("{} ▾", deck.tasks.history_filter.label());
    let fw = text_w(&painter, &filter_text, theme::ui(11.0)) + 12.0;
    let f = Rect::from_min_size(Pos2::new(row.max.x - fw, row.center().y - 10.0), Vec2::new(fw, 20.0));
    let resp = clickable(ui, f, Id::new("tasks-history-filter"));
    let hot = resp.hovered() || deck.tasks.history_menu;
    if hot {
        painter.rect_filled(f, 4.0, p.hover);
    }
    text_in(&painter, f, Align2::CENTER_CENTER, &filter_text, theme::ui(11.0), if hot { p.text } else { p.text_2 });
    if tip(resp, t!("Filter the finished tasks")).clicked() {
        acts.push(Act::ToggleHistoryMenu);
    }
    Some(f)
}
