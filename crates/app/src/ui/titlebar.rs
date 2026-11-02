//! The app's own title bar (the window has no system decoration and the bar has no background): ☰ menu,
//! sidebar button, logo, the Agents | Documents switch (only with the sidebar collapsed),
//! project and branch, the window organization switch (Tabs / Split / Highlight), subscription usage,
//! the notifications bell and minimize/maximize/close.

use egui::{Align2, Color32, CursorIcon, Id, Order, PointerButton, Pos2, Rect, ResizeDirection, Sense, Stroke, Ui, Vec2, ViewportCommand};

use super::{chip, clickable, galley, galley_spaced, place, tip};
use crate::deck::Deck;
use crate::icons::Icon;
use crate::theme;
use holt_core::t;
use holt_core::Organization;

pub struct Anchors {
    pub settings_anchor: Rect,
    /// Anchor of the new-agent menu when the sidebar's row is not in view (Ctrl+T with the sidebar collapsed).
    pub fallback_anchor: Rect,
    /// The notifications bell (its popover opens under it).
    pub notices_anchor: Rect,
}

/// Width of each window button (minimize, maximize, close).
const WIN_BTN_W: f32 = 40.0;
/// Size of the window buttons' glyphs.
const WIN_GLYPH: f32 = 9.0;
/// Thickness of the resize strips on the window edges.
const RESIZE_EDGE: f32 = 5.0;
/// Icon buttons on the left (menu, sidebar) and compact organization button.
const ICON_BTN: Vec2 = Vec2::new(26.0, 24.0);
/// Space between the items on the left.
const GAP: f32 = 8.0;
/// Full 3-segment organization switch (`recess` track, corners 6; three 30×20 segments, corners 4, 2 px inset).
pub const ORG_TRACK_W: f32 = 96.0;
pub const ORG_TRACK_H: f32 = 24.0;
const ORG_SEG: Vec2 = Vec2::new(30.0, 20.0);
const ORG_INSET: f32 = 2.0;
/// Size of the layout glyphs and their stroke.
const ORG_GLYPH: Vec2 = Vec2::new(16.0, 11.0);
const ORG_GLYPH_STROKE: f32 = 1.4;
/// Below this window width the 3-segment organization switch compacts to a single 26×24 cycle button.
pub const ORG_FULL_MIN_WIN_W: f32 = 860.0;
/// Anchored to the center island's right edge, the switch keeps at least this much from the usage meter.
const ORG_USAGE_MIN_GAP: f32 = 16.0;
/// Fallback (left of the usage meter): gap on each side of the 1 px separator, which is 16 px tall.
const ORG_FALLBACK_GAP: f32 = 10.0;
/// Space between the mode name and the track.
const ORG_LABEL_GAP: f32 = 8.0;

/// How the window organization switch renders in the title bar given the available horizontal room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgSwitchMode {
    /// Full 96×24 3-segment `recess` switch (`Tabs` / `Split` / `Highlight`).
    Full,
    /// Single 26×24 icon button showing the current organization and cycling on click.
    Compact,
    /// Hidden when `< 26 px` remains between the left title bar items and the right edge of the switch.
    Hidden,
}

/// Decides whether the title bar organization switch renders in full (`96×24`), compacts to a single
/// `26×24` cycle button (`win_w < 860` or `avail_w < 96`), or hides (`avail_w < 26`).
pub fn org_switch_mode(win_w: f32, avail_w: f32) -> OrgSwitchMode {
    if avail_w < ICON_BTN.x {
        OrgSwitchMode::Hidden
    } else if win_w < ORG_FULL_MIN_WIN_W || avail_w < ORG_TRACK_W {
        OrgSwitchMode::Compact
    } else {
        OrgSwitchMode::Full
    }
}

/// Where the organization switch goes in the title bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrgLayout {
    pub mode: OrgSwitchMode,
    /// Right edge of the switch.
    pub right: f32,
    /// A 1 px separator sits between the switch and the usage meter (fallback placement).
    pub separator: bool,
    /// The active mode's name shows left of the track.
    pub label: bool,
}

