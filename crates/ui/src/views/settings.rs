//! Settings view: language, theme, fonts, vault, sync, update, shortcuts, about.
//! Author: Passwd Word

use crate::i18n::Language;
use crate::theme::Theme;
use crate::{UiData, UiEvent, UiState};

/// Draw the Settings tab.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new(state.t("Appearance")).strong());
            ui.horizontal(|ui| {
                ui.label(state.t("Language:"));
                egui::ComboBox::from_id_salt("language_picker")
                    .selected_text(state.language.name())
                    .show_ui(ui, |ui| {
                        for lang in Language::all() {
                            if ui
                                .selectable_label(state.language == lang, lang.name())
                                .clicked()
                            {
                                state.language = lang;
                                events.push(UiEvent::SetLanguage(lang.code().to_string()));
                                ui.close();
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(state.t("Theme:"));
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
                .add(egui::Slider::new(&mut fs, 10.0..=22.0).text(state.t("UI font size")))
                .changed()
            {
                events.push(UiEvent::SetFontSize(fs));
            }
            let mut tfs = data.theme.terminal_font_size;
            if ui
                .add(
                    egui::Slider::new(&mut tfs, 10.0..=24.0)
                        .text(state.t("Terminal font size")),
                )
                .changed()
            {
                events.push(UiEvent::SetTerminalFontSize(tfs));
            }

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new(state.t("Vault")).strong());
            if !state.vault_exists {
                ui.label(state.t(
                    "No vault yet — set a master password so credentials can be stored encrypted.",
                ));
                if ui.button(state.t("Set master password")).clicked() {
                    state.show_vault_dialog = true;
                }
            } else if !state.vault_unlocked {
                ui.label(state.t("Vault is locked."));
                if ui.button(state.t("Unlock vault")).clicked() {
                    state.show_vault_dialog = true;
                }
            } else {
                ui.label(state.t("Vault is unlocked."));
                ui.horizontal(|ui| {
                    ui.label(state.t("Current password:"));
                    ui.add(egui::TextEdit::singleline(&mut state.settings_pw_old).password(true));
                });
                ui.horizontal(|ui| {
                    ui.label(state.t("New password:"));
                    ui.add(egui::TextEdit::singleline(&mut state.settings_pw_new).password(true));
                });
                ui.horizontal(|ui| {
                    ui.label(state.t("Confirm new:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.settings_pw_confirm).password(true),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button(state.t("Change master password")).clicked() {
                        if state.settings_pw_new.len() < 8 {
                            state.toast(
                                state.t("New password must be at least 8 characters"),
                                true,
                            );
                        } else if state.settings_pw_new != state.settings_pw_confirm {
                            state.toast(state.t("New passwords do not match"), true);
                        } else {
                            events.push(UiEvent::ChangeMasterPassword {
                                old: std::mem::take(&mut state.settings_pw_old),
                                new: std::mem::take(&mut state.settings_pw_new),
                            });
                            state.settings_pw_confirm.clear();
                        }
                    }
                    if ui.button(state.t("🔒 Lock vault")).clicked() {
                        events.push(UiEvent::LockVault);
                    }
                });
            }

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new(state.t("Sync")).strong());
            ui.horizontal(|ui| {
                ui.label(state.t("Mode:"));
                egui::ComboBox::from_id_salt("sync_kind")
                    .selected_text(match state.sync_kind.as_str() {
                        "file" => state.t("Shared folder"),
                        "http" => state.t("Sync server"),
                        _ => state.t("Off"),
                    })
                    .show_ui(ui, |ui| {
                        for (id, key) in [
                            ("off", "Off"),
                            ("file", "Shared folder"),
                            ("http", "Sync server"),
                        ] {
                            let label = state.t(key);
                            if ui.selectable_label(state.sync_kind == id, label).clicked() {
                                state.sync_kind = id.to_string();
                            }
                        }
                    });
                if ui.button(state.t("Save")).clicked() {
                    events.push(UiEvent::SetSyncKind(state.sync_kind.clone()));
                }
                if ui.button(state.t("Sync now")).clicked() {
                    events.push(UiEvent::SyncNow);
                }
            });
            if state.sync_kind == "file" {
                ui.horizontal(|ui| {
                    ui.label(state.t("Folder:"));
                    let folder_hint = state.t("/path/to/synced/folder");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.sync_file_dir)
                            .hint_text(folder_hint)
                            .desired_width(320.0),
                    );
                });
                ui.label(
                    egui::RichText::new(
                        state.t("Any folder replicated by Syncthing / Nextcloud / rsync works."),
                    )
                    .small()
                    .weak(),
                );
            } else if state.sync_kind == "http" {
                ui.horizontal(|ui| {
                    ui.label(state.t("Server:"));
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
                    ui.label(state.t("Syncing…"));
                });
            }
            if let Some(summary) = &data.sync.last_summary {
                ui.label(state.tf("Last sync: {summary}", &[("summary", summary)]));
            } else if let Some(ts) = data.sync.last_sync {
                ui.label(state.tf("Last sync at {ts}", &[("ts", &ts.to_string())]));
            }
            if let Some(err) = &data.sync.error {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 120, 120),
                    state.tf("Sync error: {err}", &[("err", err)]),
                );
            }
            ui.label(
                egui::RichText::new(state.t(
                    "Only hosts, groups and snippets sync — and always end-to-end encrypted. Credentials never leave this device.",
                ))
                .small()
                .weak(),
            );

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new(state.t("Updates")).strong());
            ui.horizontal(|ui| {
                ui.label(state.tf(
                    "Current version: {current}",
                    &[("current", &data.update.current)],
                ));
                if ui.button(state.t("Check for updates")).clicked() {
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
                        state.tf(
                            "New version available: {latest}",
                            &[(
                                "latest",
                                &data.update.latest.clone().unwrap_or_default(),
                            )],
                        ),
                    );
                    if let Some(url) = data.update.url.clone()
                        && ui.button(state.t("Download")).clicked()
                    {
                        events.push(UiEvent::OpenUrl(url));
                    }
                });
            } else if let Some(err) = &data.update.error {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 120, 120),
                    state.tf("Update check failed: {err}", &[("err", err)]),
                );
            }

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new(state.t("Keyboard shortcuts")).strong());
            egui::Grid::new("shortcut_grid")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(state.t("Action")).strong());
                    ui.label(egui::RichText::new("Windows / Linux").strong());
                    ui.label(egui::RichText::new("macOS").strong());
                    ui.end_row();
                    for s in crate::shortcuts::default_shortcuts() {
                        ui.label(state.t(s.description));
                        ui.label(egui::RichText::new(s.windows_linux).monospace());
                        ui.label(egui::RichText::new(s.macos).monospace());
                        ui.end_row();
                    }
                });

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new(state.t("About")).strong());
            ui.label(state.tf("OpsDeck {current}", &[("current", &data.update.current)]));
            ui.label(
                egui::RichText::new(state.t(
                    "A local-first SSH workstation. Your data stays on your machine.",
                ))
                .small()
                .weak(),
            );
        });
    });
}
