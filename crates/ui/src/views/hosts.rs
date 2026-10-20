//! Left panel: host search, groups, host list, connect actions.
//! Author: Passwd Word

use opsdeck_ssh::SessionState;

use crate::{HostForm, MainTab, UiData, UiEvent, UiState};

/// Draw the left host panel.
pub fn left_panel(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
) {
    egui::Panel::left("host_panel")
        .default_size(264.0)
        .size_range(200.0..=420.0)
        .show(ui, |ui| {
            // Search
            ui.horizontal(|ui| {
                let search_hint = state.t("Search hosts  (Ctrl+P)");
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.host_search)
                        .hint_text(search_hint)
                        .desired_width(f32::INFINITY),
                );
                if state.focus_host_search {
                    resp.request_focus();
                    state.focus_host_search = false;
                }
            });
            ui.add_space(4.0);

            // Action row
            ui.horizontal(|ui| {
                if ui.small_button(state.t("＋ Add")).clicked() {
                    state.host_form = HostForm::blank();
                    state.show_host_editor = true;
                }
                if ui.small_button(state.t("⤒ Import")).clicked() {
                    state.confirm = Some(crate::ConfirmDialog {
                        title: state.t("Import hosts").to_string(),
                        message: state
                            .t("Paste the exported JSON below, then confirm.")
                            .to_string(),
                        confirm_label: state.t("Import").to_string(),
                        action: "import_hosts".to_string(),
                    });
                    state.import_buffer.clear();
                }
                if ui.small_button(state.t("⤓ Export")).clicked() {
                    events.push(UiEvent::ExportHosts);
                }
            });
            ui.add_space(4.0);
            ui.separator();

            // Favorites
            let query = state.host_search.to_lowercase();
            let favorites: Vec<_> = data
                .hosts
                .iter()
                .filter(|h| h.favorite && matches_query(h, &query))
                .collect();
            if !favorites.is_empty() {
                ui.label(egui::RichText::new(state.t("★ Favorites")).small().weak());
                for host in favorites {
                    host_row(ui, state, data, events, host);
                }
                ui.separator();
            }

            // Groups
            let ungrouped: Vec<_> = data
                .hosts
                .iter()
                .filter(|h| h.group_id.is_none() && matches_query(h, &query))
                .collect();
            for group in data.groups {
                let members: Vec<_> = data
                    .hosts
                    .iter()
                    .filter(|h| {
                        h.group_id.as_deref() == Some(group.id.as_str()) && matches_query(h, &query)
                    })
                    .collect();
                if members.is_empty() && !query.is_empty() {
                    continue;
                }
                let header = egui::CollapsingHeader::new(&group.name)
                    .default_open(true)
                    .show(ui, |ui| {
                        for host in members {
                            host_row(ui, state, data, events, host);
                        }
                    });
                let _ = header;
            }
            if !ungrouped.is_empty() {
                if !data.groups.is_empty() {
                    ui.label(egui::RichText::new(state.t("Ungrouped")).small().weak());
                }
                for host in ungrouped {
                    host_row(ui, state, data, events, host);
                }
            }

            if data.hosts.is_empty() {
                ui.add_space(12.0);
                ui.label(egui::RichText::new(state.t("No hosts yet.")).weak());
                ui.label(
                    egui::RichText::new(state.t("Click “＋ Add” to add your first server."))
                        .small()
                        .weak(),
                );
            }

            ui.add_space(8.0);
            ui.separator();

            // Sessions
            if !data.sessions.is_empty() {
                ui.label(egui::RichText::new(state.t("Sessions")).small().weak());
                for session in data.sessions {
                    ui.horizontal(|ui| {
                        let dot = match session.state {
                            SessionState::Ready => "🟢",
                            SessionState::Connecting | SessionState::Reconnecting => "🟡",
                            SessionState::Failed => "🔴",
                            _ => "⚪",
                        };
                        ui.label(format!("{dot} {}", session.name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("✕")
                                .on_hover_text(state.t("Disconnect"))
                                .clicked()
                            {
                                events.push(UiEvent::DisconnectSession(session.id));
                            }
                            if session.state == SessionState::Failed
                                && ui
                                    .small_button("↻")
                                    .on_hover_text(state.t("Reconnect"))
                                    .clicked()
                            {
                                events.push(UiEvent::ReconnectSession(session.id));
                            }
                        });
                    });
                    if let Some(err) = &session.last_error {
                        ui.label(
                            egui::RichText::new(err)
                                .small()
                                .color(egui::Color32::from_rgb(255, 120, 120)),
                        );
                    }
                }
            }
        });
}

