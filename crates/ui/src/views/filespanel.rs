//! Session file workspace (Xterminal-style left panel inside the terminal
//! view): path bar, refresh, directory entries for the active host.
//! Author: Passwd Word

use std::time::Instant;

use crate::{MainTab, UiData, UiEvent, UiState};

/// Draw the file workspace panel for the active pane's host.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host_id: &str,
) {
    egui::Panel::left("term_files")
        .default_size(248.0)
        .size_range(180.0..=420.0)
        .show(ui, |ui| {
            let path = state.sftp_path(host_id);
            // Header
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(state.t("Files")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("🔄")
                        .on_hover_text(state.t("Refresh"))
                        .clicked()
                    {
                        events.push(UiEvent::SftpNavigate {
                            host_id: host_id.to_string(),
                            path: path.clone(),
                        });
                    }
                    if ui
                        .small_button("↗")
                        .on_hover_text(state.t("Open in Files"))
                        .clicked()
                    {
                        state.main_tab = MainTab::Files;
                        state.sftp_host = Some(host_id.to_string());
                    }
                });
            });
            // Path bar
            ui.horizontal(|ui| {
                if ui
                    .small_button("⬆")
                    .on_hover_text(state.t("Parent directory"))
                    .clicked()
                {
                    let parent = parent_path(&path);
                    events.push(UiEvent::SftpNavigate {
                        host_id: host_id.to_string(),
                        path: parent,
                    });
                }
                ui.label(egui::RichText::new(&path).small().weak().monospace());
            });
            ui.separator();

            // Auto-load the listing when missing/stale (5s retry cooldown).
            let listing = data.sftp_listings.get(host_id);
            let needs_load = match listing {
                None => true,
                Some(l) => l.path != path && !l.loading,
            };
            if needs_load {
                let due = state
                    .files_panel_requested
                    .get(host_id)
                    .is_none_or(|t| t.elapsed().as_secs() >= 5);
                if due {
                    state
                        .files_panel_requested
                        .insert(host_id.to_string(), Instant::now());
                    events.push(UiEvent::SftpNavigate {
                        host_id: host_id.to_string(),
                        path,
                    });
                }
            }

            match data.sftp_listings.get(host_id) {
                None => {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(state.t("Loading…"));
                    });
                }
                Some(l) => {
                    if l.loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(state.t("Loading…"));
                        });
                    }
                    if let Some(err) = &l.error {
                        ui.colored_label(egui::Color32::from_rgb(255, 120, 120), err);
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Directories first, then files; alphabetical within.
                        let mut entries: Vec<_> = l.entries.iter().collect();
                        entries.sort_by(|a, b| {
                            b.is_dir()
                                .cmp(&a.is_dir())
                                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                        });
                        for entry in entries {
                            let icon = if entry.is_dir() {
                                "📁"
                            } else if entry.kind == opsdeck_sftp::FileKind::Symlink {
                                "🔗"
                            } else {
                                "📄"
                            };
                            let resp = ui.selectable_label(
                                false,
                                egui::RichText::new(format!("{icon} {}", entry.name)).small(),
                            );
                            if entry.is_dir() && resp.clicked() {
                                events.push(UiEvent::SftpNavigate {
                                    host_id: host_id.to_string(),
                                    path: entry.path.clone(),
                                });
                            }
                            resp.on_hover_text(format!(
                                "{}\n{}",
                                entry.path,
                                super::dashboard::fmt_bytes(entry.size as i64)
                            ));
                        }
                        if l.entries.is_empty() && !l.loading && l.error.is_none() {
                            ui.label(egui::RichText::new(state.t("(empty)")).small().weak());
                        }
                    });
                }
            }
        });
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
