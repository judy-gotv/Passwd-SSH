//! Connection manager: 1:1 Xterminal-style — green protocol bar, host
//! table (favorite / system / latency / name / address / live info / note /
//! actions) filtered by the left group tree + search.
//! Author: Passwd Word

use crate::{GroupFilter, HostForm, MainTab, UiData, UiEvent, UiState};

/// Xterminal brand green (header bar, Connect buttons).
pub(crate) const XGREEN: egui::Color32 = egui::Color32::from_rgb(40, 174, 96);
const XGREEN_DARK: egui::Color32 = egui::Color32::from_rgb(30, 140, 75);

/// Filter hosts by the current group filter + search query.
fn filtered_hosts<'a>(state: &UiState, data: &'a UiData) -> Vec<&'a opsdeck_storage::models::Host> {
    let query = state.host_search.to_lowercase();
    let mut hosts: Vec<&opsdeck_storage::models::Host> = data
        .hosts
        .iter()
        .filter(|h| h.protocol == state.manager_protocol)
        .filter(|h| match &state.group_filter {
            GroupFilter::All => true,
            GroupFilter::Favorites => h.favorite,
            GroupFilter::Recent => h.last_connected_at.is_some(),
            GroupFilter::Frequent => h.connect_count > 0,
            GroupFilter::Group(gid) => h.group_id.as_deref() == Some(gid.as_str()),
        })
        .filter(|h| {
            query.is_empty()
                || h.name.to_lowercase().contains(&query)
                || h.host.to_lowercase().contains(&query)
                || h.username.to_lowercase().contains(&query)
                || h.tag_list()
                    .iter()
                    .any(|t| t.to_lowercase().contains(&query))
        })
        .collect();
    match state.group_filter {
        GroupFilter::Recent => {
            hosts.sort_by_key(|h| std::cmp::Reverse(h.last_connected_at.unwrap_or(0)));
        }
        GroupFilter::Frequent => {
            hosts.sort_by_key(|h| std::cmp::Reverse(h.connect_count));
        }
        _ => {}
    }
    hosts
}