/// Places the organization switch. It anchors its right edge to the center island's right edge (`center_right`,
/// so it sits above the area it rearranges); when that leaves less than 16 px to the usage meter (or to the
/// notifications bell without a meter), it goes left of the meter instead, with a separator and 10 px gaps.
/// `left_min` is where the items on the left end, `usage_left` the usage meter's left edge (`None` without one),
/// `bar_limit` the right limit of the area (the bell's left edge minus its margin), `label_w` the width of the
/// mode name. The name shows only when it fits beside the full track.
pub fn org_switch_layout(win_w: f32, left_min: f32, center_right: f32, usage_left: Option<f32>, bar_limit: f32, label_w: f32) -> OrgLayout {
    let stop = usage_left.unwrap_or(bar_limit);
    let place = |right: f32, separator: bool| {
        let avail = right - left_min;
        let mode = org_switch_mode(win_w, avail);
        let label = mode == OrgSwitchMode::Full && avail >= ORG_TRACK_W + ORG_LABEL_GAP + label_w;
        OrgLayout { mode, right, separator: separator && mode != OrgSwitchMode::Hidden, label }
    };
    let anchored = (center_right + ORG_USAGE_MIN_GAP <= stop).then(|| place(center_right, false));
    // Fallback: left of the usage meter (or of the bell, with no meter), further right, so it may have more room.
    let fallback = match usage_left {
        Some(left) => place(left - ORG_FALLBACK_GAP - 1.0 - ORG_FALLBACK_GAP, true),
        None => place(stop, false),
    };
    let rank = |m: OrgSwitchMode| match m {
        OrgSwitchMode::Full => 2,
        OrgSwitchMode::Compact => 1,
        OrgSwitchMode::Hidden => 0,
    };
    match anchored {
        Some(a) if rank(a.mode) >= rank(fallback.mode) => a,
        _ => fallback,
    }
}

/// Next window organization when cycling the compact title bar button (`Tabs -> Split -> Highlight -> Tabs`).
pub fn next_organization(org: Organization) -> Organization {
    match org {
        Organization::Tabs => Organization::Split,
        Organization::Split => Organization::Highlight,
        Organization::Highlight => Organization::Tabs,
    }
}

/// The branch to show on the chip: its name, or the start of the commit with a detached HEAD.
fn branch_label(deck: &Deck) -> Option<String> {
    let st = deck.git.status()?;
    st.branch.clone().or_else(|| st.head.as_deref().map(|h| h.chars().take(7).collect()))
}

/// Icon button of the bar (26×24, corners 5, `hover` background). Returns the response and whether it is lit.
fn icon_button(ui: &Ui, painter: &egui::Painter, rect: Rect, id: &str, lit: bool) -> (egui::Response, bool) {
    let r = clickable(ui, rect, Id::new(id));
    let hot = r.hovered() || lit;
    if hot {
        painter.rect_filled(rect, 5.0, theme::pal().hover);
    }
    (r, hot)
}

