//! Session file workspace (Xterminal-style left panel inside the terminal
//! view): breadcrumb, expandable directory tree, and full file operations
//! (upload / download / edit & save / rename / new folder & file / delete).
//! Author: Passwd Word

use std::time::Instant;

use opsdeck_sftp::{FileEntry, FileKind};

use crate::{FileEditor, MainTab, TextPrompt, UiData, UiEvent, UiState};

const MAX_TREE_DEPTH: usize = 12;

/// Draw the file workspace panel for the active pane's host.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host_id: &str,
) {
    egui::Panel::left("term_files")
        .default_size(264.0)
        .size_range(200.0..=460.0)
        .show(ui, |ui| {
            let current = state.sftp_path(host_id);
            // Header
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(state.t("Files")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("↗")
                        .on_hover_text(state.t("Open in Files"))
                        .clicked()
                    {
                        state.main_tab = MainTab::Files;
                        state.sftp_host = Some(host_id.to_string());
                    }
                    if ui
                        .small_button("🔄")
                        .on_hover_text(state.t("Refresh"))
                        .clicked()
                    {
                        events.push(UiEvent::SftpNavigate {
                            host_id: host_id.to_string(),
                            path: current.clone(),
                        });
                    }
                });
            });
            toolbar(ui, state, events, host_id, &current);
            if state.files_upload_open {
                upload_row(ui, state, events, host_id, &current);
            }
            breadcrumb(ui, state, events, host_id, &current);
            ui.separator();

            // The tree is rooted at "/" like Xterminal; make sure the root
            // and the current directory are loaded.
            ensure_listing(state, data, events, host_id, "/");
            if current != "/" {
                ensure_listing(state, data, events, host_id, &current);
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                tree_level(ui, state, data, events, host_id, "/", 0);
            });
        });
}

/// Action toolbar under the header: upload / download / edit / new / delete,
/// acting on the selected entry (or the current directory for "new").
fn toolbar(
    ui: &mut egui::Ui,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    current: &str,
) {
    let selected = state.files_selected.get(host_id).cloned();
    ui.horizontal(|ui| {
        if ui
            .small_button("⬆")
            .on_hover_text(state.t("Upload"))
            .clicked()
        {
            state.files_upload_open = !state.files_upload_open;
        }
        if ui
            .small_button("⬇")
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
        if ui
            .small_button("✏️")
            .on_hover_text(state.t("Edit"))
            .clicked()
        {
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
        if ui
            .small_button("🗑")
            .on_hover_text(state.t("Delete"))
            .clicked()
        {
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

/// Inline upload row: type a local path, upload it into the current dir.
fn upload_row(
    ui: &mut egui::Ui,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    current: &str,
) {
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
                remote: join_path(current, &base),
            });
            state.sftp_upload_local.clear();
            state.files_upload_open = false;
        }
    });
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

/// Set the current directory (breadcrumb follows) and load it.
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

/// One tree level: entries of `dir`, recursing into expanded directories.
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
        ui.colored_label(egui::Color32::from_rgb(255, 120, 120), err);
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
        let is_dir = entry.is_dir();
        let expanded = state
            .files_expanded
            .get(host_id)
            .is_some_and(|set| set.contains(&entry.path));
        let selected = state
            .files_selected
            .get(host_id)
            .is_some_and(|(p, _)| p == &entry.path);
        let mut toggle: Option<bool> = None;
        let mut activate = false;
        let mut edit = false;
        ui.horizontal(|ui| {
            if depth > 0 {
                ui.add_space(depth as f32 * 14.0);
            }
            if is_dir {
                let arrow = if expanded { "▼" } else { "▶" };
                if ui.small_button(arrow).clicked() {
                    toggle = Some(!expanded);
                }
            } else {
                ui.add_space(20.0);
            }
            let icon = if is_dir {
                "📁"
            } else if entry.kind == FileKind::Symlink {
                "🔗"
            } else {
                "📄"
            };
            let resp = ui.selectable_label(
                selected,
                egui::RichText::new(format!("{icon} {}", entry.name)).small(),
            );
            if resp.clicked() {
                activate = true;
            }
            if resp.double_clicked() {
                if is_dir {
                    activate = true;
                } else {
                    edit = true;
                }
            }
            if !is_dir {
                ui.label(
                    egui::RichText::new(super::dashboard::fmt_bytes(entry.size as i64))
                        .small()
                        .weak(),
                );
            }
            resp.context_menu(|ui| {
                entry_menu(ui, state, events, host_id, &entry);
            });
        });
        if let Some(open) = toggle {
            let set = state.files_expanded.entry(host_id.to_string()).or_default();
            if open {
                set.insert(entry.path.clone());
                ensure_listing(state, data, events, host_id, &entry.path);
            } else {
                set.remove(&entry.path);
            }
        }
        if activate {
            state
                .files_selected
                .insert(host_id.to_string(), (entry.path.clone(), is_dir));
            if is_dir {
                goto_dir(state, events, host_id, &entry.path);
            }
        }
        if edit {
            state
                .files_selected
                .insert(host_id.to_string(), (entry.path.clone(), false));
            open_editor(state, events, host_id, &entry.path);
        }
        if is_dir && expanded && depth < MAX_TREE_DEPTH {
            tree_level(ui, state, data, events, host_id, &entry.path, depth + 1);
        }
    }
}