/// Draw the manager page.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    egui::CentralPanel::default().show(ui, |ui| {
        green_header(ui, state, events);
        ui.add_space(6.0);

        if state.manager_protocol == "tunnel" {
            tunnel_body(ui, state, data, events);
            return;
        }
        // Toolbar: filter title + count | repository switch, refresh, new.
        ui.horizontal(|ui| {
            let title: String = match &state.group_filter {
                GroupFilter::All => state.t("All hosts").to_string(),
                GroupFilter::Favorites => state.t("★ Favorites").to_string(),
                GroupFilter::Recent => state.t("Recent").to_string(),
                GroupFilter::Frequent => state.t("Frequent").to_string(),
                GroupFilter::Group(gid) => data
                    .groups
                    .iter()
                    .find(|g| g.id == *gid)
                    .map(|g| g.name.clone())
                    .unwrap_or_else(|| state.t("All hosts").to_string()),
            };
            ui.label(egui::RichText::new(&title).strong());
            let hosts = filtered_hosts(state, data);
            ui.label(egui::RichText::new(format!("({})", hosts.len())).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let new_btn = egui::Button::new(
                    egui::RichText::new(state.t("＋ New")).color(egui::Color32::WHITE),
                )
                .fill(XGREEN);
                if ui.add(new_btn).clicked() {
                    state.host_form = HostForm::blank();
                    state.host_form.protocol = state.manager_protocol.clone();
                    state.host_form.port = match state.manager_protocol.as_str() {
                        "telnet" => "23".to_string(),
                        "vnc" => "5900".to_string(),
                        "rdp" => "3389".to_string(),
                        _ => "22".to_string(),
                    };
                    state.host_editor_section = 0;
                    state.show_host_editor = true;
                }
                if ui
                    .small_button(format!("🔄 {}", state.t("Refresh")))
                    .on_hover_text(state.t("Refresh latency"))
                    .clicked()
                {
                    events.push(UiEvent::PingHosts);
                }
                repo_switch(ui, state, data);
            });
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        let hosts = filtered_hosts(state, data);
        if hosts.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(state.t("No hosts yet.")).weak());
                ui.label(
                    egui::RichText::new(state.t("Click “＋ New” to add your first server."))
                        .small()
                        .weak(),
                );
            });
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("host_table")
                .striped(true)
                .spacing([14.0, 7.0])
                .show(ui, |ui| {
                    // Header
                    ui.label("");
                    for key in [
                        "System", "Latency", "Name", "Address", "Info", "Note", "Actions",
                    ] {
                        ui.label(egui::RichText::new(state.t(key)).small().weak());
                    }
                    ui.end_row();

                    for host in hosts {
                        let connecting = data.connecting.contains_key(&host.id);
                        let selected = state.selected_host.as_deref() == Some(host.id.as_str());

                        // Favorite star
                        let star = if host.favorite {
                            egui::RichText::new("★").color(egui::Color32::from_rgb(240, 180, 40))
                        } else {
                            egui::RichText::new("☆").weak()
                        };
                        if ui
                            .selectable_label(false, star)
                            .on_hover_text(state.t("Toggle favorite"))
                            .clicked()
                        {
                            events.push(UiEvent::ToggleFavorite(host.id.clone()));
                        }
                        // System
                        let (icon, os_hint) = os_badge(data, host);
                        ui.label(icon).on_hover_text(os_hint);
                        // Latency
                        latency_badge(ui, state, data, &host.id);
                        // Name
                        let name_resp =
                            ui.selectable_label(selected, egui::RichText::new(&host.name).strong());
                        if name_resp.clicked() {
                            state.selected_host = Some(host.id.clone());
                        }
                        if name_resp.double_clicked() && !connecting {
                            events.push(UiEvent::ConnectHost(host.id.clone()));
                        }
                        name_resp.on_hover_text(state.tf(
                            "{user}@{host}:{port}\nDouble-click to connect",
                            &[
                                ("user", &host.username),
                                ("host", &host.host),
                                ("port", &host.port.to_string()),
                            ],
                        ));
                        // Address
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}@{}:{}",
                                    host.username, host.host, host.port
                                ))
                                .weak()
                                .monospace()
                                .small(),
                            );
                            if ui
                                .small_button("⧉")
                                .on_hover_text(state.t("Copy"))
                                .clicked()
                            {
                                ui.ctx().copy_text(format!(
                                    "{}@{}:{}",
                                    host.username, host.host, host.port
                                ));
                            }
                        });
                        // Info (live metrics for connected hosts)
                        info_cell(ui, data, host);
                        // Note (falls back to tags)
                        let note = host
                            .note
                            .clone()
                            .filter(|n| !n.trim().is_empty())
                            .unwrap_or_else(|| host.tag_list().join(", "));
                        ui.label(egui::RichText::new(note).weak().small());
                        // Actions
                        ui.horizontal(|ui| {
                            if connecting {
                                ui.spinner();
                            } else {
                                let connect_btn = egui::Button::new(
                                    egui::RichText::new(state.t("Connect"))
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(XGREEN);
                                if ui.add(connect_btn).clicked() {
                                    state.selected_host = Some(host.id.clone());
                                    events.push(UiEvent::ConnectHost(host.id.clone()));
                                }
                            }
                            if ui.small_button(state.t("Edit")).clicked() {
                                state.host_form = super::hosts::host_form_from(host, data);
                                state.show_host_editor = true;
                            }
                            ui.menu_button(state.t("More"), |ui| {
                                if ui.button(state.t("Toggle favorite")).clicked() {
                                    events.push(UiEvent::ToggleFavorite(host.id.clone()));
                                    ui.close();
                                }
                                if ui.button(state.t("Delete")).clicked() {
                                    state.confirm = Some(crate::ConfirmDialog {
                                        title: state.t("Delete host").to_string(),
                                        message: state
                                            .tf("Delete host “{name}”?", &[("name", &host.name)]),
                                        confirm_label: state.t("Delete").to_string(),
                                        action: format!("delete_host:{}", host.id),
                                    });
                                    ui.close();
                                }
                            });
                        });
                        ui.end_row();
                    }
                });
        });
    });
}

/// The green Xterminal-style header: protocol tabs + repository actions.
fn green_header(ui: &mut egui::Ui, state: &mut UiState, events: &mut Vec<UiEvent>) {
    egui::Frame::new()
        .fill(XGREEN)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (id, icon, label) in [
                    ("ssh", "🖥", "SSH"),
                    ("rdp", "🪟", "RDP"),
                    ("telnet", "⌨", "Telnet"),
                    ("tunnel", "⇄", state.t("Tunnels")),
                    ("vnc", "🖱", "VNC"),
                ] {
                    let selected = state.manager_protocol == id;
                    let mut text =
                        egui::RichText::new(format!("{icon} {label}")).color(egui::Color32::WHITE);
                    if selected {
                        text = text
                            .background_color(egui::Color32::WHITE)
                            .color(XGREEN_DARK)
                            .strong();
                    }
                    if ui.selectable_label(selected, text).clicked() {
                        state.manager_protocol = id.to_string();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pw_btn = egui::Button::new(
                        egui::RichText::new(state.t("Edit repository password")).color(XGREEN_DARK),
                    )
                    .fill(egui::Color32::WHITE);
                    if ui.add(pw_btn).clicked() {
                        state.main_tab = MainTab::Settings;
                    }
                    let import_btn = egui::Button::new(
                        egui::RichText::new(format!("⤒ {}", state.t("Import"))).color(XGREEN_DARK),
                    )
                    .fill(egui::Color32::WHITE);
                    if ui.add(import_btn).clicked() {
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
                    let _ = events;
                });
            });
        });
}

