//! Main layout: menu bar, central content, status bar, global shortcuts.
//! Author: Passwd Word

use crate::{MainTab, UiData, UiEvent, UiState};

/// Top menu bar.
pub fn top_bar(ui: &mut egui::Ui, state: &mut UiState, events: &mut Vec<UiEvent>) {
    egui::Panel::top("menu_bar").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(state.t("File"), |ui| {
                if ui.button(state.t("New terminal tab    Ctrl+T")).clicked() {
                    new_tab(state, events);
                    ui.close();
                }
                if ui.button(state.t("Import hosts…")).clicked() {
                    state.confirm = Some(crate::ConfirmDialog {
                        title: state.t("Import hosts").to_string(),
                        message: state
                            .t("Paste the exported JSON below, then confirm.")
                            .to_string(),
                        confirm_label: state.t("Import").to_string(),
                        action: "import_hosts".to_string(),
                    });
                    state.import_buffer.clear();
                    ui.close();
                }
                if ui.button(state.t("Export hosts…")).clicked() {
                    events.push(UiEvent::ExportHosts);
                    ui.close();
                }
                ui.separator();
                if ui.button(state.t("🔒 Lock vault")).clicked() {
                    events.push(UiEvent::LockVault);
                    ui.close();
                }
                if ui.button(state.t("Quit")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.close();
                }
            });

            ui.menu_button(state.t("View"), |ui| {
                for (tab, label) in [
                    (MainTab::Manager, state.t("Hosts")),
                    (MainTab::Terminal, state.t("Terminal")),
                    (MainTab::Files, state.t("Files (SFTP)")),
                    (MainTab::Monitor, state.t("Monitor")),
                    (MainTab::Snippets, state.t("Snippets")),
                    (MainTab::Settings, state.t("Settings")),
                ] {
                    if ui.selectable_label(state.main_tab == tab, label).clicked() {
                        state.main_tab = tab;
                        ui.close();
                    }
                }
                ui.separator();
                ui.menu_button(state.t("Theme"), |ui| {
                    for theme in crate::theme::Theme::builtins() {
                        if ui
                            .selectable_label(state.theme_name == theme.name, &theme.name)
                            .clicked()
                        {
                            state.theme_name = theme.name.clone();
                            events.push(UiEvent::SetTheme(theme.name.clone()));
                            ui.close();
                        }
                    }
                });
            });

            ui.menu_button(state.t("Session"), |ui| {
                if ui
                    .button(state.t("Split left/right    Ctrl+Shift+E"))
                    .clicked()
                {
                    events.push(UiEvent::SplitPane { vertical: false });
                    ui.close();
                }
                if ui
                    .button(state.t("Split up/down    Ctrl+Shift+O"))
                    .clicked()
                {
                    events.push(UiEvent::SplitPane { vertical: true });
                    ui.close();
                }
                ui.separator();
                if ui.button(state.t("Port forwarding…")).clicked() {
                    state.show_forward_dialog = true;
                    ui.close();
                }
                if ui.button(state.t("Reconnect session")).clicked() {
                    if let Some(pane) = state.active_pane().cloned() {
                        events.push(UiEvent::ReconnectSession(pane.session));
                    }
                    ui.close();
                }
            });

            ui.menu_button(state.t("Help"), |ui| {
                if ui.button(state.t("Keyboard shortcuts")).clicked() {
                    state.main_tab = MainTab::Settings;
                    ui.close();
                }
                if ui.button(state.t("Check for updates")).clicked() {
                    events.push(UiEvent::CheckUpdate);
                    ui.close();
                }
            });

            // Right side: tab switcher
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (tab, label) in [
                    (MainTab::Manager, state.t("Hosts")),
                    (MainTab::Terminal, state.t("Terminal")),
                    (MainTab::Files, state.t("Files")),
                    (MainTab::Monitor, state.t("Monitor")),
                    (MainTab::Snippets, state.t("Snippets")),
                    (MainTab::Settings, "⚙"),
                ] {
                    if ui.selectable_label(state.main_tab == tab, label).clicked() {
                        state.main_tab = tab;
                    }
                }
            });
        });
    });
}