/// Right-click menu for one entry.
fn entry_menu(
    ui: &mut egui::Ui,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    host_id: &str,
    entry: &FileEntry,
) {
    let is_dir = entry.is_dir();
    if is_dir {
        if ui.button(state.t("Open")).clicked() {
            state
                .files_selected
                .insert(host_id.to_string(), (entry.path.clone(), true));
            goto_dir(state, events, host_id, &entry.path);
            ui.close();
        }
        if ui.button(state.t("New folder…")).clicked() {
            state.text_prompt = Some(TextPrompt {
                title: state.t("New folder").to_string(),
                value: String::new(),
                action: format!("sftp_mkdir:{host_id}:{}", entry.path),
            });
            ui.close();
        }
        if ui.button(state.t("New file…")).clicked() {
            state.text_prompt = Some(TextPrompt {
                title: state.t("New file").to_string(),
                value: String::new(),
                action: format!("sftp_newfile:{host_id}:{}", entry.path),
            });
            ui.close();
        }
    } else {
        if ui.button(state.t("⬇ Download")).clicked() {
            state
                .files_selected
                .insert(host_id.to_string(), (entry.path.clone(), false));
            events.push(UiEvent::SftpDownloadToDownloads {
                host_id: host_id.to_string(),
                remote: entry.path.clone(),
            });
            ui.close();
        }
        if ui.button(state.t("✏️ Edit")).clicked() {
            state
                .files_selected
                .insert(host_id.to_string(), (entry.path.clone(), false));
            open_editor(state, events, host_id, &entry.path);
            ui.close();
        }
    }
    if ui.button(state.t("Rename…")).clicked() {
        state.text_prompt = Some(TextPrompt {
            title: state.t("Rename").to_string(),
            value: entry.name.clone(),
            action: format!("sftp_rename:{host_id}:{}", entry.path),
        });
        ui.close();
    }
    if ui.button(state.t("🗑 Delete")).clicked() {
        state.confirm = Some(crate::ConfirmDialog {
            title: state.t("Delete").to_string(),
            message: state.tf("Delete {path}?", &[("path", &entry.path)]),
            confirm_label: state.t("Delete").to_string(),
            action: format!("sftp_delete:{host_id}:{}", entry.path),
        });
        ui.close();
    }
}

/// Open the built-in editor for a remote file (loads asynchronously).
fn open_editor(state: &mut UiState, events: &mut Vec<UiEvent>, host_id: &str, path: &str) {
    state.file_editor = Some(FileEditor::loading(host_id, path));
    events.push(UiEvent::SftpEditFile {
        host_id: host_id.to_string(),
        path: path.to_string(),
    });
}

// ---------------------------------------------------------------------------
// Overlays: text prompt + file editor windows (called from dialogs::show_all)
// ---------------------------------------------------------------------------

/// Draw the file-workspace overlays: the rename/new prompt and the editor.
pub fn overlays(ctx: &egui::Context, state: &mut UiState, events: &mut Vec<UiEvent>) {
    prompt_window(ctx, state, events);
    editor_window(ctx, state, events);
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

fn editor_window(ctx: &egui::Context, state: &mut UiState, events: &mut Vec<UiEvent>) {
    if state.file_editor.is_none() {
        return;
    }
    let (host_id, path, saving, dirty) = {
        let e = state.file_editor.as_ref().unwrap();
        (e.host_id.clone(), e.path.clone(), e.saving, e.dirty())
    };
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    let title = format!("✏️ {name}{}", if dirty { " ●" } else { "" });
    let mut open = true;
    let mut do_save = false;
    let mut do_close = false;
    egui::Window::new(title)
        .default_size([720.0, 480.0])
        .resizable(true)
        .open(&mut open)
        .show(ctx, |ui| {
            let editor = state.file_editor.as_mut().unwrap();
            if editor.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(state.t("Loading…"));
                });
                return;
            }
            if let Some(err) = &editor.error {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), err);
            }
            egui::ScrollArea::both().show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut editor.content)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(22),
                );
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&path).small().weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(state.t("Close")).clicked() {
                        do_close = true;
                    }
                    let save_label = if saving {
                        state.t("Saving…")
                    } else {
                        state.t("Save")
                    };
                    if ui
                        .add_enabled(dirty && !saving, egui::Button::new(save_label))
                        .clicked()
                    {
                        do_save = true;
                    }
                });
            });
        });
    if do_save {
        let content = state
            .file_editor
            .as_ref()
            .map(|e| e.content.clone())
            .unwrap_or_default();
        if let Some(e) = state.file_editor.as_mut() {
            e.saving = true;
        }
        events.push(UiEvent::SftpSaveFile {
            host_id,
            path,
            content,
        });
    }
    if do_close || !open {
        state.file_editor = None;
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
