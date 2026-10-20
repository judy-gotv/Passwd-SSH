//! Session file workspace (Xterminal-style): left directory tree with
//! full-row selection and small chevrons, rich context menus, and the
//! built-in editor rendered as tabs in the center (above the terminal).
//! Author: Passwd Word

use std::time::Instant;

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Vec2};
use opsdeck_sftp::{FileEntry, FileKind};

use crate::{FileEditor, MainTab, TextPrompt, UiData, UiEvent, UiState};

const MAX_TREE_DEPTH: usize = 12;
/// Xterminal brand green (selection, save button).
pub(crate) const XGREEN: Color32 = Color32::from_rgb(40, 174, 96);
const ROW_H: f32 = 22.0;

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Draw the file workspace panel for the active pane's host.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host_id: &str,
) {
    egui::Panel::left("term_files")
        .default_size(300.0)
        .size_range(220.0..=520.0)
        .show(ui, |ui| {
            let current = state.sftp_path(host_id);
            // Header: workspace title + collapse-all / open-in-Files.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(state.t("File workspace")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_btn(ui, "↗")
                        .on_hover_text(state.t("Open in Files"))
                        .clicked()
                    {
                        state.main_tab = MainTab::Files;
                        state.sftp_host = Some(host_id.to_string());
                    }
                    if icon_btn(ui, "⤢")
                        .on_hover_text(state.t("Collapse all"))
                        .clicked()
                    {
                        state.files_expanded.remove(host_id);
                    }
                });
            });
            toolbar(ui, state, events, host_id, &current);
            if state.files_upload_open {
                upload_row(ui, state, events, host_id, &current);
            }
            breadcrumb(ui, state, events, host_id, &current);
            ui.separator();

            ensure_listing(state, data, events, host_id, "/");
            if current != "/" {
                ensure_listing(state, data, events, host_id, &current);
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                tree_level(ui, state, data, events, host_id, "/", 0);
            });
        });
}

/// A borderless toolbar icon button.
fn icon_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(text).size(14.0)).frame(false))
}

/// Action toolbar: refresh / upload / download / edit / new / delete.
fn toolbar(
    ui: &mut egui::Ui,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    current: &str,
) {
    let selected = state.files_selected.get(host_id).cloned();
    ui.horizontal(|ui| {
        if icon_btn(ui, "🔄")
            .on_hover_text(state.t("Refresh"))
            .clicked()
        {
            events.push(UiEvent::SftpNavigate {
                host_id: host_id.to_string(),
                path: current.to_string(),
            });
        }
        if icon_btn(ui, "⬆").on_hover_text(state.t("Upload")).clicked() {
            state.files_upload_open = !state.files_upload_open;
            state.files_upload_target = None;
        }
        if icon_btn(ui, "⬇")
            .on_hover_text(state.t("Download"))
            .clicked()
        {
            match &selected {
                Some((path, false)) => events.push(UiEvent::SftpDownloadToDownloads {
                    host_id: host_id.to_string(),
                    remote: path.clone(),
                }),
                _ => state.toast(state.t("Select a file to download"), true),
            }
        }
        if icon_btn(ui, "✏️").on_hover_text(state.t("Edit")).clicked() {
            match &selected {
                Some((path, false)) => open_editor(state, events, host_id, path),
                _ => state.toast(state.t("Select a file to edit"), true),
            }
        }
        ui.menu_button("➕", |ui| {
            if ui.button(state.t("New folder…")).clicked() {
                state.text_prompt = Some(TextPrompt {
                    title: state.t("New folder").to_string(),
                    value: String::new(),
                    action: format!("sftp_mkdir:{host_id}:{current}"),
                });
                ui.close();
            }
            if ui.button(state.t("New file…")).clicked() {
                state.text_prompt = Some(TextPrompt {
                    title: state.t("New file").to_string(),
                    value: String::new(),
                    action: format!("sftp_newfile:{host_id}:{current}"),
                });
                ui.close();
            }
        })
        .response
        .on_hover_text(state.t("New"));
        if icon_btn(ui, "🗑").on_hover_text(state.t("Delete")).clicked() {
            match &selected {
                Some((path, _)) => {
                    state.confirm = Some(crate::ConfirmDialog {
                        title: state.t("Delete").to_string(),
                        message: state.tf("Delete {path}?", &[("path", path)]),
                        confirm_label: state.t("Delete").to_string(),
                        action: format!("sftp_delete:{host_id}:{path}"),
                    });
                }
                None => state.toast(state.t("Select a file or folder first"), true),
            }
        }
    });
}

