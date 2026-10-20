//! Terminal tabs: tab bar, split panes, the terminal widget itself
//! (render + keyboard/mouse input + selection + search).
//! Author: Passwd Word

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Vec2};
use opsdeck_ssh::SessionId;
use opsdeck_terminal::input::{KeyMods, TermKey, ctrl_byte, key_to_bytes};

use crate::theme::terminal_defaults;
use crate::{TermTab, UiData, UiEvent, UiState};

/// Draw the terminal tab: tab bar + toolbar + panes.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    // Tab bar + toolbar
    egui::Panel::top("term_toolbar").show(ui, |ui| {
        ui.horizontal(|ui| {
            // Tabs
            for (i, tab) in state.tabs.iter().enumerate() {
                let title = tab_title(tab);
                let active = i == state.active_tab && state.active_desktop.is_none();
                if ui.selectable_label(active, title).clicked() {
                    state.active_tab = i;
                    state.active_desktop = None;
                }
                if ui
                    .small_button("✕")
                    .on_hover_text(state.t("Close tab"))
                    .clicked()
                {
                    let sessions: Vec<SessionId> = tab.panes.iter().map(|p| p.session).collect();
                    for s in sessions {
                        events.push(UiEvent::ClosePane(s));
                    }
                }
                ui.separator();
            }
            if state.tabs.is_empty() {
                ui.label(egui::RichText::new(state.t("No terminal tabs")).weak());
            }
            let mut close_desktop: Option<u64> = None;
            for d in state.desktops.iter() {
                let active = state.active_desktop == Some(d.id);
                let label = format!("🖥 {}", d.title);
                let resp = ui.selectable_label(active, egui::RichText::new(&label).monospace());
                if resp.clicked() {
                    state.active_desktop = Some(d.id);
                }
                if ui
                    .small_button("✕")
                    .on_hover_text(state.t("Close desktop"))
                    .clicked()
                {
                    close_desktop = Some(d.id);
                }
                ui.separator();
            }
            if let Some(id) = close_desktop {
                events.push(UiEvent::CloseDesktop { id });
                state.desktops.retain(|d| d.id != id);
                if state.active_desktop == Some(id) {
                    state.active_desktop = None;
                }
                state.desktop_textures.remove(&id);
                state.desktop_frame_versions.remove(&id);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(state.show_monitor_panel, "📊")
                    .on_hover_text(state.t("Monitor panel"))
                    .clicked()
                {
                    state.show_monitor_panel = !state.show_monitor_panel;
                }
                if ui
                    .selectable_label(state.show_files_panel, "📁")
                    .on_hover_text(state.t("Files panel"))
                    .clicked()
                {
                    state.show_files_panel = !state.show_files_panel;
                }
                if ui
                    .small_button("🔍")
                    .on_hover_text(state.t("Search terminal (Ctrl+F)"))
                    .clicked()
                {
                    toggle_search(state);
                }
                if ui
                    .small_button("⬒")
                    .on_hover_text(state.t("Split left/right (Ctrl+Shift+E)"))
                    .clicked()
                {
                    events.push(UiEvent::SplitPane { vertical: false });
                }
                if ui
                    .small_button("⬓")
                    .on_hover_text(state.t("Split up/down (Ctrl+Shift+O)"))
                    .clicked()
                {
                    events.push(UiEvent::SplitPane { vertical: true });
                }
                if ui
                    .small_button("＋")
                    .on_hover_text(state.t("New tab (Ctrl+T)"))
                    .clicked()
                {
                    new_tab_for_selection(state, events);
                }
            });
        });
    });

    // Session side panels (Xterminal-style): files left, monitor right.
    let active_host = if state.active_desktop.is_some() {
        None
    } else {
        state
            .active_pane()
            .map(|p| p.host_id.clone())
            .filter(|h| !h.is_empty())
    };
    if let Some(host_id) = active_host {
        if state.show_files_panel {
            super::filespanel::show(ui, state, data, events, &host_id);
        }
        if state.show_monitor_panel {
            super::dashboard::show(ui, state, data, &host_id);
        }
    }

    // Panes (or the active remote desktop).
    egui::CentralPanel::default().show(ui, |ui| {
        if let Some(desktop_id) = state.active_desktop {
            super::desktop::show(ui, state, data, events, desktop_id);
            return;
        }
        if state.tabs.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(state.t("No open sessions")).size(18.0));
                    ui.label(
                        state
                            .t("Select a host on the left and press ▶ or double-click to connect."),
                    );
                    ui.add_space(8.0);
                    if ui.button(state.t("＋ New tab")).clicked() {
                        new_tab_for_selection(state, events);
                    }
                });
            });
            return;
        }
        let tab_idx = state.active_tab.min(state.tabs.len() - 1);
        state.active_tab = tab_idx;
        let vertical = state.tabs[tab_idx].vertical;
        let pane_count = state.tabs[tab_idx].panes.len();

        // Search bar overlay (per active pane).
        if let Some(pane) = state.tabs[tab_idx]
            .panes
            .get(state.tabs[tab_idx].active_pane)
            .cloned()
            && state
                .search_open
                .get(&pane.session)
                .copied()
                .unwrap_or(false)
        {
            search_bar(ui, state, data, events, pane.session);
        }

        if vertical {
            ui.vertical(|ui| {
                for pane_idx in 0..pane_count {
                    let (session, title) = {
                        let p = &state.tabs[tab_idx].panes[pane_idx];
                        (p.session, p.title.clone())
                    };
                    let h = ui.available_height() / (pane_count - pane_idx) as f32;
                    let rect =
                        Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), h));
                    pane_widget(
                        ui, state, data, events, tab_idx, pane_idx, session, &title, rect,
                    );
                }
            });
        } else {
            ui.horizontal(|ui| {
                for pane_idx in 0..pane_count {
                    let (session, title) = {
                        let p = &state.tabs[tab_idx].panes[pane_idx];
                        (p.session, p.title.clone())
                    };
                    let w = ui.available_width() / (pane_count - pane_idx) as f32;
                    let rect =
                        Rect::from_min_size(ui.cursor().min, Vec2::new(w, ui.available_height()));
                    pane_widget(
                        ui, state, data, events, tab_idx, pane_idx, session, &title, rect,
                    );
                    if pane_idx + 1 < pane_count {
                        ui.separator();
                    }
                }
            });
        }
    });
}