/// `center_right` is the right edge of the center island (the organization switch anchors to it).
pub fn draw(deck: &mut Deck, ui: &mut Ui, rect: Rect, center_right: f32) -> Anchors {
    let painter = ui.painter_at(rect);
    let p = theme::pal();
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let cy = rect.center().y;

    // ---------- Drag area (under everything: the buttons, created later, win the click) ----------
    let drag = ui.interact(rect, Id::new("titlebar-drag"), Sense::click_and_drag());
    if drag.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    } else if drag.drag_started_by(PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    // ---------- ☰ Menu (settings) ----------
    let mut x = rect.min.x + theme::ISLAND_GAP + 2.0;
    let menu = Rect::from_min_size(Pos2::new(x, cy - ICON_BTN.y / 2.0), ICON_BTN);
    let (r, hot) = icon_button(ui, &painter, menu, "settings-btn", deck.settings_open);
    let c = if hot { p.text } else { p.text_2 };
    for dy in [-4.0, 0.0, 4.0] {
        let y = menu.center().y + dy;
        painter.line_segment([Pos2::new(menu.center().x - 5.5, y), Pos2::new(menu.center().x + 5.5, y)], Stroke::new(1.5, c));
    }
    if tip(r, t!("Menu")).clicked() {
        deck.settings_open = !deck.settings_open;
        deck.new_tab_menu_open = false;
    }
    x = menu.max.x + GAP;

    // ---------- Sidebar (collapse/expand) ----------
    let toggle = Rect::from_min_size(Pos2::new(x, cy - ICON_BTN.y / 2.0), ICON_BTN);
    let (r, hovered) = icon_button(ui, &painter, toggle, "sidebar-toggle", false);
    let expanded = 1.0 - ctx.animate_bool_with_time(Id::new("sidebar-toggle-anim"), deck.sidebar_collapsed, 0.15);
    paint_sidebar_icon(&painter, toggle.center(), if hovered { p.text } else { p.text_2 }, expanded);
    let title = if deck.sidebar_collapsed { t!("Expand sidebar (Ctrl+B)") } else { t!("Collapse sidebar (Ctrl+B)") };
    if tip(r, title).clicked() {
        deck.toggle_sidebar();
    }
    x = toggle.max.x + GAP;

    // ---------- Separator, logo, app name, project and branch ----------
    super::separator(&painter, Pos2::new(x + 0.5, cy - 8.0), Pos2::new(x + 0.5, cy + 8.0));
    x += 1.0 + GAP;
    deck.icons.paint(&painter, Icon::Holt, Pos2::new(x + 8.0, cy), 16.0, p.text_strong, 0.0);
    x += 16.0 + GAP;
    let name = galley_spaced(&painter, "Holt", theme::ui_bold(13.0), p.text, -0.26);
    x = place(&painter, Rect::from_min_max(Pos2::new(x, rect.min.y), Pos2::new(rect.max.x, rect.max.y)), Align2::LEFT_CENTER, name, p.text)
        .max
        .x
        + GAP;
    let project = deck.project_views().into_iter().find(|v| v.active).map(|v| v.name);
    if let Some(project) = project {
        let slash = galley(&painter, "/", theme::ui(12.0), p.text_faint);
        x = place(&painter, Rect::from_min_max(Pos2::new(x, rect.min.y), rect.max), Align2::LEFT_CENTER, slash, p.text_faint).max.x + GAP;
        let g = galley(&painter, &project, theme::ui(12.0), p.text_2);
        x = place(&painter, Rect::from_min_max(Pos2::new(x, rect.min.y), rect.max), Align2::LEFT_CENTER, g, p.text_2).max.x + GAP;
        if let Some(branch) = branch_label(deck) {
            x = chip(&painter, Pos2::new(x, cy), &branch, 10.5, p.text_soft).max.x;
        }
    }
    // With the sidebar collapsed the Agents | Documents switch comes back here, compact and after the
    // project name (expanded, it is the first thing in the sidebar).
    if deck.sidebar_collapsed {
        let size = Vec2::new(super::docs::switch_compact_width(&painter, deck), super::docs::SWITCH_COMPACT_H);
        super::docs::switch(deck, ui, Rect::from_min_size(Pos2::new(x + GAP, cy - size.y / 2.0), size), "title-center");
        x += GAP + size.x + GAP;
    }

    // ---------- Window buttons ----------
    let close = Rect::from_min_max(Pos2::new(rect.max.x - WIN_BTN_W, rect.min.y), rect.max);
    let max_btn = close.translate(Vec2::new(-WIN_BTN_W, 0.0));
    let min_btn = max_btn.translate(Vec2::new(-WIN_BTN_W, 0.0));
    if window_button(ui, &painter, min_btn, "win-min", Glyph::Minimize, false) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
    let glyph = if maximized { Glyph::Restore } else { Glyph::Maximize };
    if window_button(ui, &painter, max_btn, "win-max", glyph, false) {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
    if window_button(ui, &painter, close, "win-close", Glyph::Close, true) {
        deck.close_requested = true;
    }

    // ---------- Notifications (left of the window buttons) ----------
    let bell = Rect::from_min_size(Pos2::new(min_btn.min.x - 6.0 - ICON_BTN.x, cy - ICON_BTN.y / 2.0), ICON_BTN);
    super::notices::bell(deck, ui, &painter, bell);

    // ---------- Subscription usage (left of the bell) ----------
    let usage_right = bell.min.x - 6.0;
    let usage = Rect::from_min_max(Pos2::new(x + 12.0, rect.min.y), Pos2::new(usage_right, rect.max.y));
    let usage_left = if usage.width() > 0.0 { super::usage::draw(deck, ui, usage) } else { usage_right };

    // ---------- Window organization switch (above the area it rearranges) ----------
    let has_usage = usage_left < usage_right;
    let org_label = org_name(deck.organization);
    let label_w = super::text_w(&painter, org_label, theme::ui(11.5));
    let layout = org_switch_layout(rect.width(), x + GAP, center_right, has_usage.then_some(usage_left), usage_right, label_w);
    draw_organization_switch(deck, ui, &painter, layout, usage_left, cy);

    if !maximized {
        resize_edges(&ctx);
    }

    Anchors {
        settings_anchor: menu,
        fallback_anchor: Rect::from_min_size(Pos2::new(toggle.max.x, rect.max.y), Vec2::new(34.0, 0.0)),
        notices_anchor: bell,
    }
}

/// Name of a window organization (the label next to the switch and the menu items).
pub fn org_name(org: Organization) -> &'static str {
    match org {
        Organization::Tabs => t!("Tabs"),
        Organization::Split => t!("Split"),
        Organization::Highlight => t!("Highlight"),
    }
}