/// Inline upload row: type a local path, upload into the target directory.
fn upload_row(
    ui: &mut egui::Ui,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    current: &str,
) {
    let target = state
        .files_upload_target
        .clone()
        .unwrap_or_else(|| current.to_string());
    ui.horizontal(|ui| {
        let hint = state.t("/path/to/local/file");
        ui.add(
            egui::TextEdit::singleline(&mut state.sftp_upload_local)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        );
        if ui.small_button(state.t("⬆ Upload")).clicked() && !state.sftp_upload_local.is_empty() {
            let local = state.sftp_upload_local.clone();
            let base = local
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("upload.bin")
                .to_string();
            events.push(UiEvent::SftpUpload {
                host_id: host_id.to_string(),
                local,
                remote: join_path(&target, &base),
            });
            state.sftp_upload_local.clear();
            state.files_upload_open = false;
            state.files_upload_target = None;
        }
    });
    ui.label(
        egui::RichText::new(state.tf("Upload into {dir}", &[("dir", &target)]))
            .small()
            .weak(),
    );
}

/// Clickable breadcrumb for the current directory.
fn breadcrumb(
    ui: &mut egui::Ui,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    current: &str,
) {
    ui.horizontal_wrapped(|ui| {
        if current == "." {
            ui.label(egui::RichText::new("🏠 ~").small().weak());
            return;
        }
        if ui
            .small_button("🏠 /")
            .on_hover_text(state.t("Root directory"))
            .clicked()
        {
            goto_dir(state, events, host_id, "/");
        }
        let mut acc = String::new();
        for seg in current.split('/').filter(|s| !s.is_empty()) {
            acc.push('/');
            acc.push_str(seg);
            let target = acc.clone();
            ui.label(egui::RichText::new("›").small().weak());
            if ui.small_button(seg).clicked() {
                goto_dir(state, events, host_id, &target);
            }
        }
    });
}

/// Set the current directory and load it.
fn goto_dir(state: &mut UiState, events: &mut Vec<UiEvent>, host_id: &str, path: &str) {
    state
        .sftp_paths
        .insert(host_id.to_string(), path.to_string());
    state
        .files_expanded
        .entry(host_id.to_string())
        .or_default()
        .insert(path.to_string());
    events.push(UiEvent::SftpNavigate {
        host_id: host_id.to_string(),
        path: path.to_string(),
    });
}

