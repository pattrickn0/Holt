//! `#sidebar`: the Agents | Documents switch, the projects and agents, the draggable divider, and the
//! VS Code-style file explorer with filter and changed-only views.

use std::collections::HashMap;

use egui::{Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::{clickable, galley, galley_trunc, paint_splitter, place, section_label, separator, text_in, tip};
use crate::deck::{Deck, TreeRow};
use crate::icons::Icon;
use crate::theme;
use holt_core::git::{EntryKind, GitEntry};
use holt_core::t;
use holt_core::DirEntry;

/// Height of the `FILES` section header.
pub const FILES_HEAD_H: f32 = super::projects::SECTION_H;
/// Height of an explorer row.
pub const FILE_ROW_H: f32 = 20.0;
/// Minimum height of the `PROJECTS` list above the divider: 3 agent rows.
pub const MIN_PROJECTS_LIST_H: f32 = 3.0 * super::projects::AGENT_ROW_H;
/// Minimum height of the `FILES` section below the divider: header + 3 file rows.
pub const MIN_FILES_H: f32 = FILES_HEAD_H + 3.0 * FILE_ROW_H;
/// Height of the draggable divider hit area between `PROJECTS` and `FILES`.
pub const SPLITTER_H: f32 = 6.0;
/// Height reserved at the bottom of the sidebar when `FILES` is collapsed (pinned footer).
const COLLAPSED_FILES_H: f32 = FILES_HEAD_H + 4.0;
/// Default fraction of the sidebar height below the `PROJECTS` header allocated to `PROJECTS`.
pub const DEFAULT_PROJECTS_RATIO: f32 = 0.42;
/// Height of the filter input box under the `FILES` header.
const FILTER_BAR_H: f32 = 24.0;
/// Side margin of the rows (the selection does not touch the island edge) and indent per folder level.
const SIDE_MARGIN: f32 = 4.0;
const FILE_INDENT: f32 = 12.0;

/// Buttons of the `FILES` header. Filter and ChangedOnly are always there (toggles with a lit state);
/// CollapseAll only shows while the pointer is over the `FILES` area. Refresh lives only in the right-click menu:
/// the tree already refreshes by itself (`CoreEvent::WorkspaceChanged`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesAction {
    Filter,
    ChangedOnly,
    CollapseAll,
}

/// Hit area of a header button (corners 5).
const FILES_BTN: f32 = 22.0;
/// Space between the header buttons, and from the last one to the island edge.
const FILES_BTN_GAP: f32 = 2.0;
const FILES_BTN_EDGE: f32 = 6.0;
/// Grid of the header glyphs: a 12 px box, stroke 1.3 for all.
const FILES_GLYPH_STROKE: f32 = 1.3;

/// Flattened file row when a filter query or "changed only" is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatFileRow {
    pub name: String,
    pub parent_dir: String,
    pub rel_path: String,
    pub git_mark: Option<char>,
}

/// Computes `(max_list_h, cap_h)` for the `PROJECTS` list given the available height below the `PROJECTS`
/// header (`avail_h`), the natural height of the project/agent tree (`content_h`), the saved fraction (`ratio`),
/// and whether `FILES` is collapsed.
///
/// When the tree is shorter than `cap_h`, `max_list_h == content_h` so `FILES` rises right under `PROJECTS`
/// with no empty band. Minimums: 3 agent rows above (`MIN_PROJECTS_LIST_H`), `FILES` header + 3 file rows below
/// (`MIN_FILES_H`).
pub fn projects_max_height(avail_h: f32, content_h: f32, ratio: f32, files_collapsed: bool) -> (f32, f32) {
    let avail_h = avail_h.max(0.0);
    if files_collapsed {
        let cap = (avail_h - COLLAPSED_FILES_H).max(0.0);
        return (content_h.min(cap), cap);
    }
    let max_cap = (avail_h - SPLITTER_H - MIN_FILES_H).max(MIN_PROJECTS_LIST_H).min(avail_h);
    let min_cap = MIN_PROJECTS_LIST_H.min(max_cap);
    let r = if ratio > 0.0 { ratio.clamp(0.05, 0.95) } else { DEFAULT_PROJECTS_RATIO };
    let cap_h = (avail_h * r).round().clamp(min_cap, max_cap);
    (content_h.min(cap_h), cap_h)
}

/// Converts a pointer Y offset (measured from the top of the `PROJECTS` list) into a clamped config fraction.
pub fn clamp_projects_ratio(avail_h: f32, pointer_offset_y: f32) -> f32 {
    if avail_h <= 1.0 {
        return DEFAULT_PROJECTS_RATIO;
    }
    let max_cap = (avail_h - SPLITTER_H - MIN_FILES_H).max(MIN_PROJECTS_LIST_H).min(avail_h);
    let min_cap = MIN_PROJECTS_LIST_H.min(max_cap);
    let clamped = pointer_offset_y.clamp(min_cap, max_cap);
    (clamped / avail_h).clamp(0.05, 0.95)
}

/// Whether the hover-only collapse-all button has room next to the two always-visible ones, given the
/// horizontal room to the right of the `"FILES"` label (and the 6 px edge margin already taken out).
pub fn files_hover_slot_fits(avail_after_label: f32) -> bool {
    avail_after_label >= 3.0 * FILES_BTN + 2.0 * FILES_BTN_GAP
}

/// What the `FILES` header says next to the label: `"N changed"` while the changed-only view is on, otherwise
/// `"<project> · <branch>"`.
pub fn files_header_meta(changed_only: bool, changed: usize, project: Option<&str>, branch: Option<&str>) -> (String, bool) {
    if changed_only {
        let text = if changed == 1 { t!("1 changed").to_owned() } else { t!("{n} changed", n = changed) };
        (text, true)
    } else {
        (files_context_label(project, branch), false)
    }
}

