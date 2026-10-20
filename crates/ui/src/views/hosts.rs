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
    if state.hosts_panel_collapsed {
        egui::Panel::left("host_panel")
            .default_size(40.0)
            .size_range(40.0..=40.0)
            .show(ui, |ui| {
                if ui
                    .add(egui::Button::new("☰").frame(false))
                    .on_hover_text(state.t("Expand"))
                    .clicked()
                {
                    state.hosts_panel_collapsed = false;
                }
            });
        return;
    }
    egui::Panel::left("host_panel")
        .default_size(264.0)
        .size_range(200.0..=420.0)
        .show(ui, |ui| {
            // Tree header (Xterminal): collapse | refresh, +group, +protocol
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(format!("☰ {}", state.t("Collapse"))).frame(false))
                    .clicked()
                {
                    state.hosts_panel_collapsed = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let proto = if matches!(
                        state.manager_protocol.as_str(),
                        "ssh" | "rdp" | "telnet" | "vnc"
                    ) {
                        state.manager_protocol.clone()
                    } else {
                        "ssh".to_string()
                    };
                    if ui
                        .small_button(format!("＋{}", super::manager::proto_label(&proto)))
                        .on_hover_text(state.t("Add host"))
                        .clicked()
                    {
                        state.manager_protocol = proto;
                        super::manager::open_new_host(state);
                    }
                    if ui
                        .small_button(state.t("＋ Group"))
                        .on_hover_text(state.t("New group"))
                        .clicked()
                    {
                        state.new_group_open = !state.new_group_open;
                    }
                    if ui
                        .add(egui::Button::new("⟳").frame(false))
                        .on_hover_text(state.t("Refresh latency"))
                        .clicked()
                    {
                        events.push(UiEvent::PingHosts);
                    }
                });
            });
            if state.new_group_open {
                ui.horizontal(|ui| {
                    let hint = state.t("New group");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut state.new_group_name)
                            .hint_text(hint)
                            .desired_width(f32::INFINITY),
                    );
                    resp.request_focus();
                    if ui.small_button("＋").clicked() && !state.new_group_name.trim().is_empty() {
                        events.push(UiEvent::CreateGroup(
                            state.new_group_name.trim().to_string(),
                        ));
                        state.new_group_name.clear();
                        state.new_group_open = false;
                    }
                    if resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !state.new_group_name.trim().is_empty()
                    {
                        events.push(UiEvent::CreateGroup(
                            state.new_group_name.trim().to_string(),
                        ));
                        state.new_group_name.clear();
                        state.new_group_open = false;
                    }
                });
            }
            ui.add_space(4.0);

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
            ui.add_space(6.0);

            // Group tree ------------------------------------------------------
            ui.label(egui::RichText::new(state.t("Groups")).small().weak());
            ui.add_space(2.0);

            // In the Manager tab the tree is scoped to the active
            // protocol (Xterminal gives each protocol its own tree).
            let scoped = state.main_tab == MainTab::Manager
                && matches!(
                    state.manager_protocol.as_str(),
                    "ssh" | "rdp" | "telnet" | "vnc"
                );
            let scoped_hosts: Vec<&opsdeck_storage::models::Host> = data
                .hosts
                .iter()
                .filter(|h| !scoped || h.protocol == state.manager_protocol)
                .collect();
            let total = scoped_hosts.len();
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
            let fav_count = scoped_hosts.iter().filter(|h| h.favorite).count();
            if tree_row(
                ui,
                "★",
                state.t("Favorites"),
                fav_count,
                state.group_filter == GroupFilter::Favorites,
            ) {
                state.group_filter = GroupFilter::Favorites;
                state.main_tab = MainTab::Manager;
            }
            let recent_count = scoped_hosts
                .iter()
                .filter(|h| h.last_connected_at.is_some())
                .count();
            if tree_row(
                ui,
                "🕘",
                state.t("Recent"),
                recent_count,
                state.group_filter == GroupFilter::Recent,
            ) {
                state.group_filter = GroupFilter::Recent;
                state.main_tab = MainTab::Manager;
            }
            let freq_count = scoped_hosts.iter().filter(|h| h.connect_count > 0).count();
            if tree_row(
                ui,
                "🔥",
                state.t("Frequent"),
                freq_count,
                state.group_filter == GroupFilter::Frequent,
            ) {
                state.group_filter = GroupFilter::Frequent;
                state.main_tab = MainTab::Manager;
            }
            ui.add_space(4.0);

            for group in data.groups {
                let count = scoped_hosts
                    .iter()
                    .filter(|h| h.group_id.as_deref() == Some(group.id.as_str()))
                    .count();
                if scoped && count == 0 {
                    continue;
                }
                let selected = state.group_filter == GroupFilter::Group(group.id.clone());
                let expanded = state.tree_expanded.contains(&group.id);
                let mut delete = false;
                ui.horizontal(|ui| {
                    // Small chevron toggles the host leaves (Xterminal tree).
                    let chev = if expanded { "▾" } else { "▸" };
                    if ui
                        .add(egui::Button::new(egui::RichText::new(chev).size(10.0)).frame(false))
                        .clicked()
                    {
                        if expanded {
                            state.tree_expanded.remove(&group.id);
                        } else {
                            state.tree_expanded.insert(group.id.clone());
                        }
                    }
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
                if expanded {
                    for host in scoped_hosts
                        .iter()
                        .filter(|h| h.group_id.as_deref() == Some(group.id.as_str()))
                    {
                        ui.horizontal(|ui| {
                            ui.add_space(34.0);
                            let is_sel = state.selected_host.as_deref() == Some(host.id.as_str());
                            let resp = ui.selectable_label(
                                is_sel,
                                egui::RichText::new(format!("· {}", host.name)).small(),
                            );
                            if resp.clicked() {
                                state.selected_host = Some(host.id.clone());
                                state.group_filter = GroupFilter::Group(group.id.clone());
                                state.main_tab = MainTab::Manager;
                            }
                            if resp.double_clicked() {
                                events.push(UiEvent::ConnectHost(host.id.clone()));
                            }
                        });
                    }
                }
                if delete {
                    state.confirm = Some(crate::ConfirmDialog {
                        title: state.t("Delete group").to_string(),
                        message: state.tf("Delete group “{name}”?", &[("name", &group.name)]),
                        confirm_label: state.t("Delete").to_string(),
                        action: format!("delete_group:{}", group.id),
                    });
                }
            }

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