fn tab_title(tab: &TermTab) -> String {
    let names: Vec<&str> = tab.panes.iter().map(|p| p.title.as_str()).collect();
    let joined = names.join(" ⫿ ");
    if joined.chars().count() > 32 {
        format!("{}…", joined.chars().take(31).collect::<String>())
    } else {
        joined
    }
}

fn new_tab_for_selection(state: &mut UiState, events: &mut Vec<UiEvent>) {
    if let Some(host_id) = state.selected_host.clone() {
        events.push(UiEvent::NewTabForHost(host_id));
    } else {
        state.toast(state.t("Select a host on the left first"), true);
    }
}

fn toggle_search(state: &mut UiState) {
    if let Some(pane) = state.active_pane().cloned() {
        let open = !state
            .search_open
            .get(&pane.session)
            .copied()
            .unwrap_or(false);
        state.search_open.insert(pane.session, open);
        if open {
            state.search_query.entry(pane.session).or_default();
        }
    }
}

/// The floating search bar.
fn search_bar(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    session: SessionId,
) {
    let _ = events;
    let _ = data;
    egui::Area::new(egui::Id::unique(("term_search", session)))
        .anchor(Align2::RIGHT_TOP, Vec2::new(-12.0, 44.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgb(30, 30, 30))
                .stroke(Stroke::new(1.0, Color32::from_rgb(90, 90, 90)))
                .corner_radius(6.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let hint = state.t("Search terminal");
                        let query = state.search_query.entry(session).or_default();
                        let resp = ui.add(
                            egui::TextEdit::singleline(query)
                                .hint_text(hint)
                                .desired_width(200.0),
                        );
                        resp.request_focus();
                        let hits = state
                            .terminals
                            .get(&session)
                            .map(|t| t.search(query))
                            .unwrap_or_default();
                        ui.label(format!("{}/{}", 0, hits.len()));
                        if ui.small_button("▲").clicked() {
                            jump_hit(state, session, -1);
                        }
                        if ui.small_button("▼").clicked()
                            || (resp.lost_focus()
                                && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            jump_hit(state, session, 1);
                        }
                        if ui.small_button("✕").clicked() {
                            state.search_open.insert(session, false);
                        }
                    });
                });
        });
}