/// Formats `"<project> · <branch>"` for the `FILES` header (or `"<project>"` when there is no branch).
pub fn files_context_label(project: Option<&str>, branch: Option<&str>) -> String {
    let proj = project.map(str::trim).filter(|s| !s.is_empty());
    let br = branch.map(str::trim).filter(|s| !s.is_empty());
    match (proj, br) {
        (Some(p), Some(b)) => format!("{p} · {b}"),
        (Some(p), None) => p.to_owned(),
        _ => String::new(),
    }
}

/// Flattens matching workspace files into path rows when `query` is non-empty or `changed_only` is active.
/// Includes deleted git entries from `marks` that are no longer on disk in `files`.
pub fn flatten_files(files: &[DirEntry], marks: &HashMap<String, char>, query: &str, changed_only: bool) -> Vec<FlatFileRow> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    for entry in files {
        if entry.is_dir {
            continue;
        }
        let mark = marks.get(entry.rel_path.as_str()).copied();
        if changed_only && mark.is_none() {
            continue;
        }
        if !q.is_empty() && !entry.name.to_lowercase().contains(&q) && !entry.rel_path.to_lowercase().contains(&q) {
            continue;
        }
        let parent_dir = entry.rel_path.rsplit_once('/').map_or_else(String::new, |(dir, _)| dir.to_owned());
        out.push(FlatFileRow { name: entry.name.clone(), parent_dir, rel_path: entry.rel_path.clone(), git_mark: mark });
    }
    // Deleted files reported by git status may not appear in `files` on disk: include them when they match.
    let mut extra: Vec<(&String, &char)> = marks.iter().filter(|(path, _)| !out.iter().any(|r| &r.rel_path == *path)).collect();
    extra.sort_by_key(|(a, _)| *a);
    for (rel_path, &mark) in extra {
        let (parent_dir, name) = match rel_path.rsplit_once('/') {
            Some((dir, n)) => (dir.to_owned(), n.to_owned()),
            None => (String::new(), rel_path.clone()),
        };
        if !q.is_empty() && !name.to_lowercase().contains(&q) && !rel_path.to_lowercase().contains(&q) {
            continue;
        }
        out.push(FlatFileRow { name, parent_dir, rel_path: rel_path.clone(), git_mark: Some(mark) });
    }
    out
}