/// Request a directory listing when missing (5s retry cooldown per path).
fn ensure_listing(
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    path: &str,
) {
    let key = (host_id.to_string(), path.to_string());
    let needs = match data.sftp_listings.get(&key) {
        None => true,
        Some(l) => l.error.is_some() && !l.loading,
    };
    if !needs {
        return;
    }
    let stamp_key = format!("{host_id}\n{path}");
    let due = state
        .files_panel_requested
        .get(&stamp_key)
        .is_none_or(|t| t.elapsed().as_secs() >= 5);
    if due {
        state
            .files_panel_requested
            .insert(stamp_key, Instant::now());
        events.push(UiEvent::SftpNavigate {
            host_id: host_id.to_string(),
            path: path.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

/// One tree level: full-width rows with painted chevrons.
fn tree_level(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    dir: &str,
    depth: usize,
) {
    let key = (host_id.to_string(), dir.to_string());
    let Some(listing) = data.sftp_listings.get(&key) else {
        ensure_listing(state, data, events, host_id, dir);
        if depth == 0 {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(state.t("Loading…"));
            });
        }
        return;
    };
    if listing.loading && listing.entries.is_empty() {
        ui.label(egui::RichText::new(state.t("Loading…")).small().weak());
        return;
    }
    if let Some(err) = &listing.error
        && listing.entries.is_empty()
    {
        ui.colored_label(Color32::from_rgb(255, 120, 120), err);
        return;
    }
    let mut entries: Vec<FileEntry> = listing.entries.clone();
    entries.sort_by(|a, b| {
        b.is_dir()
            .cmp(&a.is_dir())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    if entries.is_empty() && depth == 0 {
        ui.label(egui::RichText::new(state.t("(empty)")).small().weak());
    }
    for entry in entries {
        tree_row(ui, state, data, events, host_id, &entry, depth);
        let is_dir = entry.is_dir();
        let expanded = state
            .files_expanded
            .get(host_id)
            .is_some_and(|set| set.contains(&entry.path));
        if is_dir && expanded && depth < MAX_TREE_DEPTH {
            tree_level(ui, state, data, events, host_id, &entry.path, depth + 1);
        }
    }
}

/// A single tree row: [indent][chevron][icon][name], full-width hit area.
fn tree_row(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    entry: &FileEntry,
    depth: usize,
) {
    let is_dir = entry.is_dir();
    let expanded = state
        .files_expanded
        .get(host_id)
        .is_some_and(|set| set.contains(&entry.path));
    let selected = state
        .files_selected
        .get(host_id)
        .is_some_and(|(p, _)| p == &entry.path);
    let weak = ui.visuals().weak_text_color();
    let text_color = if selected {
        Color32::WHITE
    } else {
        ui.visuals().text_color()
    };

    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 4.0, XGREEN);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
    }

    let mut x = rect.left() + depth as f32 * 16.0 + 2.0;
    // Chevron slot (dirs only): a small painted triangle.
    let chevron_rect = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(16.0, ROW_H));
    if is_dir {
        let c = Pos2::new(x + 8.0, rect.center().y);
        let pts = if expanded {
            vec![
                Pos2::new(c.x - 4.0, c.y - 2.5),
                Pos2::new(c.x + 4.0, c.y - 2.5),
                Pos2::new(c.x, c.y + 3.5),
            ]
        } else {
            vec![
                Pos2::new(c.x - 2.5, c.y - 4.5),
                Pos2::new(c.x - 2.5, c.y + 4.5),
                Pos2::new(c.x + 3.5, c.y),
            ]
        };
        let chev_color = if selected { Color32::WHITE } else { weak };
        painter.add(egui::Shape::convex_polygon(
            pts,
            chev_color,
            egui::Stroke::NONE,
        ));
    }
    x += 16.0;
    // Icon + name.
    let icon = if is_dir {
        "📁"
    } else if entry.kind == FileKind::Symlink {
        "🔗"
    } else {
        "📄"
    };
    let font = FontId::new(13.0, FontFamily::Proportional);
    painter.text(
        Pos2::new(x, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{icon} {}", entry.name),
        font,
        text_color,
    );
    let _ = chevron_rect;

    resp.clone().on_hover_text(format!(
        "{}\n{}",
        entry.path,
        super::dashboard::fmt_bytes(entry.size as i64)
    ));
    resp.context_menu(|ui| entry_menu(ui, state, events, host_id, entry));

    // Chevron click zone: the left 18px toggles expansion for directories.
    let chevron_clicked = is_dir
        && resp.clicked()
        && resp
            .interact_pointer_pos()
            .is_some_and(|p| p.x < rect.left() + depth as f32 * 16.0 + 20.0);
    if chevron_clicked {
        toggle_expand(state, data, events, host_id, entry);
    } else if resp.double_clicked() {
        if is_dir {
            state
                .files_selected
                .insert(host_id.to_string(), (entry.path.clone(), true));
            goto_dir(state, events, host_id, &entry.path);
        } else {
            state
                .files_selected
                .insert(host_id.to_string(), (entry.path.clone(), false));
            open_editor(state, events, host_id, &entry.path);
        }
    } else if resp.clicked() {
        state
            .files_selected
            .insert(host_id.to_string(), (entry.path.clone(), is_dir));
        if is_dir {
            goto_dir(state, events, host_id, &entry.path);
        }
    }
}

fn toggle_expand(
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    entry: &FileEntry,
) {
    let now_expanded = {
        let set = state.files_expanded.entry(host_id.to_string()).or_default();
        if set.contains(&entry.path) {
            set.remove(&entry.path);
            false
        } else {
            set.insert(entry.path.clone());
            true
        }
    };
    if now_expanded {
        ensure_listing(state, data, events, host_id, &entry.path);
    }
}

/// Right-click menu (Xterminal layout): refresh / upload / new ▸ /
/// download / open with ▸ / rename / copy path / terminal ▸ / more ▸ /
/// delete.
fn entry_menu(
    ui: &mut egui::Ui,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    entry: &FileEntry,
) {
    let is_dir = entry.is_dir();
    let entry_dir = if is_dir {
        entry.path.clone()
    } else {
        parent_path(&entry.path)
    };
    if ui.button(format!("🔄 {}", state.t("Refresh"))).clicked() {
        events.push(UiEvent::SftpNavigate {
            host_id: host_id.to_string(),
            path: entry_dir.clone(),
        });
        ui.close();
    }
    if ui.button(format!("⬆ {}", state.t("Upload"))).clicked() {
        state.files_upload_open = true;
        state.files_upload_target = Some(entry_dir.clone());
        ui.close();
    }
    ui.menu_button(format!("📁 {}", state.t("New")), |ui| {
        if ui.button(state.t("New folder…")).clicked() {
            state.text_prompt = Some(TextPrompt {
                title: state.t("New folder").to_string(),
                value: String::new(),
                action: format!("sftp_mkdir:{host_id}:{entry_dir}"),
            });
            ui.close();
        }
        if ui.button(state.t("New file…")).clicked() {
            state.text_prompt = Some(TextPrompt {
                title: state.t("New file").to_string(),
                value: String::new(),
                action: format!("sftp_newfile:{host_id}:{entry_dir}"),
            });
            ui.close();
        }
    });
    if !is_dir && ui.button(format!("⬇ {}", state.t("Download"))).clicked() {
        events.push(UiEvent::SftpDownloadToDownloads {
            host_id: host_id.to_string(),
            remote: entry.path.clone(),
        });
        ui.close();
    }
    ui.menu_button(format!("📂 {}", state.t("Open with")), |ui| {
        if is_dir {
            if ui.button(state.t("Open")).clicked() {
                state
                    .files_selected
                    .insert(host_id.to_string(), (entry.path.clone(), true));
                goto_dir(state, events, host_id, &entry.path);
                ui.close();
            }
        } else {
            if ui.button(state.t("Built-in editor")).clicked() {
                state
                    .files_selected
                    .insert(host_id.to_string(), (entry.path.clone(), false));
                open_editor(state, events, host_id, &entry.path);
                ui.close();
            }
            if ui.button(state.t("Download")).clicked() {
                events.push(UiEvent::SftpDownloadToDownloads {
                    host_id: host_id.to_string(),
                    remote: entry.path.clone(),
                });
                ui.close();
            }
        }
    });
    if ui.button(format!("✏️ {}", state.t("Rename"))).clicked() {
        state.text_prompt = Some(TextPrompt {
            title: state.t("Rename").to_string(),
            value: entry.name.clone(),
            action: format!("sftp_rename:{host_id}:{}", entry.path),
        });
        ui.close();
    }
    if ui.button(format!("📋 {}", state.t("Copy path"))).clicked() {
        ui.ctx().copy_text(entry.path.clone());
        ui.close();
    }
    ui.menu_button(format!("🖥 {}", state.t("Terminal")), |ui| {
        if ui.button(state.t("Open terminal here")).clicked() {
            events.push(UiEvent::TerminalCdHost {
                host_id: host_id.to_string(),
                path: entry_dir.clone(),
            });
            ui.close();
        }
    });
    ui.menu_button(format!("⋯ {}", state.t("More")), |ui| {
        if ui.button(state.t("Properties")).clicked() {
            let size = super::dashboard::fmt_bytes(entry.size as i64);
            let modified = entry
                .modified_at
                .map(|t| t.to_string())
                .unwrap_or_else(|| "—".to_string());
            let perms = entry.permissions.clone().unwrap_or_else(|| "—".to_string());
            state.confirm = Some(crate::ConfirmDialog {
                title: state.t("Properties").to_string(),
                message: format!(
                    "{}\n{}: {}\n{}: {}\n{}: {}",
                    entry.path,
                    state.t("Size"),
                    size,
                    state.t("Modified"),
                    modified,
                    state.t("Permissions"),
                    perms
                ),
                confirm_label: state.t("OK").to_string(),
                action: "noop".to_string(),
            });
            ui.close();
        }
    });
    ui.separator();
    if ui
        .button(
            egui::RichText::new(format!("🗑 {}", state.t("Delete")))
                .color(Color32::from_rgb(215, 70, 60)),
        )
        .clicked()
    {
        state.confirm = Some(crate::ConfirmDialog {
            title: state.t("Delete").to_string(),
            message: state.tf("Delete {path}?", &[("path", &entry.path)]),
            confirm_label: state.t("Delete").to_string(),
            action: format!("sftp_delete:{host_id}:{}", entry.path),
        });
        ui.close();
    }
}

/// Open a file in the built-in editor (activates an existing tab).
fn open_editor(state: &mut UiState, events: &mut Vec<UiEvent>, host_id: &str, path: &str) {
    if let Some(idx) = state
        .editors
        .iter()
        .position(|e| e.host_id == host_id && e.path == path)
    {
        state.active_editor = Some(idx);
        return;
    }
    state.editors.push(FileEditor::loading(host_id, path));
    state.active_editor = Some(state.editors.len() - 1);
    events.push(UiEvent::SftpEditFile {
        host_id: host_id.to_string(),
        path: path.to_string(),
    });
}

// ---------------------------------------------------------------------------
// Editor (center tabs, above the terminal panes)
// ---------------------------------------------------------------------------

/// Render the editor area into `rect` (called from the terminal view when
/// at least one editor tab is open).
pub fn editor_area(ui: &mut egui::Ui, state: &mut UiState, events: &mut Vec<UiEvent>, rect: Rect) {
    // Editor surface follows the theme (white in light themes).
    ui.painter()
        .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut child;

    // Tab strip.
    let mut activate: Option<usize> = None;
    let mut close: Option<usize> = None;
    ui.horizontal(|ui| {
        for (i, e) in state.editors.iter().enumerate() {
            let active = state.active_editor == Some(i);
            let name = e.path.rsplit('/').next().unwrap_or(&e.path).to_string();
            let label = format!("{}{}", if e.dirty() { "● " } else { "" }, name);
            if ui.selectable_label(active, label).clicked() {
                activate = Some(i);
            }
            if ui
                .add(egui::Button::new("✕").frame(false).small())
                .on_hover_text(state.t("Close"))
                .clicked()
            {
                close = Some(i);
            }
        }
    });
    if let Some(i) = activate {
        state.active_editor = Some(i);
    }
    if let Some(i) = close
        && i < state.editors.len()
    {
        state.editors.remove(i);
        state.active_editor = match state.editors.len() {
            0 => None,
            n => Some(i.min(n - 1)),
        };
    }
    ui.separator();

    let Some(idx) = state.active_editor.filter(|i| *i < state.editors.len()) else {
        if state.editors.is_empty() {
            return;
        }
        state.active_editor = Some(0);
        return;
    };

    editor_toolbar(ui, state, events, idx);
    let Some(editor) = state.editors.get(idx) else {
        return;
    };
    if editor.loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(state.t("Loading…"));
        });
        return;
    }
    if let Some(err) = editor.error.clone() {
        ui.colored_label(Color32::from_rgb(255, 120, 120), err);
    }
    let font_size = state.editor_font_size;
    let body_h = ui.available_height();
    ui.horizontal(|ui| {
        ui.set_height(body_h);
        egui::ScrollArea::both()
            .id_salt(("file_editor_scroll", idx))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let editor = &mut state.editors[idx];
                    let lines = editor.content.matches('\n').count() + 1;
                    let gutter = (1..=lines)
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(gutter)
                                .font(FontId::new(font_size, FontFamily::Monospace))
                                .weak(),
                        )
                        .selectable(false),
                    );
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut editor.content)
                            .font(FontId::new(font_size, FontFamily::Monospace))
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .layouter(&mut highlight_layouter),
                    );
                    state.editor_focused = resp.has_focus();
                });
            });
        minimap(ui, state, idx, body_h);
    });

    // Ctrl+S saves while the editor has focus.
    if state.editor_focused
        && ui
            .ctx()
            .input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S))
    {
        save_editor(state, events, idx);
    }
}

