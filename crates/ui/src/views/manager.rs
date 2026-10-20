//! Connection manager: 1:1 Xterminal-style — green protocol bar, host
//! table (favorite / system / latency / name / address / live info / note /
//! actions) filtered by the left group tree + search.
//! Author: Passwd Word

use crate::{GroupFilter, HostForm, MainTab, UiData, UiEvent, UiState};

/// Xterminal brand green (header bar, Connect buttons).
pub(crate) const XGREEN: egui::Color32 = egui::Color32::from_rgb(40, 174, 96);

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
                    open_new_host(state);
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
        let hosts = filtered_hosts(state, data);
        if hosts.is_empty() {
            // No batch bar when empty; the repository switch stays.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    repo_switch(ui, state, data);
                });
            });
            protocol_empty_state(ui, state);
            return;
        }
        batch_bar(ui, state, data, events);
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("host_table")
                .striped(true)
                .spacing([14.0, 7.0])
                .show(ui, |ui| {
                    // Header: select-all checkbox, then columns; the Info
                    // header carries the rich-info toggle switch.
                    {
                        let all_selected = !hosts.is_empty()
                            && hosts.iter().all(|h| state.manager_selected.contains(&h.id));
                        let mut all = all_selected;
                        if ui.checkbox(&mut all, "").changed() {
                            if all {
                                for h in &hosts {
                                    state.manager_selected.insert(h.id.clone());
                                }
                            } else {
                                for h in &hosts {
                                    state.manager_selected.remove(&h.id);
                                }
                            }
                        }
                    }
                    for key in ["", "System", "Latency", "Name", "Address"] {
                        ui.label(egui::RichText::new(state.t(key)).small().weak());
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(state.t("Info")).small().weak());
                        toggle_switch(ui, &mut state.manager_info_on);
                    });
                    for key in ["Note", "Actions"] {
                        ui.label(egui::RichText::new(state.t(key)).small().weak());
                    }
                    ui.end_row();

                    for host in hosts {
                        let connecting = data.connecting.contains_key(&host.id);

                        // Batch checkbox
                        let mut checked = state.manager_selected.contains(&host.id);
                        if ui.checkbox(&mut checked, "").changed() {
                            if checked {
                                state.manager_selected.insert(host.id.clone());
                            } else {
                                state.manager_selected.remove(&host.id);
                            }
                        }
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
                        // System: distro badge from the real OS
                        os_badge(ui, data, host);
                        // Latency
                        latency_badge(ui, state, data, &host.id);
                        // Name: plain text (Xterminal), click selects.
                        let name_resp = ui.add(
                            egui::Label::new(egui::RichText::new(&host.name).strong())
                                .sense(egui::Sense::click()),
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
                        // Address: the IP/host only (Xterminal style);
                        // the copy button copies just the address.
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&host.host).weak().monospace().small())
                                .on_hover_text(format!(
                                    "{}@{}:{}",
                                    host.username, host.host, host.port
                                ));
                            if ui
                                .small_button("⧉")
                                .on_hover_text(state.t("Copy"))
                                .clicked()
                            {
                                ui.ctx().copy_text(host.host.clone());
                            }
                        });
                        // Info (rich metrics block or compact line)
                        if state.manager_info_on {
                            info_block(ui, state, data, host);
                        } else {
                            info_cell(ui, data, host);
                        }
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
                            if ui
                                .add(egui::Button::new(state.t("Edit")).small().frame(false))
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

/// Empty state for a protocol tab (Xterminal): big icon, "No {p}
/// connections yet", and a green create button for that protocol.
fn protocol_empty_state(ui: &mut egui::Ui, state: &mut UiState) {
    let proto = state.manager_protocol.clone();
    let pname = proto_label(&proto);
    let icon = match proto.as_str() {
        "rdp" => "🪟",
        "telnet" => "⌨",
        "vnc" => "🖱",
        _ => "🖥",
    };
    ui.add_space(70.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(icon).size(46.0).weak());
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(state.tf("No {p} connections yet", &[("p", pname)]))
                .size(16.0)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(state.t("Add your first connection with the button below."))
                .small()
                .weak(),
        );
        ui.add_space(12.0);
        let btn = egui::Button::new(
            egui::RichText::new(format!(
                "＋ {}",
                state.tf("New {p} connection", &[("p", pname)])
            ))
            .color(egui::Color32::WHITE),
        )
        .fill(XGREEN);
        if ui.add(btn).clicked() {
            open_new_host(state);
        }
    });
}