pub fn draw(deck: &mut Deck, ui: &mut Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    let ctx = ui.ctx().clone();

    // Keeps the workspace root logically expanded so that `deck.tree_rows()` lists the files.
    deck.tree.expanded.insert(String::new());

    let dragging = ctx.dragged_id().is_some() || deck.pane_drag.is_some() || deck.drop_hover;

    // ---------- Agents | Documents switch, with the island width (6 px margin) ----------
    let switch = Rect::from_min_size(Pos2::new(rect.min.x + 6.0, rect.min.y + 6.0), Vec2::new(rect.width() - 12.0, super::docs::SWITCH_H));
    super::docs::switch(deck, ui, switch, "side-center");
    let mut top_y = switch.max.y + 6.0;
    separator(&painter, Pos2::new(rect.min.x, top_y + 0.5), Pos2::new(rect.max.x, top_y + 0.5));
    top_y += 1.0;

    // ---------- PROJECTS section ----------
    let projects = deck.project_views();
    let gap = ui.spacing().item_spacing.y;
    let content_h = super::projects::list_height(&projects) + gap * projects.len() as f32;
    let list_top = top_y + 4.0 + super::projects::SECTION_H + 2.0;
    let avail_h = (rect.max.y - list_top).max(0.0);
    let (max_list_h, _) = projects_max_height(avail_h, content_h, deck.sidebar_projects_ratio, deck.files_collapsed);

    super::projects::draw(deck, ui, &mut top_y, rect, max_list_h, dragging);

    // ---------- Draggable divider between PROJECTS and FILES (when FILES is open) ----------
    if !deck.files_collapsed {
        let split_r = Rect::from_min_size(Pos2::new(rect.min.x, top_y + 2.0), Vec2::new(rect.width(), SPLITTER_H));
        let split_resp = ui.interact(split_r, Id::new("projects-files-splitter"), Sense::drag());
        if split_resp.hovered() || split_resp.dragged() {
            ctx.set_cursor_icon(CursorIcon::ResizeVertical);
        }
        if split_resp.drag_started() {
            deck.sidebar_projects_dragging = true;
        }
        if split_resp.dragged() {
            if let Some(ptr) = ui.input(|i| i.pointer.latest_pos()) {
                deck.sidebar_projects_ratio = clamp_projects_ratio(avail_h, ptr.y - list_top);
            }
        }
        if split_resp.drag_stopped() {
            deck.sidebar_projects_dragging = false;
            deck.persist_layout();
        }
        let hot = split_resp.hovered() || deck.sidebar_projects_dragging || deck.debug_projects_splitter_hot;
        let line_y = split_r.center().y;
        separator(&painter, Pos2::new(rect.min.x, line_y), Pos2::new(rect.max.x, line_y));
        paint_splitter(&painter, split_r.shrink2(Vec2::new(4.0, 0.0)), hot, deck.accent());
        tip(split_resp, t!("Drag to resize"));
        top_y = split_r.max.y;
    }

    // ---------- FILES section header ----------
    let head_rect = if deck.files_collapsed {
        let sep_y = rect.max.y - COLLAPSED_FILES_H + 0.5;
        separator(&painter, Pos2::new(rect.min.x, sep_y), Pos2::new(rect.max.x, sep_y));
        Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y - FILES_HEAD_H - 2.0), Vec2::new(rect.width(), FILES_HEAD_H))
    } else {
        Rect::from_min_size(Pos2::new(rect.min.x, top_y + 1.0), Vec2::new(rect.width(), FILES_HEAD_H))
    };
    top_y = head_rect.max.y;

    let files_label = t!("FILES");
    section_label(&painter, head_rect.shrink2(Vec2::new(0.0, 1.0)), files_label);
    let label_w =
        super::galley_spaced(&painter, files_label, theme::label(10.0), theme::pal().text_3, 10.0 * theme::LABEL_SPACING_EM).size().x;
    let label_end_x = head_rect.min.x + 10.0 + label_w;

    let has_folder = deck.has_folder();
    let active_proj_name = projects.iter().find(|p| p.active).map(|p| p.name.as_str());
    let branch_name = deck.git.status().and_then(|s| s.branch.clone());
    let marks = git_marks(deck);
    let (meta_text, meta_strong) = if has_folder {
        files_header_meta(deck.tree.changed_only, marks.len(), active_proj_name, branch_name.as_deref())
    } else {
        (String::new(), false)
    };

    // Clicking "FILES" (and the context label area) collapses/expands the FILES section; right click opens the
    // menu with the actions that are not always on the header. Created first: the buttons, created later, win the click.
    let toggle_resp = clickable(ui, head_rect, Id::new("files-header-toggle"));
    if toggle_resp.secondary_clicked() && has_folder {
        let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(head_rect.center());
        ctx.data_mut(|d| d.insert_temp(files_menu_key(), Some(pos)));
    }

    // The collapse-all button fades in while the pointer is over the FILES area (header and tree).
    let area = Rect::from_min_max(head_rect.min, if deck.files_collapsed { head_rect.max } else { rect.max });
    let over_area = !dragging && ctx.input(|i| i.pointer.hover_pos()).is_some_and(|ptr| area.contains(ptr));
    let hover_t = ctx.animate_bool_with_time(Id::new("files-hover-actions"), over_area, 0.12);

    let mut icons_left_x = head_rect.max.x - FILES_BTN_EDGE;
    let mut slot_w = 0.0;
    if has_folder && !deck.files_collapsed {
        let avail_after_label = (icons_left_x - (label_end_x + 8.0)).max(0.0);
        let mut actions = vec![FilesAction::ChangedOnly, FilesAction::Filter];
        if files_hover_slot_fits(avail_after_label) {
            actions.push(FilesAction::CollapseAll);
        }
        for action in actions {
            let hover_only = action == FilesAction::CollapseAll;
            let btn_r = Rect::from_min_size(
                Pos2::new(icons_left_x - FILES_BTN, head_rect.min.y + (FILES_HEAD_H - FILES_BTN) / 2.0),
                Vec2::splat(FILES_BTN),
            );
            icons_left_x = btn_r.min.x - FILES_BTN_GAP;
            let vis = if hover_only { hover_t } else { 1.0 };
            if hover_only {
                slot_w = (FILES_BTN + FILES_BTN_GAP) * hover_t;
                if hover_t <= 0.0 {
                    continue;
                }
            }
            let is_lit = match action {
                FilesAction::Filter => deck.tree.filter_open || !deck.tree.filter.trim().is_empty(),
                FilesAction::ChangedOnly => deck.tree.changed_only,
                FilesAction::CollapseAll => false,
            };
            let resp = clickable(ui, btn_r, Id::new(("files-hdr-btn", action as u8)));
            let hov = resp.hovered() && !dragging;
            let p = theme::pal();
            if is_lit {
                painter.rect_filled(btn_r, 5.0, theme::with_opacity(p.selected, vis));
            } else if hov {
                painter.rect_filled(btn_r, 5.0, theme::with_opacity(p.hover, vis));
            }
            let fg = if is_lit {
                p.text_strong
            } else if hov {
                p.text
            } else {
                p.text_3
            };
            let fg = theme::with_opacity(fg, vis);
            match action {
                FilesAction::Filter => paint_filter_icon(&painter, btn_r.center(), fg),
                FilesAction::ChangedOnly => paint_changed_icon(&painter, btn_r.center(), fg),
                FilesAction::CollapseAll => paint_collapse_all(&painter, btn_r.center(), fg),
            }
            let tooltip = match action {
                FilesAction::Filter => t!("Filter files"),
                FilesAction::ChangedOnly => t!("Show changed files only"),
                FilesAction::CollapseAll => t!("Collapse all folders"),
            };
            if tip(resp, tooltip).clicked() {
                match action {
                    FilesAction::Filter => {
                        deck.tree.filter_open = !deck.tree.filter_open;
                        if deck.tree.filter_open {
                            deck.tree.focus_filter = true;
                        } else {
                            deck.tree.filter.clear();
                        }
                        deck.persist_layout();
                    }
                    FilesAction::ChangedOnly => {
                        deck.tree.changed_only = !deck.tree.changed_only;
                        deck.persist_layout();
                    }
                    FilesAction::CollapseAll => collapse_all_folders(deck),
                }
            }
        }
    }
    if hover_t > 0.0 && hover_t < 1.0 {
        // The fade is a few frames: keep drawing until it settles.
        ctx.request_repaint();
    }

    // Context text ("<project> · <branch>", or "N changed") in meta(10), with ellipsis; it gives way to the
    // collapse-all button as it fades in.
    let ctx_left = label_end_x + 5.0;
    let ctx_right = icons_left_x - 2.0 - if has_folder { slot_w } else { 0.0 };
    if !meta_text.is_empty() && ctx_right - ctx_left >= 24.0 {
        let color = if meta_strong { theme::pal().text_2 } else { theme::pal().text_3 };
        let g = galley_trunc(&painter, &meta_text, theme::meta(10.0), color, ctx_right - ctx_left);
        place(
            &painter,
            Rect::from_min_max(Pos2::new(ctx_left, head_rect.min.y), Pos2::new(ctx_right, head_rect.max.y)),
            Align2::LEFT_CENTER,
            g,
            color,
        );
    }

    let toggle_tip = if deck.files_collapsed { t!("Expand files") } else { t!("Collapse files") };
    if tip(toggle_resp, toggle_tip).clicked() {
        deck.files_collapsed = !deck.files_collapsed;
        deck.persist_layout();
    }
    show_files_menu(deck, &ctx);

    if deck.files_collapsed {
        return;
    }

    // ---------- Empty state when the project has no folder ----------
    if !has_folder {
        let empty_r = Rect::from_min_max(Pos2::new(rect.min.x, top_y), rect.max);
        draw_no_folder_empty_state(deck, ui, &painter, empty_r);
        return;
    }

    // ---------- Filter field (24 px, recess, corners 6, Esc closes) ----------
    if deck.tree.filter_open {
        let field_r = Rect::from_min_size(Pos2::new(rect.min.x + 6.0, top_y + 2.0), Vec2::new(rect.width() - 12.0, FILTER_BAR_H));
        top_y = field_r.max.y + 2.0;
        painter.rect_filled(field_r, 6.0, theme::pal().recess);
        paint_filter_icon(&painter, Pos2::new(field_r.min.x + 12.0, field_r.center().y), theme::pal().text_3);

        // Clear / close button on the right of the filter field
        let clear_r = Rect::from_min_size(Pos2::new(field_r.max.x - 20.0, field_r.min.y + 3.0), Vec2::new(17.0, 18.0));
        let clear_resp = clickable(ui, clear_r, Id::new("files-filter-clear"));
        if clear_resp.hovered() {
            painter.rect_filled(clear_r, 4.0, theme::pal().hover);
        }
        let clear_fg = if clear_resp.hovered() { theme::pal().text } else { theme::pal().text_3 };
        let cc = clear_r.center();
        let cs = Stroke::new(1.3, clear_fg);
        painter.line_segment([cc + Vec2::new(-3.0, -3.0), cc + Vec2::new(3.0, 3.0)], cs);
        painter.line_segment([cc + Vec2::new(3.0, -3.0), cc + Vec2::new(-3.0, 3.0)], cs);

        let text_r =
            Rect::from_min_max(Pos2::new(field_r.min.x + 22.0, field_r.min.y + 3.0), Pos2::new(clear_r.min.x - 2.0, field_r.max.y - 3.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(text_r));
        let filter_id = Id::new("files-filter-input");
        let before = deck.tree.filter.clone();
        let out = egui::TextEdit::singleline(&mut deck.tree.filter)
            .id(filter_id)
            .frame(egui::Frame::NONE)
            .font(theme::ui(11.5))
            .text_color(theme::pal().text)
            .hint_text(egui::RichText::new(t!("Filter files…")).font(theme::ui(11.5)).color(theme::pal().text_3))
            .desired_width(text_r.width().max(1.0))
            .show(&mut child);
        if deck.tree.focus_filter {
            out.response.request_focus();
            deck.tree.focus_filter = false;
        }
        let border = if out.response.has_focus() { theme::accent_edge(deck.accent()) } else { theme::pal().separator };
        painter.rect_stroke(field_r, 6.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);

        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if tip(clear_resp, t!("Close filter (Esc)")).clicked() || esc {
            deck.tree.filter.clear();
            deck.tree.filter_open = false;
            deck.persist_layout();
        } else if deck.tree.filter != before {
            deck.persist_layout();
        }
    }

    // ---------- File list (flattened when filter or changed_only is active; tree otherwise) ----------
    let list_top = top_y + 2.0;
    let list_rect = Rect::from_min_max(Pos2::new(rect.min.x, list_top), rect.max);
    let flatten_active = !deck.tree.filter.trim().is_empty() || deck.tree.changed_only;

    if flatten_active {
        let all_files = deck.all_workspace_files();
        let flat_rows = flatten_files(&all_files, &marks, &deck.tree.filter, deck.tree.changed_only);
        if flat_rows.is_empty() {
            let msg =
                if deck.tree.changed_only && deck.tree.filter.trim().is_empty() { t!("No changed files") } else { t!("No matching files") };
            let msg_r =
                Rect::from_min_size(Pos2::new(list_rect.min.x + 12.0, list_rect.min.y + 12.0), Vec2::new(list_rect.width() - 24.0, 20.0));
            let g = galley_trunc(&painter, msg, theme::ui(11.5), theme::pal().text_3, msg_r.width().max(1.0));
            place(&painter, msg_r, Align2::LEFT_CENTER, g, theme::pal().text_3);
            return;
        }
        let mut ui_list = ui.new_child(egui::UiBuilder::new().max_rect(list_rect).layout(egui::Layout::top_down(egui::Align::Min)));
        ui_list.set_clip_rect(list_rect);
        let mut clicked_file: Option<String> = None;
        let mut dbl_file: Option<String> = None;
        egui::ScrollArea::vertical().id_salt("file-tree-flat").auto_shrink([false, false]).show_rows(
            &mut ui_list,
            FILE_ROW_H,
            flat_rows.len(),
            |ui, range| {
                let width = ui.available_width();
                for i in range {
                    let (row_rect, resp) = ui.allocate_exact_size(Vec2::new(width, FILE_ROW_H), Sense::click());
                    let row = &flat_rows[i];
                    paint_flat_row(deck, ui, row_rect, row, &resp);
                    let resp = tip(resp, row.rel_path.clone());
                    if resp.double_clicked() {
                        dbl_file = Some(row.rel_path.clone());
                    } else if resp.clicked() {
                        clicked_file = Some(row.rel_path.clone());
                    }
                }
            },
        );
        if let Some(f) = dbl_file {
            deck.tree_file_double_click(&f);
        } else if let Some(f) = clicked_file {
            deck.tree_file_click(&f);
        }
        return;
    }

    let rows = deck.tree_rows();
    let mut ui_list = ui.new_child(egui::UiBuilder::new().max_rect(list_rect).layout(egui::Layout::top_down(egui::Align::Min)));
    ui_list.set_clip_rect(list_rect);
    let mut clicked_dir: Option<String> = None;
    let mut clicked_file: Option<String> = None;
    let mut dbl_file: Option<String> = None;
    egui::ScrollArea::vertical().id_salt("file-tree").auto_shrink([false, false]).show_rows(
        &mut ui_list,
        FILE_ROW_H,
        rows.len(),
        |ui, range| {
            let width = ui.available_width();
            for i in range {
                let (row_rect, resp) = ui.allocate_exact_size(Vec2::new(width, FILE_ROW_H), Sense::click());
                let row = &rows[i];
                paint_row(deck, ui, row_rect, row, &resp, &marks);
                let resp = tip(resp, row.entry.rel_path.clone());
                if row.entry.is_dir {
                    if resp.clicked() {
                        clicked_dir = Some(row.entry.rel_path.clone());
                    }
                } else if resp.double_clicked() {
                    dbl_file = Some(row.entry.rel_path.clone());
                } else if resp.clicked() {
                    clicked_file = Some(row.entry.rel_path.clone());
                }
            }
        },
    );
    if let Some(d) = clicked_dir {
        deck.toggle_dir(&d);
    }
    if let Some(f) = dbl_file {
        deck.tree_file_double_click(&f);
    } else if let Some(f) = clicked_file {
        deck.tree_file_click(&f);
    }
}