/// Jump to the next/previous search hit.
fn jump_hit(state: &mut UiState, session: SessionId, dir: i32) {
    let query = state
        .search_query
        .get(&session)
        .cloned()
        .unwrap_or_default();
    if query.is_empty() {
        return;
    }
    if let Some(term) = state.terminals.get_mut(&session) {
        let hits = term.search(&query);
        if hits.is_empty() {
            return;
        }
        // Cycle through hits with a per-session cursor.
        let cursor_key = session.as_u128() as u64;
        let idx = state
            .search_hit_cursor
            .get(&cursor_key)
            .copied()
            .unwrap_or(0);
        let next = ((idx as i32 + dir).rem_euclid(hits.len() as i32)) as usize;
        state.search_hit_cursor.insert(cursor_key, next);
        term.scroll_to_grid_line(hits[next].0);
    }
}

// ---------------------------------------------------------------------------
// The terminal widget
// ---------------------------------------------------------------------------

/// Draw one terminal pane inside `rect`.
#[allow(clippy::too_many_arguments)]
fn pane_widget(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    tab_idx: usize,
    pane_idx: usize,
    session: SessionId,
    title: &str,
    rect: Rect,
) {
    let theme = data.theme;
    let font_id = FontId::new(theme.terminal_font_size, FontFamily::Monospace);

    // Cell metrics from a sample run.
    let sample =
        ui.painter()
            .layout_no_wrap("MMMMMMMMMM".to_owned(), font_id.clone(), Color32::WHITE);
    let cell_w = (sample.rect.width() / 10.0).max(4.0);
    let cell_h = sample.rect.height().max(8.0);

    let cols = ((rect.width() / cell_w).floor() as usize).clamp(20, 500);
    let rows = ((rect.height() / cell_h).floor() as usize).clamp(5, 400);

    // Resize: inform the backend when the grid size changed.
    if state.term_sizes.get(&session).copied() != Some((cols, rows)) {
        state.term_sizes.insert(session, (cols, rows));
        if let Some(term) = state.terminals.get_mut(&session) {
            term.resize(cols, rows);
        }
        events.push(UiEvent::TerminalResize(session, cols as u16, rows as u16));
    }

    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    if response.clicked() {
        response.request_focus();
        state.tabs[tab_idx].active_pane = pane_idx;
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.terminal_bg());

    let (def_fg, def_bg) = terminal_defaults(theme);
    let egui_color = |c: opsdeck_terminal::RgbColor| Color32::from_rgb(c.r, c.g, c.b);

    // --- selection overlay helpers -------------------------------------
    let pos_to_cell = |pos: Pos2| -> (usize, usize) {
        let c = ((pos.x - rect.left()) / cell_w).floor() as isize;
        let r = ((pos.y - rect.top()) / cell_h).floor() as isize;
        (
            r.clamp(0, rows as isize - 1).max(0) as usize,
            c.clamp(0, cols as isize - 1).max(0) as usize,
        )
    };

    // Mouse: selection drag.
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let cell = pos_to_cell(pos);
            state.selections.insert(
                session,
                crate::Selection {
                    anchor: cell,
                    head: cell,
                },
            );
        }
    } else if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let cell = pos_to_cell(pos);
            if let Some(sel) = state.selections.get_mut(&session) {
                sel.head = cell;
            }
        }
    } else if response.drag_stopped() {
        // Click without drag clears the selection.
        if let Some(sel) = state.selections.get(&session)
            && sel.anchor == sel.head
        {
            state.selections.remove(&session);
        }
    }

    // --- draw grid ------------------------------------------------------
    if let Some(term) = state.terminals.get(&session) {
        let rendered = term.render(def_fg, def_bg);

        // Selection highlight (under text).
        if let Some(sel) = state.selections.get(&session) {
            let ((r1, c1), (r2, c2)) = sel.normalized();
            let hl = Color32::from_rgba_unmultiplied(70, 130, 220, 110);
            for r in r1..=r2.min(rows.saturating_sub(1)) {
                let cs = if r == r1 { c1 } else { 0 };
                let ce = if r == r2 { c2.saturating_add(1) } else { cols };
                if ce > cs {
                    let x = rect.left() + cs as f32 * cell_w;
                    let y = rect.top() + r as f32 * cell_h;
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(x, y),
                            Vec2::new((ce - cs) as f32 * cell_w, cell_h),
                        ),
                        0.0,
                        hl,
                    );
                }
            }
        }

        for (r, line) in rendered.lines.iter().enumerate() {
            if r >= rows {
                break;
            }
            let y = rect.top() + r as f32 * cell_h;
            for (si, span) in line.spans.iter().enumerate() {
                let start = span.start_col.min(cols);
                let end = if si + 1 < line.spans.len() {
                    line.spans[si + 1].start_col.min(cols)
                } else {
                    cols
                };
                if end <= start || span.text.is_empty() {
                    continue;
                }
                let x = rect.left() + start as f32 * cell_w;
                let w = (end - start) as f32 * cell_w;
                let fg = egui_color(span.fg);
                if span.bg != def_bg {
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, cell_h)),
                        0.0,
                        egui_color(span.bg),
                    );
                }
                painter.text(
                    Pos2::new(x, y),
                    Align2::LEFT_TOP,
                    &span.text,
                    font_id.clone(),
                    fg,
                );
                if span.bold {
                    // Faux bold: second pass slightly offset.
                    painter.text(
                        Pos2::new(x + cell_w * 0.06, y),
                        Align2::LEFT_TOP,
                        &span.text,
                        font_id.clone(),
                        fg,
                    );
                }
                if span.underline {
                    painter.line_segment(
                        [
                            Pos2::new(x, y + cell_h - 2.0),
                            Pos2::new(x + w, y + cell_h - 2.0),
                        ],
                        Stroke::new(1.0, fg),
                    );
                }
            }
        }

        // Cursor.
        let (cr, cc) = rendered.cursor;
        if rendered.cursor_visible && cr < rows && cc < cols {
            let x = rect.left() + cc as f32 * cell_w;
            let y = rect.top() + cr as f32 * cell_h;
            let cursor_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h));
            let cursor_color = egui_color(def_fg);
            if response.has_focus() {
                painter.rect_filled(cursor_rect, 0.0, cursor_color);
            } else {
                painter.rect_stroke(
                    cursor_rect,
                    0.0,
                    Stroke::new(1.0, cursor_color),
                    egui::StrokeKind::Outside,
                );
            }
        }

        // If the remote closed the session, show a notice.
        if let Some(info) = data.session(&session) {
            use opsdeck_ssh::SessionState;
            if info.state == SessionState::Disconnected || info.state == SessionState::Failed {
                let msg = info
                    .last_error
                    .clone()
                    .unwrap_or_else(|| state.t("Session closed.").to_string());
                painter.text(
                    Pos2::new(rect.left() + 8.0, rect.bottom() - cell_h - 4.0),
                    Align2::LEFT_TOP,
                    state.tf("— {msg} [press ↻ to reconnect]", &[("msg", &msg)]),
                    font_id.clone(),
                    Color32::from_rgb(255, 150, 120),
                );
            }
        }
    } else {
        // No terminal yet: show a spinner / connecting state.
        painter.text(
            Pos2::new(rect.left() + 12.0, rect.top() + 12.0),
            Align2::LEFT_TOP,
            state.tf("Connecting to {title}…", &[("title", title)]),
            font_id.clone(),
            theme.weak_text(),
        );
    }

    // --- scroll ----------------------------------------------------------
    if response.hovered() {
        let delta_y = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if delta_y != 0.0
            && let Some(term) = state.terminals.get_mut(&session)
        {
            term.scroll((delta_y / cell_h) as i32);
        }
    }

    // --- keyboard --------------------------------------------------------
    if response.has_focus() {
        let mut out: Vec<u8> = Vec::new();
        let mut copy_now = false;
        let mut clear_search = false;

        ui.ctx().input(|i| {
            for ev in &i.events {
                match ev {
                    egui::Event::Text(text) => {
                        // (Ctrl combinations arrive as Key events, not Text.)
                        out.extend_from_slice(text.as_bytes());
                    }
                    egui::Event::Paste(text) => {
                        out.extend_from_slice(text.as_bytes());
                    }
                    egui::Event::Copy => {
                        copy_now = true;
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        handle_key(
                            state,
                            session,
                            *key,
                            *modifiers,
                            &mut out,
                            &mut copy_now,
                            &mut clear_search,
                            events,
                            data,
                        );
                    }
                    _ => {}
                }
            }
        });

        if copy_now {
            copy_selection(ui, state, session);
        }
        if clear_search {
            state.search_open.insert(session, false);
        }
        if !out.is_empty() {
            events.push(UiEvent::TerminalInput(session, out));
        }
    }
}