/// Central content dispatch.
pub fn central(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    match state.main_tab {
        MainTab::Manager => super::manager::show(ui, state, data, events),
        MainTab::Terminal => super::terminal::show(ui, state, data, events),
        MainTab::Files => super::sftp::show(ui, state, data, events),
        MainTab::Monitor => super::monitor::show(ui, state, data, events),
        MainTab::Snippets => super::snippets::show(ui, state, data, events),
        MainTab::Settings => super::settings::show(ui, state, data, events),
    }
}

/// Bottom status bar.
pub fn status_bar(ui: &mut egui::Ui, state: &mut UiState, data: &UiData) {
    egui::Panel::bottom("status_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(state.tf("{n} session(s)", &[("n", &data.sessions.len().to_string())]));
            if data.sessions.is_empty() {
                ui.label(egui::RichText::new(state.t("not connected")).weak());
            }
            if !data.key_prompts.is_empty() {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 200, 90),
                    state.tf(
                        "⚠ {n} host key approval(s) pending",
                        &[("n", &data.key_prompts.len().to_string())],
                    ),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if data.update.available {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 220, 120),
                        state.tf(
                            "⬆ Update available: {latest}",
                            &[("latest", &data.update.latest.clone().unwrap_or_default())],
                        ),
                    );
                }
                if data.sync.configured {
                    let txt = data
                        .sync
                        .last_summary
                        .clone()
                        .unwrap_or_else(|| state.t("sync on").to_string());
                    ui.label(egui::RichText::new(format!("🔄 {txt}")).small().weak());
                }
                if !data.forwards.is_empty() {
                    ui.label(
                        egui::RichText::new(state.tf(
                            "⇄ {n} forward(s)",
                            &[("n", &data.forwards.len().to_string())],
                        ))
                        .small()
                        .weak(),
                    );
                }
                let _ = state;
            });
        });
    });
}

/// Global keyboard shortcuts (checked every frame).
pub fn global_shortcuts(ctx: &egui::Context, state: &mut UiState, events: &mut Vec<UiEvent>) {
    ctx.input(|i| {
        for ev in &i.events {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = ev
            else {
                continue;
            };
            use egui::Key;
            if modifiers.command && !modifiers.shift {
                match key {
                    Key::T => new_tab(state, events),
                    Key::W => close_active_tab(state, events),
                    Key::P => {
                        state.focus_host_search = true;
                    }
                    Key::F => {
                        // Terminal search (only meaningful on the terminal tab).
                        if state.main_tab == MainTab::Terminal
                            && let Some(pane) = state.active_pane().cloned()
                        {
                            let open = !state
                                .search_open
                                .get(&pane.session)
                                .copied()
                                .unwrap_or(false);
                            state.search_open.insert(pane.session, open);
                        }
                    }
                    Key::Num1 => switch_tab(state, 0),
                    Key::Num2 => switch_tab(state, 1),
                    Key::Num3 => switch_tab(state, 2),
                    Key::Num4 => switch_tab(state, 3),
                    Key::Num5 => switch_tab(state, 4),
                    Key::Num6 => switch_tab(state, 5),
                    Key::Num7 => switch_tab(state, 6),
                    Key::Num8 => switch_tab(state, 7),
                    Key::Num9 => switch_tab(state, 8),
                    _ => {}
                }
            } else if modifiers.command && modifiers.shift {
                match key {
                    Key::E => events.push(UiEvent::SplitPane { vertical: false }),
                    Key::O => events.push(UiEvent::SplitPane { vertical: true }),
                    Key::F => {
                        state.main_tab = MainTab::Files;
                    }
                    Key::M => {
                        state.main_tab = MainTab::Monitor;
                    }
                    _ => {}
                }
            }
        }
    });
}

fn new_tab(state: &mut UiState, events: &mut Vec<UiEvent>) {
    if let Some(host_id) = state.selected_host.clone() {
        events.push(UiEvent::NewTabForHost(host_id));
    } else {
        state.toast(state.t("Select a host on the left first"), true);
    }
}

fn close_active_tab(state: &mut UiState, events: &mut Vec<UiEvent>) {
    if let Some(tab) = state.tabs.get(state.active_tab) {
        for pane in &tab.panes {
            events.push(UiEvent::ClosePane(pane.session));
        }
    }
}

fn switch_tab(state: &mut UiState, idx: usize) {
    if idx < state.tabs.len() {
        state.active_tab = idx;
    }
}