/// Empty state of the `FILES` section when the project has no folder: the message in `text_2`, wrapped and
/// centered, and 10 px under it a Secondary `"Open folder…"` button with the folder glyph. The block starts 16 px
/// under the header (`rect.min.y` is the header's bottom edge).
fn draw_no_folder_empty_state(deck: &mut Deck, ui: &Ui, painter: &egui::Painter, rect: Rect) {
    let p = theme::pal();
    let wrap_w = (rect.width() - 20.0).max(40.0);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_w;
    job.halign = egui::Align::Center;
    job.append(
        t!("This project has no folder"),
        0.0,
        egui::TextFormat { font_id: theme::ui(12.0), color: p.text_2, line_height: Some(12.0 * 1.4), ..Default::default() },
    );
    let msg = painter.layout_job(job);
    let top = rect.min.y + 16.0;
    let msg_h = msg.size().y;
    painter.galley(Pos2::new(rect.center().x, top), msg, p.text_2);

    let label = t!("Open folder…");
    let btn_w = super::folder_button_width(painter, label).min((rect.width() - 12.0).max(40.0));
    let btn_r = Rect::from_center_size(
        Pos2::new(rect.center().x, top + msg_h + 10.0 + super::FOLDER_BUTTON_H / 2.0),
        Vec2::new(btn_w, super::FOLDER_BUTTON_H),
    );
    if super::folder_button(ui, btn_r, Id::new("files-empty-open-folder"), label).clicked() {
        deck.pick_workspace();
    }
}