/// Handle one key press while a terminal has focus.
/// Returns `true` when the key was fully handled (nothing else to do).
#[allow(clippy::too_many_arguments)]
fn handle_key(
    state: &mut UiState,
    session: SessionId,
    key: egui::Key,
    modifiers: egui::Modifiers,
    out: &mut Vec<u8>,
    copy_now: &mut bool,
    clear_search: &mut bool,
    _events: &mut Vec<UiEvent>,
    data: &UiData,
) {
    use egui::Key;
    let _ = data;

    // Global shortcuts win over terminal input.
    if is_global_shortcut(key, modifiers) {
        return;
    }

    let has_selection = state.selections.contains_key(&session);

    // Copy: Ctrl+Shift+C / Cmd+C (with selection) / Event::Copy.
    if (modifiers.ctrl && modifiers.shift && key == Key::C)
        || (modifiers.mac_cmd && !modifiers.ctrl && key == Key::C)
    {
        if has_selection {
            *copy_now = true;
        } else if modifiers.ctrl {
            out.push(0x03); // Ctrl+C → SIGINT
        }
        return;
    }
    // Paste via Ctrl+Shift+V: egui has no clipboard-read API; the standard
    // Ctrl+V / Cmd+V arrives as Event::Paste and is handled there.
    if (modifiers.ctrl || modifiers.mac_cmd) && key == Key::V {
        return; // swallow: Paste event carries the content (or nothing)
    }

    // Search toggle.
    if modifiers.command && key == Key::F {
        let open = !state.search_open.get(&session).copied().unwrap_or(false);
        state.search_open.insert(session, open);
        return;
    }
    if key == Key::Escape && state.search_open.get(&session).copied().unwrap_or(false) {
        *clear_search = true;
        return;
    }

    // Ctrl+letter → control byte.
    if modifiers.ctrl
        && !modifiers.alt
        && !modifiers.shift
        && let Some(b) = ctrl_letter(key)
    {
        out.push(b);
        return;
    }

    // Alt+letter → ESC + letter (meta).
    if modifiers.alt
        && !modifiers.ctrl
        && !modifiers.mac_cmd
        && let Some(c) = key_letter(key)
    {
        out.push(0x1b);
        out.extend_from_slice(c.to_lowercase().to_string().as_bytes());
        return;
    }

    // Special keys.
    let mods = KeyMods {
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        shift: modifiers.shift,
    };
    if let Some(term_key) = map_special(key) {
        // Shift+PageUp/PageDown scroll instead of sending bytes.
        if modifiers.shift && (term_key == TermKey::PageUp || term_key == TermKey::PageDown) {
            if let Some(term) = state.terminals.get_mut(&session) {
                let (_, rows) = state.term_sizes.get(&session).copied().unwrap_or((80, 24));
                let delta = rows as i32 - 1;
                term.scroll(if term_key == TermKey::PageUp {
                    delta
                } else {
                    -delta
                });
            }
            return;
        }
        out.extend_from_slice(&key_to_bytes(term_key, mods));
        return;
    }

    // Shift+Home/End scroll to top/bottom.
    if modifiers.shift && key == Key::Home {
        if let Some(term) = state.terminals.get_mut(&session) {
            term.scroll(i32::MAX / 2);
        }
        return;
    }
    if modifiers.shift
        && key == Key::End
        && let Some(term) = state.terminals.get_mut(&session)
    {
        term.scroll_to_bottom();
    }
}