fn matches_query(host: &opsdeck_storage::models::Host, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    host.name.to_lowercase().contains(query)
        || host.host.to_lowercase().contains(query)
        || host.username.to_lowercase().contains(query)
        || host
            .tag_list()
            .iter()
            .any(|t| t.to_lowercase().contains(query))
}

/// One host row: status dot, name, address, connect + context buttons.
fn host_row(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    host: &opsdeck_storage::models::Host,
) {
    let selected = state.selected_host.as_deref() == Some(host.id.as_str());
    let connecting = data.connecting.contains_key(&host.id);

    ui.horizontal(|ui| {
        // Favorite star
        let star = if host.favorite { "★" } else { "☆" };
        if ui
            .small_button(star)
            .on_hover_text(state.t("Toggle favorite"))
            .clicked()
        {
            events.push(UiEvent::ToggleFavorite(host.id.clone()));
        }

        let resp = ui.selectable_label(selected, egui::RichText::new(&host.name).strong());
        if resp.clicked() {
            state.selected_host = Some(host.id.clone());
        }
        if resp.double_clicked() && !connecting {
            events.push(UiEvent::ConnectHost(host.id.clone()));
        }
        resp.on_hover_text(state.tf(
            "{user}@{host}:{port}\nDouble-click to connect",
            &[
                ("user", &host.username),
                ("host", &host.host),
                ("port", &host.port.to_string()),
            ],
        ));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if connecting {
                ui.spinner();
            } else if ui
                .small_button("▶")
                .on_hover_text(state.t("Connect"))
                .clicked()
            {
                events.push(UiEvent::ConnectHost(host.id.clone()));
            }
            if ui
                .small_button("✎")
                .on_hover_text(state.t("Edit"))
                .clicked()
            {
                state.host_form = host_form_from(host, data);
                state.show_host_editor = true;
            }
            if ui
                .small_button("🗑")
                .on_hover_text(state.t("Delete"))
                .clicked()
            {
                state.confirm = Some(crate::ConfirmDialog {
                    title: state.t("Delete host").to_string(),
                    message: state.tf("Delete host “{name}”?", &[("name", &host.name)]),
                    confirm_label: state.t("Delete").to_string(),
                    action: format!("delete_host:{}", host.id),
                });
            }
        });
    });

    // Quick actions under the selected host.
    if selected {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            if ui.small_button(state.t("Terminal")).clicked() {
                state.main_tab = MainTab::Terminal;
                events.push(UiEvent::ConnectHost(host.id.clone()));
            }
            if ui.small_button(state.t("Files")).clicked() {
                state.main_tab = MainTab::Files;
                state.sftp_host = Some(host.id.clone());
                events.push(UiEvent::SftpNavigate {
                    host_id: host.id.clone(),
                    path: state.sftp_path(&host.id),
                });
            }
            if ui.small_button(state.t("Monitor")).clicked() {
                state.main_tab = MainTab::Monitor;
                events.push(UiEvent::MonitorSelect(host.id.clone()));
            }
        });
    }
}

fn host_form_from(host: &opsdeck_storage::models::Host, data: &UiData) -> HostForm {
    let _ = data;
    HostForm {
        id: Some(host.id.clone()),
        name: host.name.clone(),
        host: host.host.clone(),
        port: host.port.to_string(),
        username: host.username.clone(),
        auth_type: host.auth_type.clone(),
        group_id: host.group_id.clone().unwrap_or_default(),
        proxy_jump_id: host.proxy_jump_id.clone().unwrap_or_default(),
        startup_command: host.startup_command.clone().unwrap_or_default(),
        tags: host.tag_list().join(", "),
        favorite: host.favorite,
        password: String::new(),
        private_key_pem: String::new(),
        passphrase: String::new(),
        credential_id: host.credential_id.clone().unwrap_or_default(),
        error: None,
    }
}
