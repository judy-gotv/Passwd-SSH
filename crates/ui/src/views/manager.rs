//! Connection manager: Xterminal-style host table (system / latency / name /
//! address / tags / actions) filtered by the left group tree + search.
//! Author: Passwd Word

use crate::{GroupFilter, HostForm, UiData, UiEvent, UiState};

/// Filter hosts by the current group filter + search query.
fn filtered_hosts<'a>(state: &UiState, data: &'a UiData) -> Vec<&'a opsdeck_storage::models::Host> {
    let query = state.host_search.to_lowercase();
    data.hosts
        .iter()
        .filter(|h| h.protocol == state.manager_protocol)
        .filter(|h| match &state.group_filter {
            GroupFilter::All => true,
            GroupFilter::Favorites => h.favorite,
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
        .collect()
}

/// Draw the manager page.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    egui::CentralPanel::default().show(ui, |ui| {
        // Protocol tabs (Xterminal top bar).
        ui.horizontal(|ui| {
            for (id, icon, label) in [
                ("ssh", "🖥", "SSH"),
                ("rdp", "🪟", "RDP"),
                ("telnet", "⌨", "Telnet"),
                ("tunnel", "⇄", state.t("Tunnels")),
                ("vnc", "🖱", "VNC"),
            ] {
                let selected = state.manager_protocol == id;
                let text = egui::RichText::new(format!("{icon} {label}"));
                if ui
                    .selectable_label(selected, if selected { text.strong() } else { text })
                    .clicked()
                {
                    state.manager_protocol = id.to_string();
                }
            }
        });
        ui.separator();
        ui.add_space(4.0);

        if state.manager_protocol == "tunnel" {
            tunnel_body(ui, state, data, events);
            return;
        }
        // Toolbar: filter title + count, refresh latency, new host.
        ui.horizontal(|ui| {
            let title: String = match &state.group_filter {
                GroupFilter::All => state.t("All hosts").to_string(),
                GroupFilter::Favorites => state.t("★ Favorites").to_string(),
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
                if ui.small_button(state.t("＋ New")).clicked() {
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
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    // Header
                    for key in ["System", "Latency", "Name", "Address", "Tags:", "Actions"] {
                        ui.label(egui::RichText::new(state.t(key)).small().weak());
                    }
                    ui.end_row();

                    for host in hosts {
                        let connecting = data.connecting.contains_key(&host.id);
                        let selected = state.selected_host.as_deref() == Some(host.id.as_str());

                        // System
                        ui.label("🖥️");
                        // Latency
                        latency_badge(ui, state, data, &host.id);
                        // Name
                        let name_resp = ui.selectable_label(
                            selected,
                            egui::RichText::new(format!(
                                "{}{}",
                                if host.favorite { "★ " } else { "" },
                                host.name
                            ))
                            .strong(),
                        );
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
                        ui.label(
                            egui::RichText::new(format!(
                                "{}@{}:{}",
                                host.username, host.host, host.port
                            ))
                            .weak()
                            .monospace()
                            .small(),
                        );
                        // Tags
                        let tags = host.tag_list().join(", ");
                        ui.label(egui::RichText::new(tags).weak().small());
                        // Actions
                        ui.horizontal(|ui| {
                            if connecting {
                                ui.spinner();
                            } else if ui
                                .small_button(state.t("Connect"))
                                .on_hover_text(state.t("Connect"))
                                .clicked()
                            {
                                state.selected_host = Some(host.id.clone());
                                events.push(UiEvent::ConnectHost(host.id.clone()));
                            }
                            if ui
                                .small_button(state.t("Edit"))
                                .on_hover_text(state.t("Edit"))
                                .clicked()
                            {
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
            let color = if *ms < 150 {
                egui::Color32::from_rgb(120, 200, 120)
            } else if *ms < 400 {
                egui::Color32::from_rgb(220, 190, 90)
            } else {
                egui::Color32::from_rgb(220, 120, 90)
            };
            ui.label(
                egui::RichText::new(format!("{ms} ms"))
                    .small()
                    .monospace()
                    .color(color),
            );
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