/// Editor toolbar: save / download / font size / line endings / encoding.
fn editor_toolbar(ui: &mut egui::Ui, state: &mut UiState, events: &mut Vec<UiEvent>, idx: usize) {
    let Some(editor) = state.editors.get(idx) else {
        return;
    };
    let (host_id, path, dirty, saving, crlf) = (
        editor.host_id.clone(),
        editor.path.clone(),
        editor.dirty(),
        editor.saving,
        editor.content.contains("\r\n"),
    );
    ui.horizontal(|ui| {
        let save_btn = egui::Button::new(
            egui::RichText::new(if saving {
                state.t("Saving…")
            } else {
                state.t("Save")
            })
            .color(Color32::WHITE),
        )
        .fill(XGREEN);
        if ui.add_enabled(dirty && !saving, save_btn).clicked() {
            save_editor(state, events, idx);
        }
        if icon_btn(ui, "⬇")
            .on_hover_text(state.t("Download"))
            .clicked()
        {
            events.push(UiEvent::SftpDownloadToDownloads {
                host_id: host_id.clone(),
                remote: path.clone(),
            });
        }
        ui.separator();
        if icon_btn(ui, "−")
            .on_hover_text(state.t("Zoom out"))
            .clicked()
        {
            state.editor_font_size = (state.editor_font_size - 1.0).max(9.0);
        }
        ui.label(format!("{:.0}", state.editor_font_size));
        if icon_btn(ui, "＋")
            .on_hover_text(state.t("Zoom in"))
            .clicked()
        {
            state.editor_font_size = (state.editor_font_size + 1.0).min(22.0);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("UTF-8").weak().small());
            let eol = if crlf { "CRLF" } else { "LF" };
            if ui.small_button(eol).clicked()
                && let Some(editor) = state.editors.get_mut(idx)
            {
                if crlf {
                    editor.content = editor.content.replace("\r\n", "\n");
                } else {
                    editor.content = editor.content.replace('\n', "\r\n");
                }
            }
            ui.label(egui::RichText::new(&path).weak().small());
        });
    });
    ui.separator();
}

