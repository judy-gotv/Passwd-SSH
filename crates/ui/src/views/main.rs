//! Main layout: menu bar, central content, status bar, global shortcuts.
//! Author: Passwd Word

use crate::{MainTab, UiData, UiEvent, UiState};

/// Top menu bar.
pub fn top_bar(
    ctx: &egui::Context,
    state: &mut UiState,
    _data: &UiData,
    events: &mut Vec<UiEvent>,
) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New terminal tab    Ctrl+T").clicked() {
                    new_tab(state, events);
                    ui.close_menu();
                }
                if ui.button("Import hosts…").clicked() {
                    state.confirm = Some(crate::ConfirmDialog {
                        title: "Import hosts".to_string(),
                        message: "Paste the exported JSON below, then confirm.".to_string(),
                        confirm_label: "Import".to_string(),
                        action: "import_hosts".to_string(),
                    });
                    state.import_buffer.clear();
                    ui.close_menu();
                }
                if ui.button("Export hosts…").clicked() {
                    events.push(UiEvent::ExportHosts);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("🔒 Lock vault").clicked() {
                    events.push(UiEvent::LockVault);
                    ui.close_menu();
                }
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.close_menu();
                }
            });

            ui.menu_button("View", |ui| {
                for (tab, label) in [
                    (MainTab::Terminal, "Terminal"),
                    (MainTab::Files, "Files (SFTP)"),
                    (MainTab::Monitor, "Monitor"),
                    (MainTab::Snippets, "Snippets"),
                    (MainTab::Settings, "Settings"),
                ] {
                    if ui.selectable_label(state.main_tab == tab, label).clicked() {
                        state.main_tab = tab;
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.menu_button("Theme", |ui| {
                    for name in ["Dark", "Light"] {
                        if ui
                            .selectable_label(state.theme_name == name, name)
                            .clicked()
                        {
                            state.theme_name = name.to_string();
                            events.push(UiEvent::SetTheme(name.to_string()));
                            ui.close_menu();
                        }
                    }
                });
            });

            ui.menu_button("Session", |ui| {
                if ui.button("Split left/right    Ctrl+Shift+E").clicked() {
                    events.push(UiEvent::SplitPane { vertical: false });
                    ui.close_menu();
                }
                if ui.button("Split up/down    Ctrl+Shift+O").clicked() {
                    events.push(UiEvent::SplitPane { vertical: true });
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Port forwarding…").clicked() {
                    state.show_forward_dialog = true;
                    ui.close_menu();
                }
                if ui.button("Reconnect session").clicked() {
                    if let Some(pane) = state.active_pane().cloned() {
                        events.push(UiEvent::ReconnectSession(pane.session));
                    }
                    ui.close_menu();
                }
            });

            ui.menu_button("Help", |ui| {
                if ui.button("Keyboard shortcuts").clicked() {
                    state.main_tab = MainTab::Settings;
                    ui.close_menu();
                }
                if ui.button("Check for updates").clicked() {
                    events.push(UiEvent::CheckUpdate);
                    ui.close_menu();
                }
            });

            // Right side: tab switcher
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (tab, label) in [
                    (MainTab::Terminal, "Terminal"),
                    (MainTab::Files, "Files"),
                    (MainTab::Monitor, "Monitor"),
                    (MainTab::Snippets, "Snippets"),
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
pub fn central(ctx: &egui::Context, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    match state.main_tab {
        MainTab::Terminal => super::terminal::show(ctx, state, data, events),
        MainTab::Files => super::sftp::show(ctx, state, data, events),
        MainTab::Monitor => super::monitor::show(ctx, state, data, events),
        MainTab::Snippets => super::snippets::show(ctx, state, data, events),
        MainTab::Settings => super::settings::show(ctx, state, data, events),
    }
}

/// Bottom status bar.
pub fn status_bar(ctx: &egui::Context, state: &mut UiState, data: &UiData) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("{} session(s)", data.sessions.len()));
            if data.sessions.is_empty() {
                ui.label(egui::RichText::new("not connected").weak());
            }
            if !data.key_prompts.is_empty() {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 200, 90),
                    format!("⚠ {} host key approval(s) pending", data.key_prompts.len()),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if data.update.available {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 220, 120),
                        format!(
                            "⬆ Update available: {}",
                            data.update.latest.clone().unwrap_or_default()
                        ),
                    );
                }
                if data.sync.configured {
                    let txt = data
                        .sync
                        .last_summary
                        .clone()
                        .unwrap_or_else(|| "sync on".to_string());
                    ui.label(egui::RichText::new(format!("🔄 {txt}")).small().weak());
                }
                if !data.forwards.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("⇄ {} forward(s)", data.forwards.len()))
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
                        if state.main_tab == MainTab::Terminal {
                            if let Some(pane) = state.active_pane().cloned() {
                                let open = !state
                                    .search_open
                                    .get(&pane.session)
                                    .copied()
                                    .unwrap_or(false);
                                state.search_open.insert(pane.session, open);
                            }
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
        state.toast("Select a host on the left first", true);
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