/// Folds every folder of the explorer back to the root.
fn collapse_all_folders(deck: &mut Deck) {
    deck.tree.expanded.retain(|p| p.is_empty());
    deck.tree.expanded.insert(String::new());
}

fn files_menu_key() -> Id {
    Id::new("files-ctx-menu")
}

/// Right-click menu of the `FILES` header: the actions that are not always on it.
fn show_files_menu(deck: &mut Deck, ctx: &egui::Context) {
    let Some(pos) = ctx.data(|d| d.get_temp::<Option<Pos2>>(files_menu_key())).flatten() else { return };
    let (w, item_h) = (190.0, 26.0);
    let h = item_h * 2.0 + 8.0;
    let pos = super::menus::keep_on_screen(ctx, pos, Vec2::new(w, h));
    let menu_rect = Rect::from_min_size(pos, Vec2::new(w, h));
    let mut picked = None;
    egui::Area::new(Id::new("files-ctx-menu-area")).order(egui::Order::Foreground).fixed_pos(pos).constrain(false).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(menu_rect.size(), Sense::hover());
        let p = ui.painter();
        super::shadow(p, rect, 8, 8, 24, theme::with_alpha(theme::pal().shadow, 0.55));
        super::frame(p, rect, 6.0, theme::pal().popup, theme::pal().glass_border);
        for (i, label) in [t!("Collapse all folders"), t!("Refresh explorer")].into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(rect.min.x + 4.0, rect.min.y + 4.0 + item_h * i as f32), Vec2::new(w - 8.0, item_h));
            let resp = clickable(ui, r, Id::new(("files-ctx-item", i)));
            if resp.hovered() {
                p.rect_filled(r, 4.0, theme::pal().hover);
            }
            text_in(p, r.shrink2(Vec2::new(8.0, 0.0)), Align2::LEFT_CENTER, label, theme::ui(12.5), theme::pal().text);
            if resp.clicked() {
                picked = Some(i);
            }
        }
    });
    match picked {
        Some(0) => collapse_all_folders(deck),
        Some(_) => {
            deck.refresh_tree();
            deck.tree.expanded.insert(String::new());
        }
        None => {}
    }
    let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !menu_rect.contains(p)));
    if picked.is_some() || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.data_mut(|d| d.insert_temp(files_menu_key(), None::<Pos2>));
    }
}