/// A scaled-down text rail on the right of the editor (Xterminal minimap).
fn minimap(ui: &mut egui::Ui, state: &UiState, idx: usize, height: f32) {
    let Some(editor) = state.editors.get(idx) else {
        return;
    };
    let text: String = editor
        .content
        .lines()
        .take(1500)
        .collect::<Vec<_>>()
        .join("\n");
    ui.allocate_ui_with_layout(
        Vec2::new(54.0, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(text)
                        .font(FontId::new(3.0, FontFamily::Monospace))
                        .weak(),
                )
                .selectable(false),
            );
        },
    );
}

/// Fire the save event for editor `idx` (marks it saving).
fn save_editor(state: &mut UiState, events: &mut Vec<UiEvent>, idx: usize) {
    let Some(editor) = state.editors.get_mut(idx) else {
        return;
    };
    if editor.saving || !editor.dirty() {
        return;
    }
    editor.saving = true;
    events.push(UiEvent::SftpSaveFile {
        host_id: editor.host_id.clone(),
        path: editor.path.clone(),
        content: editor.content.clone(),
    });
}

/// TextEdit layouter with lightweight syntax highlighting: full-line and
/// trailing comments, quoted strings and numbers get distinct colors.
fn highlight_layouter(
    ui: &egui::Ui,
    text: &dyn egui::TextBuffer,
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let text = text.as_str();
    let base = ui.visuals().text_color();
    let font_size = 13.0;
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let comment_c = Color32::from_rgb(106, 153, 85);
    let string_c = Color32::from_rgb(152, 195, 121);
    let number_c = Color32::from_rgb(209, 154, 102);
    let fmt = |color: Color32| egui::text::TextFormat {
        font_id: FontId::new(font_size, FontFamily::Monospace),
        color,
        ..Default::default()
    };
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            job.append(line, 0.0, fmt(comment_c));
            continue;
        }
        let cut = find_comment_cut(line);
        let (code, comment) = match cut {
            Some(i) => (&line[..i], &line[i..]),
            None => (line, ""),
        };
        let bytes = code.as_bytes();
        let mut i = 0;
        let mut run_start = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'"' || b == b'\'' {
                if run_start < i {
                    job.append(&code[run_start..i], 0.0, fmt(base));
                }
                let quote = b;
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != quote && bytes[j] != b'\n' {
                    j += 1;
                }
                let end = if j < bytes.len() { j + 1 } else { j };
                job.append(&code[i..end], 0.0, fmt(string_c));
                i = end;
                run_start = end;
            } else if b.is_ascii_digit() && (i == 0 || !is_word_byte(bytes[i - 1])) {
                if run_start < i {
                    job.append(&code[run_start..i], 0.0, fmt(base));
                }
                let mut j = i;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || bytes[j] == b'x')
                {
                    j += 1;
                }
                job.append(&code[i..j], 0.0, fmt(number_c));
                i = j;
                run_start = j;
            } else {
                i += 1;
            }
        }
        if run_start < code.len() {
            job.append(&code[run_start..], 0.0, fmt(base));
        }
        if !comment.is_empty() {
            job.append(comment, 0.0, fmt(comment_c));
        }
    }
    ui.fonts_mut(|f| f.layout_job(job))
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'
}

