//! Left panel: search, group tree (Xterminal style), sessions.
//! Author: Passwd Word

use opsdeck_ssh::SessionState;

use crate::{GroupFilter, HostForm, MainTab, UiData, UiEvent, UiState};

/// Draw the left panel: search + actions + group tree + sessions.
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
            ui.add_space(6.0);

            // Group tree ------------------------------------------------------
            ui.label(egui::RichText::new(state.t("Groups")).small().weak());
            ui.add_space(2.0);

            let total = data.hosts.len();
            if tree_row(
                ui,
                "🗂",
                state.t("All hosts"),
                total,
                state.group_filter == GroupFilter::All,
            ) {
                state.group_filter = GroupFilter::All;
                state.main_tab = MainTab::Manager;
            }
            let fav_count = data.hosts.iter().filter(|h| h.favorite).count();
            if tree_row(
                ui,
                "★",
                state.t("★ Favorites"),
                fav_count,
                state.group_filter == GroupFilter::Favorites,
            ) {
                state.group_filter = GroupFilter::Favorites;
                state.main_tab = MainTab::Manager;
            }
            ui.add_space(4.0);

            for group in data.groups {
                let count = data
                    .hosts
                    .iter()
                    .filter(|h| h.group_id.as_deref() == Some(group.id.as_str()))
                    .count();
                let selected = state.group_filter == GroupFilter::Group(group.id.clone());
                let mut delete = false;
                ui.horizontal(|ui| {
                    if tree_row(ui, "📁", &group.name, count, selected) {
                        state.group_filter = GroupFilter::Group(group.id.clone());
                        state.main_tab = MainTab::Manager;
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text(state.t("Delete"))
                        .clicked()
                    {
                        delete = true;
                    }
                });
                if delete {
                    state.confirm = Some(crate::ConfirmDialog {
                        title: state.t("Delete group").to_string(),
                        message: state.tf("Delete group “{name}”?", &[("name", &group.name)]),
                        confirm_label: state.t("Delete").to_string(),
                        action: format!("delete_group:{}", group.id),
                    });
                }
            }

            // New group inline
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let hint = state.t("New group");
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.new_group_name)
                        .hint_text(hint)
                        .desired_width(160.0),
                );
                if ui.small_button("＋").clicked() && !state.new_group_name.trim().is_empty() {
                    events.push(UiEvent::CreateGroup(
                        state.new_group_name.trim().to_string(),
                    ));
                    state.new_group_name.clear();
                }
                if resp.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !state.new_group_name.trim().is_empty()
                {
                    events.push(UiEvent::CreateGroup(
                        state.new_group_name.trim().to_string(),
                    ));
                    state.new_group_name.clear();
                }
            });

            ui.add_space(8.0);
            ui.separator();

            // Sessions ---------------------------------------------------------
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

/// One selectable tree row. Returns true when clicked.
fn tree_row(ui: &mut egui::Ui, icon: &str, label: &str, count: usize, selected: bool) -> bool {
    ui.selectable_label(
        selected,
        egui::RichText::new(format!("{icon} {label} ({count})")).strong(),
    )
    .clicked()
}

pub(crate) fn host_form_from(host: &opsdeck_storage::models::Host, data: &UiData) -> HostForm {
    let _ = data;
    HostForm {
        id: Some(host.id.clone()),
        protocol: host.protocol.clone(),
        name: host.name.clone(),
        host: host.host.clone(),
        port: host.port.to_string(),
        username: host.username.clone(),
        auth_type: host.auth_type.clone(),
        group_id: host.group_id.clone().unwrap_or_default(),
        proxy_jump_id: host.proxy_jump_id.clone().unwrap_or_default(),
        startup_command: host.startup_command.clone().unwrap_or_default(),
        note: host.note.clone().unwrap_or_default(),
        connect_timeout: host
            .connect_timeout_secs
            .map(|v| v.to_string())
            .unwrap_or_default(),
        keepalive_secs: host
            .keepalive_secs
            .map(|v| v.to_string())
            .unwrap_or_default(),
        term_type: host
            .term_type
            .clone()
            .unwrap_or_else(|| "xterm-256color".to_string()),
        default_path: host.default_path.clone().unwrap_or_default(),
        proxy_url: host.proxy_url.clone().unwrap_or_default(),
        env_vars: host
            .env_pairs()
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n"),
        keepalive_cmd: host.keepalive_cmd.clone().unwrap_or_default(),
        keepalive_cmd_secs: host
            .keepalive_cmd_secs
            .map(|v| v.to_string())
            .unwrap_or_else(|| "300".to_string()),
        tags: host.tag_list().join(", "),
        favorite: host.favorite,
        password: String::new(),
        private_key_pem: String::new(),
        passphrase: String::new(),
        credential_id: host.credential_id.clone().unwrap_or_default(),
        error: None,
    }
}