/// Draws the 12 px filter (magnifying glass) icon on the painter.
fn paint_filter_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
    let s = Stroke::new(FILES_GLYPH_STROKE, color);
    let c = center + Vec2::new(-1.0, -1.0);
    painter.circle_stroke(c, 3.6, s);
    painter.line_segment([c + Vec2::new(2.6, 2.6), center + Vec2::new(5.0, 5.0)], s);
}

/// Draws the 12 px "changed files only" icon: a "±" (a plus over a minus).
fn paint_changed_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
    let s = Stroke::new(FILES_GLYPH_STROKE, color);
    let plus = center + Vec2::new(0.0, -2.0);
    painter.line_segment([plus + Vec2::new(-3.0, 0.0), plus + Vec2::new(3.0, 0.0)], s);
    painter.line_segment([plus + Vec2::new(0.0, -3.0), plus + Vec2::new(0.0, 3.0)], s);
    let y = center.y + 5.0;
    painter.line_segment([Pos2::new(center.x - 3.0, y), Pos2::new(center.x + 3.0, y)], s);
}

/// The Git letter of a changed file (`M`, `A`, `D`, `R`, `U`, `?`).
fn git_mark(e: &GitEntry) -> char {
    match e.kind {
        EntryKind::Untracked => '?',
        EntryKind::Unmerged => 'U',
        EntryKind::Renamed | EntryKind::Copied => 'R',
        EntryKind::Ordinary => [e.index, e.worktree].into_iter().find(|c| matches!(c, 'A' | 'D')).unwrap_or('M'),
    }
}

/// The Git letter of every changed file of the project, by the path relative to the workspace (the `status`
/// comes with paths relative to the repository root, which may be a folder above the workspace).
fn git_marks(deck: &Deck) -> HashMap<String, char> {
    let Some(st) = deck.git.status() else { return HashMap::new() };
    st.entries
        .iter()
        .filter_map(|e| e.path.strip_prefix(st.prefix.as_str()).map(|rel| (rel.trim_end_matches('/').to_owned(), git_mark(e))))
        .collect()
}

/// Draws the "Collapse all folders" icon on the 12 px grid: a square with "−" and, behind it, the edge of
/// another square (above and to the right).
fn paint_collapse_all(painter: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(FILES_GLYPH_STROKE, color);
    let front = Rect::from_center_size(center + Vec2::new(-1.0, 1.0), Vec2::splat(8.0));
    painter.rect_stroke(front, 1.0, stroke, egui::StrokeKind::Inside);
    painter.line_segment([Pos2::new(front.min.x + 2.0, front.center().y), Pos2::new(front.max.x - 2.0, front.center().y)], stroke);
    let (x0, y0, x1, y1) = (front.min.x + 2.0, front.min.y - 2.0, front.max.x + 2.0, front.max.y - 2.0);
    painter.line_segment([Pos2::new(x0, front.min.y), Pos2::new(x0, y0)], stroke);
    painter.line_segment([Pos2::new(x0, y0), Pos2::new(x1, y0)], stroke);
    painter.line_segment([Pos2::new(x1, y0), Pos2::new(x1, y1)], stroke);
    painter.line_segment([Pos2::new(x1, y1), Pos2::new(front.max.x, y1)], stroke);
}