/// The green Xterminal-style header: protocol tabs + repository actions.
fn green_header(ui: &mut egui::Ui, state: &mut UiState, events: &mut Vec<UiEvent>) {
    // Xterminal top navigation: white row, plain text protocol tabs with
    // a green underline on the active one, repository actions on the right.
    ui.horizontal(|ui| {
        for (id, label) in [
            ("ssh", "SSH"),
            ("rdp", "RDP"),
            ("telnet", "Telnet"),
            ("tunnel", state.t("Tunnels")),
            ("vnc", "VNC"),
        ] {
            if proto_tab(ui, label, state.manager_protocol == id) {
                state.manager_protocol = id.to_string();
            }
            ui.add_space(6.0);
        }
        if ui
            .add(egui::Button::new("⚙").frame(false))
            .on_hover_text(state.t("Settings"))
            .clicked()
        {
            state.main_tab = MainTab::Settings;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let pw_btn = egui::Button::new(state.t("Edit repository password"));
            if ui.add(pw_btn).clicked() {
                state.main_tab = MainTab::Settings;
            }
            let import_btn = egui::Button::new(format!("⤒ {}", state.t("Import")));
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
    ui.separator();
}

/// One protocol tab in the top navigation: text with a green underline
/// when active (Xterminal style).
fn proto_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let font = egui::FontId::proportional(14.0);
    let text_w = ui
        .fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), egui::Color32::WHITE))
        .size()
        .x;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(text_w + 12.0, 28.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    let color = if selected {
        XGREEN
    } else if resp.hovered() {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        color,
    );
    if selected {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + 3.0, rect.bottom() - 2.0),
                egui::pos2(rect.right() - 3.0, rect.bottom()),
            ),
            1.0,
            XGREEN,
        );
    }
    resp.clicked()
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

/// System badge: the distro's real logo, drawn with the painter
/// (Debian swirl, Ubuntu circle-of-friends, Tux, Windows panes, …),
/// derived from the host's collected OS; protocol fallback before the
/// host has ever been connected.
fn os_badge(ui: &mut egui::Ui, data: &UiData, host: &opsdeck_storage::models::Host) {
    let hint = data
        .sysinfo
        .get(&host.id)
        .map(|i| format!("{} · {}", i.os_name, i.kernel))
        .unwrap_or_else(|| host.protocol.to_uppercase());
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(22.0, 22.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    paint_os_logo(&painter, rect, os_kind(data, host));
    resp.on_hover_text(hint);
}

/// Which logo to draw for this host.
fn os_kind(data: &UiData, host: &opsdeck_storage::models::Host) -> &'static str {
    let os = data
        .sysinfo
        .get(&host.id)
        .map(|i| i.os_name.to_lowercase())
        .unwrap_or_default();
    let has = |s: &str| os.contains(s);
    if has("debian") {
        "debian"
    } else if has("ubuntu") {
        "ubuntu"
    } else if has("raspbian") {
        "raspbian"
    } else if has("centos") {
        "centos"
    } else if has("red hat") || has("rhel") {
        "rhel"
    } else if has("alpine") {
        "alpine"
    } else if has("arch") {
        "arch"
    } else if has("fedora") {
        "fedora"
    } else if has("suse") {
        "suse"
    } else if has("windows") {
        "windows"
    } else {
        match host.protocol.as_str() {
            "rdp" => "windows",
            "vnc" => "vnc",
            "telnet" => "telnet",
            _ => "tux",
        }
    }
}

