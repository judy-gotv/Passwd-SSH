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
    if state.group_dialog_open {
        group_dialog(ui, state, data, events);
    }
    if state.hosts_panel_collapsed {
        egui::Panel::left("host_panel")
            .default_size(92.0)
            .size_range(92.0..=92.0)
            .show(ui, |ui| {
                ui.add_space(2.0);
                if menu_button(ui, state.t("Expand")) {
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
                if menu_button(ui, state.t("Collapse")) {
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
                        state.new_group_name.clear();
                        state.group_color = None;
                        state.group_dialog_open = true;
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
                    if let Some(hex) = group.color.as_deref().and_then(parse_hex) {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter_at(rect).circle_filled(rect.center(), 4.5, hex);
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

/// Hamburger + label button with the icon painted by hand (the bundled
/// font has no ☰ glyph — it rendered as a tofu box).
fn menu_button(ui: &mut egui::Ui, label: &str) -> bool {
    let font = egui::FontId::proportional(13.0);
    let text_w = ui
        .fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), egui::Color32::WHITE))
        .size()
        .x;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(text_w + 30.0, 22.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let color = ui.visuals().text_color();
    let x = rect.left() + 6.0;
    for i in 0..3 {
        let y = rect.center().y + (i as f32 - 1.0) * 4.0;
        painter.line_segment(
            [egui::pos2(x, y), egui::pos2(x + 11.0, y)],
            egui::Stroke::new(1.6, color),
        );
    }
    painter.text(
        egui::pos2(x + 17.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    resp.clicked()
}

/// Parse a "#rrggbb" hex color.
pub(crate) fn parse_hex(s: &str) -> Option<egui::Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

/// Color palette offered by the create-group dialog.
const GROUP_COLORS: [(&str, egui::Color32); 9] = [
    ("#e5484d", egui::Color32::from_rgb(229, 72, 77)),
    ("#f76b15", egui::Color32::from_rgb(247, 107, 21)),
    ("#ffb224", egui::Color32::from_rgb(255, 178, 36)),
    ("#46a758", egui::Color32::from_rgb(70, 167, 88)),
    ("#12a594", egui::Color32::from_rgb(18, 165, 148)),
    ("#0090ff", egui::Color32::from_rgb(0, 144, 255)),
    ("#6e56cf", egui::Color32::from_rgb(110, 86, 207)),
    ("#d6409f", egui::Color32::from_rgb(214, 64, 159)),
    ("#8b8d98", egui::Color32::from_rgb(139, 141, 152)),
];

/// Create-group dialog (Xterminal): centered modal with a name field and
/// a color palette; Confirm stays disabled until a name is typed.
fn group_dialog(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    let theme = data.theme;
    let mut open = true;
    let mut cancel = false;
    let mut confirm = false;
    egui::Window::new("group_dialog")
        .open(&mut open)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(
            egui::Frame::new()
                .fill(theme.panel())
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(
                        theme.text().r(),
                        theme.text().g(),
                        theme.text().b(),
                        40,
                    ),
                ))
                .corner_radius(8.0)
                .inner_margin(16.0)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(60),
                }),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(state.t("Create group"))
                        .strong()
                        .size(15.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("✕").frame(false))
                        .on_hover_text(state.t("Cancel"))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(format!("{}:", state.t("Name")));
                let hint = state.t("New group");
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.new_group_name)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY),
                );
                resp.request_focus();
                if resp.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !state.new_group_name.trim().is_empty()
                {
                    confirm = true;
                }
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(format!("{}:", state.t("Color")));
                // "No color" tile (square with a red slash, like the reference)
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                let painter = ui.painter_at(rect);
                painter.rect_stroke(
                    rect.shrink(1.0),
                    3.0,
                    egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
                    egui::StrokeKind::Inside,
                );
                painter.line_segment(
                    [rect.left_bottom(), rect.right_top()],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(229, 72, 77)),
                );
                if state.group_color.is_none() {
                    painter.rect_stroke(
                        rect.expand(1.5),
                        4.0,
                        egui::Stroke::new(1.6, theme.accent()),
                        egui::StrokeKind::Outside,
                    );
                }
                if resp.clicked() {
                    state.group_color = None;
                }
                for (hex, color) in GROUP_COLORS {
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                    let painter = ui.painter_at(rect);
                    painter.circle_filled(rect.center(), 8.0, color);
                    if state.group_color.as_deref() == Some(hex) {
                        painter.circle_stroke(
                            rect.center(),
                            10.0,
                            egui::Stroke::new(1.6, theme.accent()),
                        );
                    }
                    if resp.clicked() {
                        state.group_color = Some(hex.to_string());
                    }
                }
            });
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = egui::Button::new(
                    egui::RichText::new(state.t("Confirm")).color(egui::Color32::WHITE),
                )
                .fill(super::manager::XGREEN);
                if ui
                    .add_enabled(!state.new_group_name.trim().is_empty(), ok)
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button(state.t("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if !open {
        cancel = true;
    }
    if cancel {
        state.group_dialog_open = false;
    }
    if confirm && !state.new_group_name.trim().is_empty() {
        events.push(UiEvent::CreateGroup(
            state.new_group_name.trim().to_string(),
            state.group_color.clone(),
        ));
        state.new_group_name.clear();
        state.group_dialog_open = false;
    }
}