/// Keys reserved for global shortcuts (handled in views::main).
fn is_global_shortcut(key: egui::Key, m: egui::Modifiers) -> bool {
    use egui::Key;
    if m.command && !m.shift {
        matches!(key, Key::T | Key::W | Key::P)
    } else if m.command && m.shift {
        matches!(key, Key::E | Key::O | Key::F | Key::M | Key::C | Key::V)
    } else {
        false
    }
}

/// Map egui special keys to terminal keys.
fn map_special(key: egui::Key) -> Option<TermKey> {
    use egui::Key;
    Some(match key {
        Key::Enter => TermKey::Enter,
        Key::Tab => TermKey::Tab,
        Key::Backspace => TermKey::Backspace,
        Key::Delete => TermKey::Delete,
        Key::Insert => TermKey::Insert,
        Key::Escape => TermKey::Escape,
        Key::ArrowUp => TermKey::Up,
        Key::ArrowDown => TermKey::Down,
        Key::ArrowLeft => TermKey::Left,
        Key::ArrowRight => TermKey::Right,
        Key::Home => TermKey::Home,
        Key::End => TermKey::End,
        Key::PageUp => TermKey::PageUp,
        Key::PageDown => TermKey::PageDown,
        Key::F1 => TermKey::F(1),
        Key::F2 => TermKey::F(2),
        Key::F3 => TermKey::F(3),
        Key::F4 => TermKey::F(4),
        Key::F5 => TermKey::F(5),
        Key::F6 => TermKey::F(6),
        Key::F7 => TermKey::F(7),
        Key::F8 => TermKey::F(8),
        Key::F9 => TermKey::F(9),
        Key::F10 => TermKey::F(10),
        Key::F11 => TermKey::F(11),
        Key::F12 => TermKey::F(12),
        _ => return None,
    })
}