/// Local / cloud repository segmented switch (mirrors the sync setting).
fn repo_switch(ui: &mut egui::Ui, state: &mut UiState, data: &UiData) {
    let cloud = data.sync.configured;
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, XGREEN))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(!cloud, format!("🗄 {}", state.t("Local repository")))
                    .clicked()
                    && cloud
                {
                    state.main_tab = MainTab::Settings;
                }
                if ui
                    .selectable_label(cloud, format!("☁ {}", state.t("Cloud repository")))
                    .clicked()
                    && !cloud
                {
                    state.toast(
                        state.t("Set up sync in Settings to use a cloud repository"),
                        false,
                    );
                    state.main_tab = MainTab::Settings;
                }
            });
        });
}

/// OS / protocol badge for the System column.
fn os_badge(data: &UiData, host: &opsdeck_storage::models::Host) -> (&'static str, String) {
    let hint = data
        .sysinfo
        .get(&host.id)
        .map(|i| format!("{} · {}", i.os_name, i.kernel))
        .unwrap_or_else(|| host.protocol.to_uppercase());
    let icon = match host.protocol.as_str() {
        "rdp" => "🪟",
        "vnc" => "🖥",
        "telnet" => "⌨",
        _ => "🐧",
    };
    (icon, hint)
}

/// Live metrics summary for a connected host; OS name when only static
/// facts are known; an em dash otherwise.
fn info_cell(ui: &mut egui::Ui, data: &UiData, host: &opsdeck_storage::models::Host) {
    if let Some(s) = data.metrics.get(&host.id).and_then(|snaps| snaps.last()) {
        let cpu = s
            .cpu_percent
            .map(|v| format!("CPU {v:.0}%"))
            .unwrap_or_default();
        let mem = s
            .memory_ratio()
            .map(|r| format!("MEM {:.0}%", r * 100.0))
            .unwrap_or_default();
        let disk = match (s.disk_used_bytes, s.disk_total_bytes) {
            (Some(u), Some(t)) if t > 0 => format!("Disk {:.0}%", u as f64 / t as f64 * 100.0),
            _ => String::new(),
        };
        let line = [cpu, mem, disk]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        if line.is_empty() {
            ui.label(egui::RichText::new("—").weak().small());
        } else {
            ui.label(egui::RichText::new(line).small());
        }
    } else if let Some(info) = data.sysinfo.get(&host.id) {
        ui.label(egui::RichText::new(&info.os_name).weak().small());
    } else {
        ui.label(egui::RichText::new("—").weak().small());
    }
}

/// Latency badge: colored ms / Unreachable / not measured.
fn latency_badge(ui: &mut egui::Ui, state: &UiState, data: &UiData, host_id: &str) {
    match data.latencies.get(host_id) {
        None => {
            ui.label(egui::RichText::new("—").weak().small());
        }
        Some(None) => {
            ui.label(
                egui::RichText::new(state.t("Unreachable"))
                    .small()
                    .color(egui::Color32::from_rgb(200, 90, 90)),
            );
        }
        Some(Some(ms)) => {
            let (fill, fg) = if *ms < 150 {
                (
                    egui::Color32::from_rgb(225, 245, 228),
                    egui::Color32::from_rgb(30, 140, 75),
                )
            } else if *ms < 400 {
                (
                    egui::Color32::from_rgb(250, 240, 210),
                    egui::Color32::from_rgb(160, 120, 20),
                )
            } else {
                (
                    egui::Color32::from_rgb(250, 225, 218),
                    egui::Color32::from_rgb(180, 80, 60),
                )
            };
            egui::Frame::new()
                .fill(fill)
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(6, 1))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("⏱ {ms} ms"))
                            .small()
                            .monospace()
                            .color(fg),
                    );
                });
        }
    }
}

/// Active port forwards (the Tunnels tab).
fn tunnel_body(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(state.t("Tunnels")).strong());
        ui.label(egui::RichText::new(format!("({})", data.forwards.len())).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(state.t("New port forward")).clicked() {
                state.show_forward_dialog = true;
            }
        });
    });
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            state.t(
                "Forwards live as long as their SSH session. Start one from a connected session.",
            ),
        )
        .small()
        .weak(),
    );
    ui.add_space(6.0);
    if data.forwards.is_empty() {
        ui.label(egui::RichText::new(state.t("No active forwards")).weak());
        return;
    }
    egui::Grid::new("tunnel_table")
        .striped(true)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            for key in ["Session", "Local", "Remote", "Actions"] {
                ui.label(egui::RichText::new(state.t(key)).small().weak());
            }
            ui.end_row();
            for f in data.forwards {
                ui.label(egui::RichText::new(&f.session_name).strong());
                ui.label(egui::RichText::new(&f.local).monospace().small());
                ui.label(egui::RichText::new(&f.remote).monospace().small());
                if ui.small_button(state.t("Stop")).clicked() {
                    events.push(UiEvent::StopForward(f.id));
                }
                ui.end_row();
            }
        });
}