/// Draw a distro logo into `rect` (22×22 slot).
fn paint_os_logo(painter: &egui::Painter, rect: egui::Rect, kind: &str) {
    use egui::Color32;
    let c = rect.center();
    match kind {
        "debian" => {
            // Debian swirl: an Archimedean spiral in Debian red.
            let pts: Vec<egui::Pos2> = (0..=64)
                .map(|i| {
                    let t = i as f32 / 64.0;
                    let ang = t * std::f32::consts::TAU * 1.72 - std::f32::consts::FRAC_PI_2;
                    let r = 1.2 + t * 8.2;
                    c + egui::vec2(r * ang.cos(), r * ang.sin())
                })
                .collect();
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(2.0, Color32::from_rgb(215, 10, 83)),
            ));
        }
        "ubuntu" => {
            // Ubuntu: orange disc, white ring, three "friends" heads.
            let orange = Color32::from_rgb(233, 84, 32);
            painter.circle_filled(c, 10.0, orange);
            painter.circle_stroke(c, 5.1, egui::Stroke::new(2.0, Color32::WHITE));
            for k in 0..3 {
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 3.0;
                let head = c + egui::vec2(5.1 * a.cos(), 5.1 * a.sin());
                painter.circle_filled(head, 2.3, Color32::WHITE);
                painter.circle_filled(head, 1.15, orange);
            }
        }
        "windows" => {
            // Windows: four blue panes with a cross gap.
            let blue = Color32::from_rgb(0, 173, 239);
            let s = 7.6;
            let g = 0.9;
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let center = c + egui::vec2(dx * (s / 2.0 + g / 2.0), dy * (s / 2.0 + g / 2.0));
                painter.rect_filled(
                    egui::Rect::from_center_size(center, egui::vec2(s, s)),
                    0.5,
                    blue,
                );
            }
        }
        "centos" => {
            // CentOS: purple four-point star / diamond.
            let mut pts = Vec::new();
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0 - std::f32::consts::FRAC_PI_2;
                let r = if k % 2 == 0 { 10.0 } else { 4.4 };
                pts.push(c + egui::vec2(r * a.cos(), r * a.sin()));
            }
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points: pts,
                closed: true,
                fill: Color32::from_rgb(147, 34, 121),
                stroke: egui::Stroke::NONE.into(),
            }));
        }
        "arch" => {
            // Arch: the "A" silhouette in Arch blue.
            let pts = vec![
                c + egui::vec2(0.0, -10.0),
                c + egui::vec2(8.2, 9.5),
                c + egui::vec2(4.7, 9.5),
                c + egui::vec2(2.3, 2.6),
                c + egui::vec2(-2.3, 2.6),
                c + egui::vec2(-4.7, 9.5),
                c + egui::vec2(-8.2, 9.5),
            ];
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points: pts,
                closed: true,
                fill: Color32::from_rgb(23, 147, 209),
                stroke: egui::Stroke::NONE.into(),
            }));
        }
        "tux" => {
            // Generic Linux: a minimal Tux (black body, white belly,
            // orange beak and feet).
            let black = Color32::from_rgb(35, 35, 38);
            let orange = Color32::from_rgb(245, 166, 35);
            painter.circle_filled(c + egui::vec2(-3.9, 9.0), 1.9, orange);
            painter.circle_filled(c + egui::vec2(3.9, 9.0), 1.9, orange);
            painter.circle_filled(c, 9.3, black);
            painter.circle_filled(c + egui::vec2(0.0, 3.0), 5.7, Color32::WHITE);
            painter.circle_filled(c + egui::vec2(-2.5, -4.6), 1.0, Color32::WHITE);
            painter.circle_filled(c + egui::vec2(2.5, -4.6), 1.0, Color32::WHITE);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-2.4, -2.9),
                    c + egui::vec2(2.4, -2.9),
                    c + egui::vec2(0.0, -0.7),
                ],
                orange,
                egui::Stroke::NONE,
            ));
        }
        _ => {
            // Remaining distros / protocols: brand disc + white initial.
            let (letter, color) = match kind {
                "alpine" => ("A", Color32::from_rgb(13, 89, 127)),
                "fedora" => ("F", Color32::from_rgb(81, 162, 218)),
                "suse" => ("S", Color32::from_rgb(115, 186, 37)),
                "rhel" => ("R", Color32::from_rgb(238, 0, 0)),
                "raspbian" => ("R", Color32::from_rgb(162, 40, 70)),
                "vnc" => ("V", Color32::from_rgb(120, 90, 200)),
                "telnet" => ("T", Color32::from_rgb(96, 96, 96)),
                _ => ("L", Color32::from_rgb(90, 140, 90)),
            };
            painter.circle_filled(c, 9.5, color);
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                letter,
                egui::FontId::proportional(10.5),
                Color32::WHITE,
            );
        }
    }
}