/// What choosing an organization does (tooltips and the ☰ Layout menu).
pub fn org_effect(org: Organization) -> &'static str {
    match org {
        Organization::Tabs => t!("Tabs: one window at a time"),
        Organization::Split => t!("Split: windows side by side"),
        Organization::Highlight => t!("Highlight: one big window, the others small"),
    }
}

/// Draws the window organization control (`Tabs` / `Split` / `Highlight`) at `layout`: the active mode's name
/// (`text_3`), then the 96×24 `recess` track with three 30×20 segments (active in `selected`, not accent); a
/// single 26×24 cycle button in the compact mode; nothing when hidden.
fn draw_organization_switch(deck: &mut Deck, ui: &Ui, painter: &egui::Painter, layout: OrgLayout, usage_left: f32, cy: f32) {
    let p = theme::pal();
    if layout.separator {
        let x = usage_left - ORG_FALLBACK_GAP - 0.5;
        super::separator(painter, Pos2::new(x, cy - 8.0), Pos2::new(x, cy + 8.0));
    }
    match layout.mode {
        OrgSwitchMode::Full => {
            let track =
                Rect::from_min_size(Pos2::new(layout.right - ORG_TRACK_W, cy - ORG_TRACK_H / 2.0), Vec2::new(ORG_TRACK_W, ORG_TRACK_H));
            painter.rect_filled(track, 6.0, p.recess);
            if layout.label {
                let g = galley(painter, org_name(deck.organization), theme::ui(11.5), p.text_3);
                let r = Rect::from_min_max(Pos2::new(track.min.x - ORG_LABEL_GAP - g.size().x, track.min.y), Pos2::new(track.min.x - ORG_LABEL_GAP, track.max.y));
                place(painter, r, Align2::RIGHT_CENTER, g, p.text_3);
            }
            for (i, org) in [Organization::Tabs, Organization::Split, Organization::Highlight].into_iter().enumerate() {
                let seg = Rect::from_min_size(track.min + Vec2::splat(ORG_INSET) + Vec2::new(ORG_SEG.x * i as f32, 0.0), ORG_SEG);
                let r = clickable(ui, seg, Id::new(("titlebar-org", i)));
                let on = deck.organization == org;
                if on {
                    painter.rect_filled(seg, 4.0, p.selected);
                } else if r.hovered() {
                    painter.rect_filled(seg, 4.0, p.hover);
                }
                let fg = if on {
                    p.text_strong
                } else if r.hovered() {
                    p.text
                } else {
                    p.text_2
                };
                paint_org_icon(painter, seg.center(), org, fg);
                if tip(r, org_effect(org)).clicked() {
                    deck.set_organization(org);
                }
            }
        }
        OrgSwitchMode::Compact => {
            let btn = Rect::from_min_size(Pos2::new(layout.right - ICON_BTN.x, cy - ICON_BTN.y / 2.0), ICON_BTN);
            let (r, hot) = icon_button(ui, painter, btn, "titlebar-org-compact", false);
            let fg = if hot { p.text } else { p.text_2 };
            paint_org_icon(painter, btn.center(), deck.organization, fg);
            if tip(r, org_effect(deck.organization)).clicked() {
                deck.set_organization(next_organization(deck.organization));
            }
        }
        OrgSwitchMode::Hidden => {}
    }
}

