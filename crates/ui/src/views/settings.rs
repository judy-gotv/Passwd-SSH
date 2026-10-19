//! Settings view: theme, fonts, vault, sync, update, shortcuts, about.
//! Author: Passwd Word

use crate::theme::Theme;
use crate::{UiData, UiEvent, UiState};

/// Draw the Settings tab.
pub fn show(ctx: &egui::Context, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("Appearance").strong());
            ui.horizontal(|ui| {
                ui.label("Theme:");
                egui::ComboBox::from_id_salt("theme_picker")
                    .selected_text(&state.theme_name)
                    .show_ui(ui, |ui| {
                        for t in Theme::builtins() {
                            if ui
                                .selectable_label(state.theme_name == t.name, &t.name)
                                .clicked()
                            {
                                state.theme_name = t.name.clone();
                                events.push(UiEvent::SetTheme(t.name));
                            }
                        }
                    });
            });
            let mut fs = data.theme.font_size;
            if ui
                .add(egui::Slider::new(&mut fs, 10.0..=22.0).text("UI font size"))
                .changed()
            {
                events.push(UiEvent::SetFontSize(fs));
            }
            let mut tfs = data.theme.terminal_font_size;
            if ui
                .add(egui::Slider::new(&mut tfs, 10.0..=24.0).text("Terminal font size"))
                .changed()
            {
                events.push(UiEvent::SetTerminalFontSize(tfs));
            }

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new("Vault").strong());
            ui.horizontal(|ui| {
                ui.label("Current password:");
                ui.add(egui::TextEdit::singleline(&mut state.settings_pw_old).password(true));
            });
            ui.horizontal(|ui| {
                ui.label("New password:");
                ui.add(egui::TextEdit::singleline(&mut state.settings_pw_new).password(true));
            });
            ui.horizontal(|ui| {
                ui.label("Confirm new:");
                ui.add(egui::TextEdit::singleline(&mut state.settings_pw_confirm).password(true));
            });
            ui.horizontal(|ui| {
                if ui.button("Change master password").clicked() {
                    if state.settings_pw_new.len() < 8 {
                        state.toast("New password must be at least 8 characters", true);
                    } else if state.settings_pw_new != state.settings_pw_confirm {
                        state.toast("New passwords do not match", true);
                    } else {
                        events.push(UiEvent::ChangeMasterPassword {
                            old: std::mem::take(&mut state.settings_pw_old),
                            new: std::mem::take(&mut state.settings_pw_new),
                        });
                        state.settings_pw_confirm.clear();
                    }
                }
                if ui.button("🔒 Lock vault").clicked() {
                    events.push(UiEvent::LockVault);
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new("Sync").strong());
            ui.horizontal(|ui| {
                ui.label("Mode:");
                egui::ComboBox::from_id_salt("sync_kind")
                    .selected_text(match state.sync_kind.as_str() {
                        "file" => "Shared folder",
                        "http" => "Sync server",
                        _ => "Off",
                    })
                    .show_ui(ui, |ui| {
                        for (id, label) in [("off", "Off"), ("file", "Shared folder"), ("http", "Sync server")] {
                            if ui.selectable_label(state.sync_kind == id, label).clicked() {
                                state.sync_kind = id.to_string();
                            }
                        }
                    });
                if ui.button("Save").clicked() {
                    events.push(UiEvent::SetSyncKind(state.sync_kind.clone()));
                }
                if ui.button("Sync now").clicked() {
                    events.push(UiEvent::SyncNow);
                }
            });
            if state.sync_kind == "file" {
                ui.horizontal(|ui| {
                    ui.label("Folder:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.sync_file_dir)
                            .hint_text("/path/to/synced/folder")
                            .desired_width(320.0),
                    );
                });
                ui.label(egui::RichText::new("Any folder replicated by Syncthing / Nextcloud / rsync works.").small().weak());
            } else if state.sync_kind == "http" {
                ui.horizontal(|ui| {
                    ui.label("Server:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.sync_http_endpoint)
                            .hint_text("https://sync.example.com")
                            .desired_width(320.0),
                    );
                });
            }
            if data.sync.running {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Syncing…");
                });
            }
            if let Some(summary) = &data.sync.last_summary {
                ui.label(format!("Last sync: {summary}"));
            } else if let Some(ts) = data.sync.last_sync {
                ui.label(format!("Last sync at {ts}"));
            }
            if let Some(err) = &data.sync.error {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), format!("Sync error: {err}"));
            }
            ui.label(
                egui::RichText::new("Only hosts, groups and snippets sync — and always end-to-end encrypted. Credentials never leave this device.")
                    .small()
                    .weak(),
            );

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new("Updates").strong());
            ui.horizontal(|ui| {
                ui.label(format!("Current version: {}", data.update.current));
                if ui.button("Check for updates").clicked() {
                    events.push(UiEvent::CheckUpdate);
                }
                if data.update.checking {
                    ui.spinner();
                }
            });
            if data.update.available {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 220, 120),
                        format!("New version available: {}", data.update.latest.clone().unwrap_or_default()),
                    );
                    if let Some(url) = data.update.url.clone() {
                        if ui.button("Download").clicked() {
                            events.push(UiEvent::OpenUrl(url));
                        }
                    }
                });
            } else if let Some(err) = &data.update.error {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), format!("Update check failed: {err}"));
            }

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new("Keyboard shortcuts").strong());
            egui::Grid::new("shortcut_grid").num_columns(3).striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Action").strong());
                ui.label(egui::RichText::new("Windows / Linux").strong());
                ui.label(egui::RichText::new("macOS").strong());
                ui.end_row();
                for s in crate::shortcuts::default_shortcuts() {
                    ui.label(s.description);
                    ui.label(egui::RichText::new(s.windows_linux).monospace());
                    ui.label(egui::RichText::new(s.macos).monospace());
                    ui.end_row();
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new("About").strong());
            ui.label(format!("OpsDeck {}", data.update.current));
            ui.label(egui::RichText::new("A local-first SSH workstation. Your data stays on your machine.").small().weak());
        });
    });
}