/// Display label for a protocol id ("ssh" → "SSH", "telnet" → "Telnet").
pub(crate) fn proto_label(id: &str) -> &'static str {
    match id {
        "rdp" => "RDP",
        "telnet" => "Telnet",
        "vnc" => "VNC",
        _ => "SSH",
    }
}

/// Open the host editor for a new host of the current manager protocol.
pub(crate) fn open_new_host(state: &mut UiState) {
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

/// Batch action bar (Xterminal): selection count, Export / Duplicate /
/// Delete / Upload-to-cloud / Connect-all, repository switch on the right.
fn batch_bar(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    let ids: Vec<String> = state.manager_selected.iter().cloned().collect();
    let n = ids.len();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(state.tf("{n} selected", &[("n", &n.to_string())]))
                .small()
                .weak(),
        );
        ui.separator();
        if ui
            .add_enabled(
                n > 0,
                egui::Button::new(format!("⤓ {}", state.t("Export"))).small(),
            )
            .clicked()
        {
            events.push(UiEvent::ExportHostsSelected(ids.clone()));
        }
        if ui
            .add_enabled(
                n > 0,
                egui::Button::new(format!("⧉ {}", state.t("Duplicate"))).small(),
            )
            .clicked()
        {
            events.push(UiEvent::DuplicateHosts(ids.clone()));
        }
        if ui
            .add_enabled(
                n > 0,
                egui::Button::new(
                    egui::RichText::new(format!("🗑 {}", state.t("Delete")))
                        .color(egui::Color32::from_rgb(215, 70, 60)),
                )
                .small(),
            )
            .clicked()
        {
            state.pending_batch_delete = ids.clone();
            state.confirm = Some(crate::ConfirmDialog {
                title: state.t("Delete").to_string(),
                message: state.tf("Delete {n} hosts?", &[("n", &n.to_string())]),
                confirm_label: state.t("Delete").to_string(),
                action: "delete_hosts".to_string(),
            });
        }
        if ui
            .add_enabled(
                n > 0,
                egui::Button::new(format!("☁ {}", state.t("Upload to cloud"))).small(),
            )
            .clicked()
        {
            if data.sync.configured {
                events.push(UiEvent::SyncNow);
            } else {
                state.toast(
                    state.t("Set up sync in Settings to use a cloud repository"),
                    false,
                );
                state.main_tab = MainTab::Settings;
            }
        }
        if ui
            .add_enabled(
                n > 0,
                egui::Button::new(format!("▶ {}", state.t("Connect all"))).small(),
            )
            .clicked()
        {
            for id in &ids {
                events.push(UiEvent::ConnectHost(id.clone()));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            repo_switch(ui, state, data);
        });
    });
}