/// 16×11 window-layout glyph (stroke 1.4) for `Tabs` (tab bar on top), `Split` (two equal columns), or
/// `Highlight` (a big left window and two small ones on the right).
fn paint_org_icon(painter: &egui::Painter, center: Pos2, org: Organization, color: Color32) {
    let frame = Rect::from_center_size(center, ORG_GLYPH);
    let stroke = Stroke::new(ORG_GLYPH_STROKE, color);
    painter.rect_stroke(frame, 2.0, stroke, egui::StrokeKind::Inside);
    match org {
        Organization::Tabs => {
            let y = frame.min.y + 4.0;
            painter.line_segment([Pos2::new(frame.min.x + 1.0, y), Pos2::new(frame.max.x - 1.0, y)], stroke);
            painter.line_segment([Pos2::new(frame.min.x + 6.5, frame.min.y + 1.0), Pos2::new(frame.min.x + 6.5, y)], stroke);
        }
        Organization::Split => {
            let x = frame.center().x;
            painter.line_segment([Pos2::new(x, frame.min.y + 1.0), Pos2::new(x, frame.max.y - 1.0)], stroke);
        }
        Organization::Highlight => {
            let x = frame.min.x + 10.0;
            let y = frame.center().y;
            painter.line_segment([Pos2::new(x, frame.min.y + 1.0), Pos2::new(x, frame.max.y - 1.0)], stroke);
            painter.line_segment([Pos2::new(x, y), Pos2::new(frame.max.x - 1.0, y)], stroke);
        }
    }
}

/// "Side panel" icon: a rounded window (12×10) with the left column, filled as far as the
/// bar is open (`expanded` from 0 to 1, animated).
fn paint_sidebar_icon(painter: &egui::Painter, center: Pos2, color: egui::Color32, expanded: f32) {
    let frame = Rect::from_center_size(center, Vec2::new(12.0, 10.0));
    let col = Rect::from_min_max(frame.min, Pos2::new(frame.min.x + 4.0, frame.max.y));
    if expanded > 0.0 {
        let fill = col.shrink(1.5);
        let fill = Rect::from_min_max(fill.min, Pos2::new(fill.min.x + fill.width() * expanded, fill.max.y));
        painter.rect_filled(fill, 1.0, theme::with_opacity(color, 0.85));
    }
    painter.rect_stroke(frame, 2.0, Stroke::new(1.4, color), egui::StrokeKind::Inside);
    painter.line_segment([Pos2::new(col.max.x, frame.min.y + 1.0), Pos2::new(col.max.x, frame.max.y - 1.0)], Stroke::new(1.4, color));
}