/// Byte index where a trailing comment starts in `line`, if any.
fn find_comment_cut(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b' ' || bytes[i] == b'\t' {
            if bytes[i + 1] == b'#' {
                return Some(i);
            }
            if bytes[i + 1] == b'/' && i + 2 < bytes.len() && bytes[i + 2] == b'/' {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Overlays: the text prompt window (editor lives in the center now)
// ---------------------------------------------------------------------------

/// Draw the file-workspace overlay: the rename/new text prompt.
pub fn overlays(ctx: &egui::Context, state: &mut UiState, events: &mut Vec<UiEvent>) {
    prompt_window(ctx, state, events);
}

fn prompt_window(ctx: &egui::Context, state: &mut UiState, events: &mut Vec<UiEvent>) {
    let Some(mut prompt) = state.text_prompt.clone() else {
        return;
    };
    let mut done: Option<bool> = None;
    let mut open = true;
    egui::Window::new(prompt.title.clone())
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut prompt.value)
                    .desired_width(320.0)
                    .hint_text(state.t("Name")),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                done = Some(true);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(state.t("Cancel")).clicked() {
                    done = Some(false);
                }
                if ui.button(state.t("OK")).clicked() {
                    done = Some(true);
                }
            });
        });
    if !open {
        done = Some(false);
    }
    match done {
        Some(true) => {
            state.text_prompt = None;
            dispatch_prompt(state, events, &prompt);
        }
        Some(false) => state.text_prompt = None,
        None => state.text_prompt = Some(prompt),
    }
}