/// Ctrl+letter → control byte (None for handled shortcuts).
fn ctrl_letter(key: egui::Key) -> Option<u8> {
    use egui::Key;
    let c = match key {
        Key::A => b'a',
        Key::B => b'b',
        Key::C => b'c',
        Key::D => b'd',
        Key::E => b'e',
        Key::F => b'f',
        Key::G => b'g',
        Key::H => b'h',
        Key::I => b'i',
        Key::J => b'j',
        Key::K => b'k',
        Key::L => b'l',
        Key::M => b'm',
        Key::N => b'n',
        Key::O => b'o',
        Key::P => b'p',
        Key::Q => b'q',
        Key::R => b'r',
        Key::S => b's',
        Key::T => b't',
        Key::U => b'u',
        Key::V => b'v',
        Key::W => b'w',
        Key::X => b'x',
        Key::Y => b'y',
        Key::Z => b'z',
        Key::OpenBracket => b'[',
        Key::Backslash => b'\\',
        Key::CloseBracket => b']',
        _ => return None,
    };
    ctrl_byte(c as char)
}

fn key_letter(key: egui::Key) -> Option<char> {
    use egui::Key;
    Some(match key {
        Key::A => 'a',
        Key::B => 'b',
        Key::C => 'c',
        Key::D => 'd',
        Key::E => 'e',
        Key::F => 'f',
        Key::G => 'g',
        Key::H => 'h',
        Key::I => 'i',
        Key::J => 'j',
        Key::K => 'k',
        Key::L => 'l',
        Key::M => 'm',
        Key::N => 'n',
        Key::O => 'o',
        Key::P => 'p',
        Key::Q => 'q',
        Key::R => 'r',
        Key::S => 's',
        Key::T => 't',
        Key::U => 'u',
        Key::V => 'v',
        Key::W => 'w',
        Key::X => 'x',
        Key::Y => 'y',
        Key::Z => 'z',
        _ => return None,
    })
}

/// Copy the current selection to the clipboard.
fn copy_selection(ui: &mut egui::Ui, state: &mut UiState, session: SessionId) {
    let Some(sel) = state.selections.get(&session).copied() else {
        return;
    };
    let Some(term) = state.terminals.get(&session) else {
        return;
    };
    let ((r1, c1), (r2, c2)) = sel.normalized();
    // Colors don't matter for text extraction; pass dummies.
    let rendered = term.render(
        opsdeck_terminal::RgbColor::new(255, 255, 255),
        opsdeck_terminal::RgbColor::new(0, 0, 0),
    );
    let mut text = String::new();
    for (i, r) in (r1..=r2).enumerate() {
        if r >= rendered.lines.len() {
            break;
        }
        if i > 0 {
            text.push('\n');
        }
        let line = &rendered.lines[r];
        let cs = if r == r1 { c1 } else { 0 };
        let ce = if r == r2 {
            c2.saturating_add(1)
        } else {
            usize::MAX
        };
        let mut line_text = String::new();
        for span in &line.spans {
            for (j, ch) in span.text.chars().enumerate() {
                let cc = span.start_col + j;
                if cc >= cs && cc < ce {
                    line_text.push(ch);
                }
            }
        }
        // Trim trailing whitespace per line (but keep the newline structure).
        text.push_str(line_text.trim_end());
    }
    if !text.trim().is_empty() {
        ui.ctx().copy_text(text);
    }
}