#[derive(Clone, Copy)]
enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Window button (`WIN_BTN_W` wide, as tall as the bar). Returns `true` on a click.
fn window_button(ui: &Ui, painter: &egui::Painter, rect: Rect, id: &str, glyph: Glyph, danger: bool) -> bool {
    let r = ui.interact(rect, Id::new(id), Sense::click());
    let hot = r.hovered();
    if hot {
        painter.rect_filled(rect, 0.0, if danger { theme::pal().danger } else { theme::pal().hover });
    }
    let c = if hot && danger {
        theme::pal().text_strong
    } else if hot {
        theme::pal().text
    } else {
        theme::pal().text_3
    };
    let s = Stroke::new(1.0, c);
    let ctr = rect.center();
    let h = WIN_GLYPH / 2.0;
    match glyph {
        Glyph::Minimize => {
            painter.line_segment([Pos2::new(ctr.x - h, ctr.y), Pos2::new(ctr.x + h, ctr.y)], s);
        }
        Glyph::Maximize => {
            painter.rect_stroke(Rect::from_center_size(ctr, Vec2::splat(WIN_GLYPH)), 1.0, s, egui::StrokeKind::Inside);
        }
        Glyph::Restore => {
            // Two overlapping squares: the one behind only shows above and to the right.
            let front = Rect::from_center_size(ctr + Vec2::new(-1.0, 1.0), Vec2::splat(WIN_GLYPH - 2.0));
            painter.rect_stroke(front, 1.0, s, egui::StrokeKind::Inside);
            let (x0, y0, x1, y1) = (front.min.x + 2.0, front.min.y - 2.0, front.max.x + 2.0, front.max.y - 2.0);
            painter.line_segment([Pos2::new(x0, front.min.y), Pos2::new(x0, y0)], s);
            painter.line_segment([Pos2::new(x0, y0), Pos2::new(x1, y0)], s);
            painter.line_segment([Pos2::new(x1, y0), Pos2::new(x1, y1)], s);
            painter.line_segment([Pos2::new(x1, y1), Pos2::new(front.max.x, y1)], s);
        }
        Glyph::Close => {
            painter.line_segment([ctr + Vec2::new(-h, -h), ctr + Vec2::new(h, h)], s);
            painter.line_segment([ctr + Vec2::new(-h, h), ctr + Vec2::new(h, -h)], s);
        }
    }
    let label = match glyph {
        Glyph::Minimize => t!("Minimize"),
        Glyph::Maximize => t!("Maximize"),
        Glyph::Restore => t!("Restore"),
        Glyph::Close => t!("Close"),
    };
    tip(r, label).clicked()
}