/// Execute a confirmed text prompt.
fn dispatch_prompt(state: &mut UiState, events: &mut Vec<UiEvent>, prompt: &TextPrompt) {
    let name = prompt.value.trim().to_string();
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        state.toast(state.t("Invalid name"), true);
        return;
    }
    let mut parts = prompt.action.splitn(3, ':');
    let op = parts.next().unwrap_or("");
    let host_id = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    match op {
        "sftp_mkdir" => events.push(UiEvent::SftpMkdir {
            host_id,
            path: join_path(&target, &name),
        }),
        "sftp_newfile" => events.push(UiEvent::SftpSaveFile {
            host_id,
            path: join_path(&target, &name),
            content: String::new(),
        }),
        "sftp_rename" => events.push(UiEvent::SftpRename {
            host_id,
            from: target.clone(),
            to: join_path(&parent_path(&target), &name),
        }),
        _ => {}
    }
}

/// Join a remote directory and a file name.
fn join_path(dir: &str, name: &str) -> String {
    if dir == "." {
        name.to_string()
    } else if let Some(stripped) = dir.strip_suffix('/') {
        format!("{stripped}/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Parent of an absolute/relative SFTP path.
fn parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        Some(0) => "/".to_string(),
        Some(idx) => trimmed[..idx].to_string(),
        None => ".".to_string(),
    }
}