fn paint_row(deck: &mut Deck, ui: &Ui, rect: Rect, row: &TreeRow, resp: &egui::Response, marks: &HashMap<String, char>) {
    let painter = ui.painter();
    let ctx = ui.ctx();
    let p = theme::pal();
    let dragging = ctx.dragged_id().is_some() || deck.pane_drag.is_some() || deck.drop_hover;
    let hovered = resp.hovered() && !dragging;
    if hovered {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    let is_active = !row.entry.is_dir && deck.tree.active_file.as_deref() == Some(row.entry.rel_path.as_str());
    let mark = marks.get(row.entry.rel_path.as_str()).copied();
    let (bg, fg) = if is_active {
        (Some(p.selected_soft), p.text_strong)
    } else if hovered {
        (Some(p.hover), p.text_strong)
    } else if mark.is_some() {
        (None, p.text)
    } else {
        (None, p.text_2)
    };
    let inner = rect.shrink2(Vec2::new(SIDE_MARGIN, 0.0));
    if let Some(bg) = bg {
        painter.rect_filled(inner, 4.0, bg);
    }
    // The first level of the root has depth 1 in TreeRow: it becomes indent 0.
    let depth = row.depth.saturating_sub(1);
    let x0 = inner.min.x + 8.0 + depth as f32 * FILE_INDENT;
    let cy = rect.center().y;
    // 14 px box: the folder arrow (it turns when opening) or the file type sign, always in gray.
    let box_c = Pos2::new(x0 + 7.0, cy);
    if row.entry.is_dir {
        let t = ctx.animate_bool_with_time(Id::new(("tree-chev", &row.entry.rel_path)), row.expanded, 0.12);
        deck.icons.paint(painter, Icon::TreeChevron, box_c, 10.0, p.text_3, t * std::f32::consts::FRAC_PI_2);
    } else {
        file_icon(deck, painter, Rect::from_center_size(box_c, Vec2::splat(14.0)), &row.entry.name);
    }
    // The Git letter on the right (IBM Plex Mono 10 px).
    let right = inner.max.x - 8.0;
    let mut name_right = right;
    if let Some(m) = mark {
        let g = galley(painter, &m.to_string(), theme::meta(10.0), p.text_2);
        let w = g.size().x;
        place(
            painter,
            Rect::from_min_max(Pos2::new(right - w, rect.min.y), Pos2::new(right, rect.max.y)),
            Align2::LEFT_CENTER,
            g,
            p.text_2,
        );
        name_right = right - w - 6.0;
    }
    // Name with ellipsis.
    let name_x = x0 + 14.0 + 5.0;
    let g = galley_trunc(painter, &row.entry.name, theme::ui(11.5), fg, (name_right - name_x).max(1.0));
    place(painter, Rect::from_min_max(Pos2::new(name_x, rect.min.y), Pos2::new(name_right, rect.max.y)), Align2::LEFT_CENTER, g, fg);
}

fn paint_flat_row(deck: &mut Deck, ui: &Ui, rect: Rect, row: &FlatFileRow, resp: &egui::Response) {
    let painter = ui.painter();
    let ctx = ui.ctx();
    let p = theme::pal();
    let dragging = ctx.dragged_id().is_some() || deck.pane_drag.is_some() || deck.drop_hover;
    let hovered = resp.hovered() && !dragging;
    if hovered {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    let is_active = deck.tree.active_file.as_deref() == Some(row.rel_path.as_str());
    let (bg, fg) = if is_active {
        (Some(p.selected_soft), p.text_strong)
    } else if hovered {
        (Some(p.hover), p.text_strong)
    } else if row.git_mark.is_some() {
        (None, p.text)
    } else {
        (None, p.text_2)
    };
    let inner = rect.shrink2(Vec2::new(SIDE_MARGIN, 0.0));
    if let Some(bg) = bg {
        painter.rect_filled(inner, 4.0, bg);
    }
    let x0 = inner.min.x + 8.0;
    let cy = rect.center().y;
    let box_c = Pos2::new(x0 + 7.0, cy);
    file_icon(deck, painter, Rect::from_center_size(box_c, Vec2::splat(14.0)), &row.name);

    let right = inner.max.x - 8.0;
    let mut name_right = right;
    if let Some(m) = row.git_mark {
        let g = galley(painter, &m.to_string(), theme::meta(10.0), p.text_2);
        let w = g.size().x;
        place(
            painter,
            Rect::from_min_max(Pos2::new(right - w, rect.min.y), Pos2::new(right, rect.max.y)),
            Align2::LEFT_CENTER,
            g,
            p.text_2,
        );
        name_right = right - w - 6.0;
    }

    let name_x = x0 + 14.0 + 5.0;
    let avail = (name_right - name_x).max(1.0);
    let name_max = if row.parent_dir.is_empty() { avail } else { (avail * 0.65).max(48.0).min(avail) };
    let name_g = galley_trunc(painter, &row.name, theme::ui(11.5), fg, name_max);
    let placed_name = place(
        painter,
        Rect::from_min_max(Pos2::new(name_x, rect.min.y), Pos2::new(name_right, rect.max.y)),
        Align2::LEFT_CENTER,
        name_g,
        fg,
    );

    let dir_x = placed_name.max.x + 6.0;
    if !row.parent_dir.is_empty() && name_right - dir_x >= 18.0 {
        let dir_g = galley_trunc(painter, &row.parent_dir, theme::meta(10.0), p.text_3, name_right - dir_x);
        place(
            painter,
            Rect::from_min_max(Pos2::new(dir_x, rect.min.y), Pos2::new(name_right, rect.max.y)),
            Align2::LEFT_CENTER,
            dir_g,
            p.text_3,
        );
    }
}

/// The sign of the file type in the 14 px box, in gray (the user asked for less color).
fn file_icon(deck: &mut Deck, painter: &egui::Painter, rect: Rect, name: &str) {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let ext = if name.contains('.') { ext } else { String::new() };
    match badge_text(&ext) {
        Some(text) => {
            text_in(painter, rect, Align2::CENTER_CENTER, text, theme::mono_bold(8.5), theme::pal().text_3);
        }
        None => deck.icons.paint(painter, Icon::FileGeneric, rect.center(), 12.0, theme::pal().text_3, 0.0),
    }
}

/// The text of the sign of an extension (`None`: generic file icon).
fn badge_text(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "js" | "mjs" | "cjs" => "JS",
        "json" => "{ }",
        "md" | "markdown" => "M↓",
        "ts" | "tsx" => "TS",
        "html" | "htm" => "<>",
        "css" => "#",
        "py" => "Py",
        "rs" => "rs",
        "toml" => "⚙",
        _ => return None,
    })
}