/// Strips on the window's edges and corners to resize it (without decoration, the system offers none).
/// Each one on a foreground layer: they sit above the right bar and the terminals.
fn resize_edges(ctx: &egui::Context) {
    let s = ctx.content_rect();
    let e = RESIZE_EDGE;
    let c = 2.0 * e;
    let edges = [
        (Rect::from_min_max(s.min, Pos2::new(s.min.x + c, s.min.y + c)), ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest),
        (
            Rect::from_min_max(Pos2::new(s.max.x - c, s.min.y), Pos2::new(s.max.x, s.min.y + c)),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        (
            Rect::from_min_max(Pos2::new(s.min.x, s.max.y - c), Pos2::new(s.min.x + c, s.max.y)),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        (Rect::from_min_max(Pos2::new(s.max.x - c, s.max.y - c), s.max), ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast),
        (
            Rect::from_min_max(Pos2::new(s.min.x + c, s.min.y), Pos2::new(s.max.x - c, s.min.y + e)),
            ResizeDirection::North,
            CursorIcon::ResizeNorth,
        ),
        (
            Rect::from_min_max(Pos2::new(s.min.x + c, s.max.y - e), Pos2::new(s.max.x - c, s.max.y)),
            ResizeDirection::South,
            CursorIcon::ResizeSouth,
        ),
        (
            Rect::from_min_max(Pos2::new(s.min.x, s.min.y + c), Pos2::new(s.min.x + e, s.max.y - c)),
            ResizeDirection::West,
            CursorIcon::ResizeWest,
        ),
        (
            Rect::from_min_max(Pos2::new(s.max.x - e, s.min.y + c), Pos2::new(s.max.x, s.max.y - c)),
            ResizeDirection::East,
            CursorIcon::ResizeEast,
        ),
    ];
    for (i, (rect, dir, cursor)) in edges.into_iter().enumerate() {
        egui::Area::new(Id::new(("resize-edge", i))).order(Order::Foreground).fixed_pos(rect.min).constrain(false).show(ctx, |ui| {
            let (_, r) = ui.allocate_exact_size(rect.size(), Sense::drag());
            if r.hovered() || r.dragged() {
                ctx.set_cursor_icon(cursor);
            }
            if r.drag_started_by(PointerButton::Primary) {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organization_switch_adapts_to_window_and_available_width() {
        // Wide window with >= 96 px free shows the full 96×24 3-segment switch.
        assert_eq!(org_switch_mode(1280.0, 200.0), OrgSwitchMode::Full);
        assert_eq!(org_switch_mode(860.0, 96.0), OrgSwitchMode::Full);
        // Narrow window (< 860 px) or < 96 px free compacts to a single 26×24 cycle button.
        assert_eq!(org_switch_mode(760.0, 200.0), OrgSwitchMode::Compact);
        assert_eq!(org_switch_mode(1280.0, 95.0), OrgSwitchMode::Compact);
        assert_eq!(org_switch_mode(760.0, 26.0), OrgSwitchMode::Compact);
        // When < 26 px remains, the switch hides so it never crowds the title bar.
        assert_eq!(org_switch_mode(1280.0, 25.0), OrgSwitchMode::Hidden);
        assert_eq!(org_switch_mode(760.0, 0.0), OrgSwitchMode::Hidden);

        assert_eq!(next_organization(Organization::Tabs), Organization::Split);
        assert_eq!(next_organization(Organization::Split), Organization::Highlight);
        assert_eq!(next_organization(Organization::Highlight), Organization::Tabs);
    }

    #[test]
    fn organization_switch_anchors_to_the_center_island_or_falls_back_left_of_the_usage_meter() {
        // 1280 px: center island ends at 940, the usage meter starts at 1030: anchored, with the mode name.
        let l = org_switch_layout(1280.0, 330.0, 940.0, Some(1030.0), 1180.0, 30.0);
        assert_eq!(l, OrgLayout { mode: OrgSwitchMode::Full, right: 940.0, separator: false, label: true });
        // Exactly 16 px from the meter still anchors; 15 px falls back left of it (10 px gaps and a 1 px separator).
        assert_eq!(org_switch_layout(1280.0, 330.0, 1014.0, Some(1030.0), 1180.0, 30.0).right, 1014.0);
        let l = org_switch_layout(1280.0, 330.0, 1015.0, Some(1030.0), 1180.0, 30.0);
        assert_eq!((l.right, l.separator), (1009.0, true));
        // Without a usage meter the bell's limit is the stop; the fallback has no separator.
        let l = org_switch_layout(1280.0, 330.0, 1170.0, None, 1180.0, 30.0);
        assert_eq!((l.right, l.separator), (1180.0, false));
        // The name hides first when the room is short of track + gap + name; then the track compacts.
        let l = org_switch_layout(1280.0, 840.0, 940.0, Some(1030.0), 1180.0, 30.0);
        assert_eq!((l.mode, l.label), (OrgSwitchMode::Full, false));
        let l = org_switch_layout(1280.0, 860.0, 940.0, Some(1030.0), 1180.0, 30.0);
        assert_eq!((l.mode, l.right, l.separator), (OrgSwitchMode::Full, 1009.0, true));
        // A narrow window compacts the button but keeps it above the center island.
        let l = org_switch_layout(760.0, 400.0, 560.0, Some(640.0), 700.0, 30.0);
        assert_eq!((l.mode, l.right, l.separator), (OrgSwitchMode::Compact, 560.0, false));
        // No room anywhere: hidden, and no orphan separator.
        let l = org_switch_layout(760.0, 640.0, 560.0, Some(640.0), 700.0, 30.0);
        assert_eq!((l.mode, l.separator), (OrgSwitchMode::Hidden, false));
    }
}