/// A small pill toggle switch; returns true when clicked.
fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(30.0, 16.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    let bg = if *on {
        XGREEN
    } else {
        egui::Color32::from_rgb(170, 170, 170)
    };
    painter.rect_filled(rect, 8.0, bg);
    let cx = if *on {
        rect.right() - 8.0
    } else {
        rect.left() + 8.0
    };
    painter.circle_filled(
        egui::Pos2::new(cx, rect.center().y),
        6.0,
        egui::Color32::WHITE,
    );
    if resp.clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// A slim usage bar (56×7) colored by level.
fn mini_bar(ui: &mut egui::Ui, frac: f64) {
    let frac = frac.clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(56.0, 7.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let bg = egui::Color32::from_rgba_unmultiplied(
        ui.visuals().text_color().r(),
        ui.visuals().text_color().g(),
        ui.visuals().text_color().b(),
        36,
    );
    painter.rect_filled(rect, 3.5, bg);
    let color = if frac < 0.6 {
        XGREEN
    } else if frac < 0.85 {
        egui::Color32::from_rgb(230, 160, 50)
    } else {
        egui::Color32::from_rgb(215, 80, 70)
    };
    let mut fill = rect;
    fill.set_width(rect.width() * frac as f32);
    if fill.width() > 0.5 {
        painter.rect_filled(fill, 3.5, color);
    }
}

/// Compact byte size: 973M / 10G / 3.2MB style.
fn short_bytes(v: i64) -> String {
    let v = v.max(0) as f64;
    if v >= 1e9 {
        format!("{:.1}G", v / 1e9)
    } else if v >= 1e6 {
        format!("{:.0}M", v / 1e6)
    } else if v >= 1e3 {
        format!("{:.0}K", v / 1e3)
    } else {
        format!("{v:.0}B")
    }
}

/// Rich Info block (Xterminal): cores+net on row 1, memory+disk on row 2,
/// each with a usage bar and percentage.
fn info_block(
    ui: &mut egui::Ui,
    state: &UiState,
    data: &UiData,
    host: &opsdeck_storage::models::Host,
) {
    let Some(snaps) = data.metrics.get(&host.id) else {
        info_cell(ui, data, host);
        return;
    };
    let Some(s) = snaps.last() else {
        info_cell(ui, data, host);
        return;
    };
    // Network rates from the last two snapshots.
    let (mut rx_rate, mut tx_rate) = (String::new(), String::new());
    if snaps.len() >= 2 {
        let prev = &snaps[snaps.len() - 2];
        let dt = (s.collected_at - prev.collected_at).max(1) as f64;
        if let (Some(a), Some(b)) = (s.network_rx_bytes, prev.network_rx_bytes) {
            rx_rate = short_bytes(((a - b).max(0) as f64 / dt) as i64);
        }
        if let (Some(a), Some(b)) = (s.network_tx_bytes, prev.network_tx_bytes) {
            tx_rate = short_bytes(((a - b).max(0) as f64 / dt) as i64);
        }
    }
    let small = |txt: String| egui::RichText::new(txt).small();
    let weak = |txt: String| egui::RichText::new(txt).small().weak();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let cores = data
                .sysinfo
                .get(&host.id)
                .and_then(|i| i.cpu_cores)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".to_string());
            ui.label(weak(format!("{}({cores}C)", state.t("Cores"))));
            if let Some(cpu) = s.cpu_percent {
                mini_bar(ui, cpu / 100.0);
                ui.label(small(format!("{cpu:.1}%")));
            } else {
                ui.label(weak("—".to_string()));
            }
            ui.add_space(10.0);
            ui.label(weak(state.t("Network speed").to_string()));
            if rx_rate.is_empty() && tx_rate.is_empty() {
                ui.label(weak("—".to_string()));
            } else {
                ui.label(small(format!("↑ {tx_rate} ↓ {rx_rate}")));
            }
        });
        ui.horizontal(|ui| {
            ui.label(weak(state.t("Memory").to_string()));
            if let (Some(u), Some(t)) = (s.memory_used_bytes, s.memory_total_bytes) {
                ui.label(weak(format!("({})", short_bytes(t))));
                if t > 0 {
                    mini_bar(ui, u as f64 / t as f64);
                    ui.label(small(format!("{:.1}%", u as f64 / t as f64 * 100.0)));
                }
            } else {
                ui.label(weak("—".to_string()));
            }
            ui.add_space(10.0);
            ui.label(weak(state.t("Disk").to_string()));
            if let (Some(u), Some(t)) = (s.disk_used_bytes, s.disk_total_bytes) {
                ui.label(weak(format!("({})", short_bytes(t))));
                if t > 0 {
                    mini_bar(ui, u as f64 / t as f64);
                    ui.label(small(format!("{:.1}%", u as f64 / t as f64 * 100.0)));
                }
            } else {
                ui.label(weak("—".to_string()));
            }
        });
    });
}