/// `#sidebar-splitter`: dragging resizes between 150 and 480 px.
pub fn splitter(deck: &mut Deck, ui: &mut Ui, line: Rect, full: Rect) {
    let hit = line;
    let r = ui.interact(hit, Id::new("sidebar-splitter"), Sense::drag());
    if r.hovered() || r.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    if r.drag_started() {
        if let Some(p) = r.interact_pointer_pos() {
            deck.sidebar_dragging = Some((p.x, deck.sidebar_w));
        }
    }
    if r.dragged() {
        if let (Some((x0, w0)), Some(p)) = (deck.sidebar_dragging, ui.input(|i| i.pointer.latest_pos())) {
            deck.sidebar_w = (w0 + p.x - x0).clamp(theme::SIDEBAR_MIN_W, theme::SIDEBAR_MAX_W);
        }
    }
    if r.drag_stopped() {
        deck.sidebar_dragging = None;
        deck.persist_layout();
    }
    let hot = r.hovered() || deck.sidebar_dragging.is_some();
    let painter = ui.painter_at(full);
    paint_splitter(&painter, line, hot, deck.accent());
    tip(r, t!("Drag to resize the sidebar"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: EntryKind, index: char, worktree: char) -> GitEntry {
        GitEntry { path: "x".into(), orig_path: None, index, worktree, kind }
    }

    #[test]
    fn the_git_letter_is_the_most_telling_one() {
        assert_eq!(git_mark(&entry(EntryKind::Untracked, '?', '?')), '?');
        assert_eq!(git_mark(&entry(EntryKind::Unmerged, 'U', 'U')), 'U');
        assert_eq!(git_mark(&entry(EntryKind::Renamed, 'R', '.')), 'R');
        assert_eq!(git_mark(&entry(EntryKind::Ordinary, 'A', '.')), 'A');
        assert_eq!(git_mark(&entry(EntryKind::Ordinary, '.', 'D')), 'D');
        assert_eq!(git_mark(&entry(EntryKind::Ordinary, '.', 'M')), 'M');
        // Added and then edited still stays "A".
        assert_eq!(git_mark(&entry(EntryKind::Ordinary, 'A', 'M')), 'A');
    }

    #[test]
    fn projects_max_height_rises_when_short_and_respects_minimums() {
        // Short tree (90 px) with 500 px available and 0.42 ratio (cap 210 px): FILES rises right under it (90 px).
        let (max_h, cap_h) = projects_max_height(500.0, 90.0, 0.42, false);
        assert_eq!(cap_h, 210.0);
        assert_eq!(max_h, 90.0);

        // Tall tree (360 px): capped at 210 px so it scrolls inside.
        let (max_h, _) = projects_max_height(500.0, 360.0, 0.42, false);
        assert_eq!(max_h, 210.0);

        // Ratio too small clamps to MIN_PROJECTS_LIST_H (3 agent rows = 72 px).
        let (_, cap_min) = projects_max_height(500.0, 360.0, 0.05, false);
        assert_eq!(cap_min, MIN_PROJECTS_LIST_H);

        // Ratio too large leaves at least SPLITTER_H + MIN_FILES_H (6 + 84 = 90 px) below.
        let (_, cap_max) = projects_max_height(500.0, 480.0, 0.95, false);
        assert_eq!(cap_max, 500.0 - SPLITTER_H - MIN_FILES_H);

        // When FILES is collapsed, PROJECTS can grow up to avail_h - COLLAPSED_FILES_H.
        let (max_col, cap_col) = projects_max_height(500.0, 450.0, 0.42, true);
        assert_eq!(cap_col, 500.0 - COLLAPSED_FILES_H);
        assert_eq!(max_col, 450.0);
    }

    #[test]
    fn clamp_projects_ratio_enforces_top_and_bottom_minimums() {
        let r_low = clamp_projects_ratio(500.0, 10.0);
        assert!((r_low - MIN_PROJECTS_LIST_H / 500.0).abs() < 1e-4);

        let r_high = clamp_projects_ratio(500.0, 490.0);
        let expected_high = (500.0 - SPLITTER_H - MIN_FILES_H) / 500.0;
        assert!((r_high - expected_high).abs() < 1e-4);
    }

    #[test]
    fn the_collapse_all_slot_needs_room_for_three_buttons() {
        assert!(files_hover_slot_fits(91.0));
        assert!(files_hover_slot_fits(70.0));
        assert!(!files_hover_slot_fits(69.0));
    }

    #[test]
    fn the_header_meta_is_the_changed_count_in_the_changed_only_view() {
        assert_eq!(files_header_meta(false, 3, Some("holt"), Some("dev")), ("holt · dev".to_owned(), false));
        assert_eq!(files_header_meta(true, 3, Some("holt"), Some("dev")), ("3 changed".to_owned(), true));
        assert_eq!(files_header_meta(true, 1, Some("holt"), Some("dev")), ("1 changed".to_owned(), true));
    }

    #[test]
    fn files_context_label_combines_project_and_branch() {
        assert_eq!(files_context_label(Some("holt-dev"), Some("dev")), "holt-dev · dev");
        assert_eq!(files_context_label(Some("holt-dev"), None), "holt-dev");
        assert_eq!(files_context_label(None, Some("dev")), "");
    }

    #[test]
    fn flatten_files_filters_by_query_and_changed_only_and_splits_parent_dir() {
        let files = vec![
            DirEntry { name: "crates".into(), is_dir: true, rel_path: "crates".into(), ext: String::new() },
            DirEntry { name: "sidebar.rs".into(), is_dir: false, rel_path: "crates/app/src/ui/sidebar.rs".into(), ext: ".rs".into() },
            DirEntry { name: "projects.rs".into(), is_dir: false, rel_path: "crates/app/src/ui/projects.rs".into(), ext: ".rs".into() },
            DirEntry { name: "README.md".into(), is_dir: false, rel_path: "README.md".into(), ext: ".md".into() },
        ];
        let marks = HashMap::from([("crates/app/src/ui/sidebar.rs".to_owned(), 'M'), ("deleted.txt".to_owned(), 'D')]);

        // Changed only: includes modified sidebar.rs and deleted.txt.
        let changed = flatten_files(&files, &marks, "", true);
        assert_eq!(
            changed,
            vec![
                FlatFileRow {
                    name: "sidebar.rs".into(),
                    parent_dir: "crates/app/src/ui".into(),
                    rel_path: "crates/app/src/ui/sidebar.rs".into(),
                    git_mark: Some('M'),
                },
                FlatFileRow { name: "deleted.txt".into(), parent_dir: String::new(), rel_path: "deleted.txt".into(), git_mark: Some('D') },
            ]
        );

        // Query filter ("side") across all files.
        let filtered = flatten_files(&files, &marks, "side", false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "sidebar.rs");
        assert_eq!(filtered[0].parent_dir, "crates/app/src/ui");
    }
}
