//! Modal dialogs: host editor, confirm, host-key approval,
//! port-forward editor, snippet editor.
//! Author: Passwd Word

use opsdeck_ssh::HostKeyDecision;

use crate::{HostForm, UiData, UiEvent, UiState};

/// Draw all dialogs.
pub fn show_all(
    ctx: &egui::Context,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
) {
    host_key_prompts(ctx, state, data, events);
    if state.show_host_editor {
        host_editor(ctx, state, data, events);
    }
    if let Some(confirm) = state.confirm.clone() {
        confirm_dialog(ctx, state, events, &confirm);
    }
    if state.show_forward_dialog {
        forward_dialog(ctx, state, data, events);
    }
    if state.show_snippet_editor {
        snippet_editor(ctx, state, events);
    }
    super::filespanel::overlays(ctx, state, events);
}

// ---------------------------------------------------------------------------
// Host-key approval
// ---------------------------------------------------------------------------

fn host_key_prompts(
    ctx: &egui::Context,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
) {
    for prompt in data.key_prompts {
        let mut decision: Option<HostKeyDecision> = None;
        egui::Window::new(state.tf(
                "SSH host key: {host}:{port}",
                &[("host", &prompt.host), ("port", &prompt.port.to_string())],
            ))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if prompt.changed {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 120, 90),
                        state.t("⚠ WARNING: the host key CHANGED. This may indicate a man-in-the-middle attack."),
                    );
                } else {
                    ui.label(state.t("This server is not in your known-hosts database:"));
                }
                ui.add_space(4.0);
                ui.label(state.tf(
                    "Host: {host}:{port}",
                    &[("host", &prompt.host), ("port", &prompt.port.to_string())],
                ));
                ui.label(state.tf("Key type: {key_type}", &[("key_type", &prompt.key_type)]));
                ui.label(state.t("Fingerprint (OpenSSH):"));
                let mut key_text = prompt.key_openssh.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut key_text)
                        .desired_rows(2)
                        .desired_width(420.0),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(state.t("Trust once")).clicked() {
                        decision = Some(HostKeyDecision::TrustOnce);
                    }
                    if ui.button(state.t("Trust always")).clicked() {
                        decision = Some(HostKeyDecision::TrustAlways);
                    }
                    if ui.button(state.t("Reject")).clicked() {
                        decision = Some(HostKeyDecision::Reject);
                    }
                });
            });
        if let Some(d) = decision {
            let _ = state;
            events.push(UiEvent::HostKeyDecision {
                prompt_id: prompt.id,
                decision: d,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Host editor
// ---------------------------------------------------------------------------

fn host_editor(ctx: &egui::Context, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    let is_new = state.host_form.id.is_none();
    let mut save = false;
    let mut save_connect = false;
    let mut cancel = false;
    let mut test = false;
    let mut open = true;

    egui::Window::new(if is_new {
        state.t("Add host")
    } else {
        state.t("Edit host")
    })
    .collapsible(false)
    .auto_sized()
    .default_width(660.0)
    // Never taller than the screen: the action bar must stay reachable.
    .max_height((ctx.content_rect().height() - 16.0).max(320.0))
    .open(&mut open)
    .show(ctx, |ui| {
        // The form scrolls; the action bar below it is always visible.
        let scroll_h = (ui.ctx().content_rect().height() - 190.0).max(200.0);
        egui::ScrollArea::vertical()
            .id_salt("host_editor_scroll")
            .max_height(scroll_h)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // Section navigation (Xterminal-style).
                    ui.vertical(|ui| {
                        ui.set_width(120.0);
                        let is_ssh = state.host_form.protocol == "ssh";
                        let mut sections = vec![
                            (0usize, state.t("Basic info")),
                            (1, state.t("Connection")),
                            (4, state.t("Other")),
                        ];
                        if is_ssh {
                            sections.insert(2, (2, state.t("Jump host")));
                            sections.insert(3, (3, state.t("Proxy")));
                            sections.push((5, state.t("Init")));
                        }
                        for (idx, label) in sections {
                            if ui
                                .selectable_label(state.host_editor_section == idx, label)
                                .clicked()
                            {
                                state.host_editor_section = idx;
                            }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| match state.host_editor_section {
                        0 => section_basic(ui, state, data),
                        1 => section_connection(ui, state),
                        2 => section_jump(ui, state, data),
                        3 => section_proxy(ui, state),
                        4 => section_other(ui, state),
                        _ => section_init(ui, state),
                    });
                });
            });

        if let Some(err) = state.host_form.error.clone() {
            ui.colored_label(egui::Color32::from_rgb(255, 120, 120), err);
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(state.t("Test connection")).clicked() {
                test = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(state.t("Save")).clicked() {
                    save = true;
                }
                if ui.button(state.t("Save and connect")).clicked() {
                    save_connect = true;
                }
                if ui.button(state.t("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    });

    if !open {
        cancel = true;
    }
    if test {
        events.push(UiEvent::TestConnection(Box::new(state.host_form.clone())));
    }
    if save || save_connect {
        // Validate.
        let err = if state.host_form.name.trim().is_empty() {
            Some(state.t("Name is required.").to_string())
        } else if state.host_form.host.trim().is_empty() {
            Some(state.t("Host is required.").to_string())
        } else if state.host_form.port.trim().parse::<u16>().is_err() {
            Some(state.t("Port must be a number.").to_string())
        } else if state.host_form.username.trim().is_empty() {
            Some(state.t("Username is required.").to_string())
        } else {
            None
        };
        if let Some(e) = err {
            state.host_form.error = Some(e);
        } else {
            state.host_form.error = None;
            state.show_host_editor = false;
            let form = Box::new(std::mem::take(&mut state.host_form));
            if save_connect {
                events.push(UiEvent::SaveHostConnect(form));
            } else {
                events.push(UiEvent::SaveHost(form));
            }
            state.host_form = HostForm::blank();
        }
    }
    if cancel {
        state.show_host_editor = false;
        state.host_form = HostForm::blank();
    }
}

/// Default port per protocol.
fn default_port(protocol: &str) -> u16 {
    match protocol {
        "telnet" => 23,
        "vnc" => 5900,
        "rdp" => 3389,
        _ => 22,
    }
}

/// Basic info: identity, group, auth, credential, note.
fn section_basic(ui: &mut egui::Ui, state: &mut UiState, data: &UiData) {
    egui::Grid::new("host_basic_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(state.t("Protocol:"));
            ui.horizontal(|ui| {
                for (id, label) in [
                    ("ssh", "SSH"),
                    ("telnet", "Telnet"),
                    ("vnc", "VNC"),
                    ("rdp", "RDP"),
                ] {
                    if ui
                        .selectable_label(state.host_form.protocol == id, label)
                        .clicked()
                    {
                        state.host_form.protocol = id.to_string();
                        if state.host_form.id.is_none() {
                            state.host_form.port = default_port(id).to_string();
                        }
                    }
                }
            });
            ui.end_row();
            ui.label(state.t("Name:"));
            ui.text_edit_singleline(&mut state.host_form.name);
            ui.end_row();
            ui.label(state.t("Host:"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.host_form.host).desired_width(200.0));
                ui.label(state.t("Port:"));
                ui.add(egui::TextEdit::singleline(&mut state.host_form.port).desired_width(60.0));
            });
            ui.end_row();
            ui.label(state.t("Group:"));
            egui::ComboBox::from_id_salt("host_form_group")
                .selected_text(
                    data.groups
                        .iter()
                        .find(|g| g.id == state.host_form.group_id)
                        .map(|g| g.name.clone())
                        .unwrap_or_else(|| state.t("— none —").to_string()),
                )
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(state.host_form.group_id.is_empty(), state.t("— none —"))
                        .clicked()
                    {
                        state.host_form.group_id.clear();
                    }
                    for g in data.groups {
                        if ui
                            .selectable_label(state.host_form.group_id == g.id, &g.name)
                            .clicked()
                        {
                            state.host_form.group_id = g.id.clone();
                        }
                    }
                });
            ui.end_row();
            ui.label(state.t("Auth:"));
            ui.horizontal(|ui| {
                for (id, label) in [
                    ("password", state.t("Password")),
                    ("private_key", state.t("Key")),
                    ("agent", state.t("Agent")),
                ] {
                    if ui
                        .selectable_label(state.host_form.auth_type == id, label)
                        .clicked()
                    {
                        state.host_form.auth_type = id.to_string();
                    }
                }
            });
            ui.end_row();
            ui.label(state.t("Username:"));
            ui.text_edit_singleline(&mut state.host_form.username);
            ui.end_row();
        });

    ui.add_space(4.0);
    ui.label(egui::RichText::new(state.t("Credential")).strong());
    if !data.credentials.is_empty() {
        egui::ComboBox::from_id_salt("host_form_cred")
            .selected_text(
                data.credentials
                    .iter()
                    .find(|c| c.id == state.host_form.credential_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| state.t("— new —").to_string()),
            )
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.host_form.credential_id.is_empty(), state.t("— new —"))
                    .clicked()
                {
                    state.host_form.credential_id.clear();
                }
                for c in data.credentials {
                    if ui
                        .selectable_label(state.host_form.credential_id == c.id, &c.name)
                        .clicked()
                    {
                        state.host_form.credential_id = c.id.clone();
                    }
                }
            });
    }
    if state.host_form.credential_id.is_empty() {
        match state.host_form.auth_type.as_str() {
            "password" => {
                ui.horizontal(|ui| {
                    ui.label(state.t("Password:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.host_form.password).password(true),
                    );
                });
            }
            "private_key" => {
                ui.label(state.t("Private key (PEM):"));
                ui.add(
                    egui::TextEdit::multiline(&mut state.host_form.private_key_pem)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    ui.label(state.t("Passphrase:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.host_form.passphrase).password(true),
                    );
                });
            }
            _ => {
                ui.label(
                    egui::RichText::new(state.t("Uses your local SSH agent (no stored secret)."))
                        .small()
                        .weak(),
                );
            }
        }
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let fav_label = state.t("★ Favorite");
        ui.checkbox(&mut state.host_form.favorite, fav_label);
    });
    ui.horizontal(|ui| {
        ui.label(state.t("Note:"));
        ui.text_edit_singleline(&mut state.host_form.note);
    });
}

/// Connection settings: timeout, keepalive, terminal type.
fn section_connection(ui: &mut egui::Ui, state: &mut UiState) {
    egui::Grid::new("host_conn_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(state.t("Connect timeout (sec):"));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.host_form.connect_timeout)
                        .desired_width(80.0),
                );
                ui.label(
                    egui::RichText::new(state.t("empty = default"))
                        .small()
                        .weak(),
                );
            });
            ui.end_row();
            if state.host_form.protocol == "ssh" {
                ui.label(state.t("Keepalive (sec):"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.host_form.keepalive_secs)
                            .desired_width(80.0),
                    );
                    ui.label(
                        egui::RichText::new(state.t("0 = off, empty = 30"))
                            .small()
                            .weak(),
                    );
                });
                ui.end_row();
            }
            ui.label(state.t("Terminal type:"));
            ui.text_edit_singleline(&mut state.host_form.term_type);
            ui.end_row();
        });
}

/// Jump host picker.
fn section_jump(ui: &mut egui::Ui, state: &mut UiState, data: &UiData) {
    ui.horizontal(|ui| {
        ui.label(state.t("Jump host:"));
        egui::ComboBox::from_id_salt("host_form_jump")
            .selected_text(
                data.hosts
                    .iter()
                    .find(|h| h.id == state.host_form.proxy_jump_id)
                    .map(|h| h.name.clone())
                    .unwrap_or_else(|| state.t("— none —").to_string()),
            )
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(
                        state.host_form.proxy_jump_id.is_empty(),
                        state.t("— none —"),
                    )
                    .clicked()
                {
                    state.host_form.proxy_jump_id.clear();
                }
                for h in data.hosts {
                    if Some(h.id.as_str()) == state.host_form.id.as_deref() {
                        continue;
                    }
                    if ui
                        .selectable_label(state.host_form.proxy_jump_id == h.id, &h.name)
                        .clicked()
                    {
                        state.host_form.proxy_jump_id = h.id.clone();
                    }
                }
            });
    });
}

/// Proxy settings.
fn section_proxy(ui: &mut egui::Ui, state: &mut UiState) {
    ui.horizontal(|ui| {
        ui.label(state.t("Proxy URL:"));
        ui.add(
            egui::TextEdit::singleline(&mut state.host_form.proxy_url)
                .hint_text("socks5://127.0.0.1:1080")
                .desired_width(280.0),
        );
    });
    ui.label(
        egui::RichText::new(state.t("HTTP CONNECT and SOCKS5 are supported."))
            .small()
            .weak(),
    );
}

/// Other settings: tags, default path.
fn section_other(ui: &mut egui::Ui, state: &mut UiState) {
    egui::Grid::new("host_other_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(state.t("Tags (comma separated):"));
            ui.text_edit_singleline(&mut state.host_form.tags);
            ui.end_row();
            ui.label(state.t("Default path:"));
            ui.add(
                egui::TextEdit::singleline(&mut state.host_form.default_path)
                    .hint_text("/var/www")
                    .desired_width(240.0),
            );
            ui.end_row();
        });
}

/// Init: startup command, keepalive command, environment variables.
fn section_init(ui: &mut egui::Ui, state: &mut UiState) {
    egui::Grid::new("host_init_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(state.t("Startup cmd:"));
            ui.add(
                egui::TextEdit::singleline(&mut state.host_form.startup_command)
                    .desired_width(280.0),
            );
            ui.end_row();
            ui.label(state.t("Keepalive command:"));
            ui.add(
                egui::TextEdit::singleline(&mut state.host_form.keepalive_cmd)
                    .hint_text("ls")
                    .desired_width(280.0),
            );
            ui.end_row();
            ui.label(state.t("Keepalive period (sec):"));
            ui.add(
                egui::TextEdit::singleline(&mut state.host_form.keepalive_cmd_secs)
                    .desired_width(80.0),
            );
            ui.end_row();
        });
    ui.add_space(4.0);
    ui.label(state.t("Env vars (KEY=value per line):"));
    ui.add(
        egui::TextEdit::multiline(&mut state.host_form.env_vars)
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
}

// ---------------------------------------------------------------------------
// Generic confirm
// ---------------------------------------------------------------------------

fn confirm_dialog(
    ctx: &egui::Context,
    state: &mut UiState,
    events: &mut Vec<UiEvent>,
    confirm: &crate::ConfirmDialog,
) {
    let mut done: Option<bool> = None;
    egui::Window::new(&confirm.title)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(&confirm.message);
            if confirm.action == "import_hosts" {
                let import_hint = state.t("Paste JSON here…");
                ui.add(
                    egui::TextEdit::multiline(&mut state.import_buffer)
                        .hint_text(import_hint)
                        .desired_rows(8)
                        .desired_width(420.0),
                );
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(state.t("Cancel")).clicked() {
                    done = Some(false);
                }
                if ui.button(&confirm.confirm_label).clicked() {
                    done = Some(true);
                }
            });
        });

    match done {
        Some(true) => {
            let action = confirm.action.clone();
            state.confirm = None;
            dispatch_confirm_action(state, events, &action);
        }
        Some(false) => {
            state.confirm = None;
        }
        None => {}
    }
}

fn dispatch_confirm_action(state: &mut UiState, events: &mut Vec<UiEvent>, action: &str) {
    if action == "import_hosts" {
        let json = std::mem::take(&mut state.import_buffer);
        if json.trim().is_empty() {
            state.toast(state.t("Nothing to import"), true);
        } else {
            events.push(UiEvent::ImportHosts(json));
        }
        return;
    }
    let mut parts = action.splitn(3, ':');
    match parts.next().unwrap_or("") {
        "delete_host" => {
            if let Some(id) = parts.next() {
                events.push(UiEvent::DeleteHost(id.to_string()));
            }
        }
        "delete_group" => {
            if let Some(id) = parts.next() {
                events.push(UiEvent::DeleteGroup(id.to_string()));
            }
        }
        "noop" => {}
        "delete_credential" => {
            if let Some(id) = parts.next() {
                events.push(UiEvent::DeleteCredential(id.to_string()));
            }
        }
        "delete_snippet" => {
            if let Some(id) = parts.next() {
                events.push(UiEvent::DeleteSnippet(id.to_string()));
            }
        }
        "sftp_delete" => {
            let host_id = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            if !host_id.is_empty() && !path.is_empty() {
                events.push(UiEvent::SftpDelete { host_id, path });
            }
        }
        other => {
            state.toast(
                state.tf("Unknown action: {other}", &[("other", other)]),
                true,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Port forwarding
// ---------------------------------------------------------------------------

fn forward_dialog(
    ctx: &egui::Context,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
) {
    let mut start = false;
    let mut cancel = false;
    egui::Window::new(state.t("New port forward (local → remote)"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(state.t("Session:"));
                let current = state
                    .forward_session
                    .and_then(|s| data.session(&s))
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| state.t("— select —").to_string());
                egui::ComboBox::from_id_salt("forward_session")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for s in data.sessions {
                            if ui
                                .selectable_label(state.forward_session == Some(s.id), &s.name)
                                .clicked()
                            {
                                state.forward_session = Some(s.id);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(state.t("Local bind:"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.forward_bind)
                        .hint_text("127.0.0.1:8080")
                        .desired_width(200.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label(state.t("Remote host:"));
                ui.text_edit_singleline(&mut state.forward_remote_host);
                ui.label(state.t("port:"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.forward_remote_port).desired_width(80.0),
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(state.t("Start")).clicked() {
                    start = true;
                }
                if ui.button(state.t("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if cancel {
        state.show_forward_dialog = false;
    }
    if start {
        match (
            state.forward_session,
            state.forward_remote_port.trim().parse::<u16>(),
        ) {
            (Some(session), Ok(port)) if !state.forward_remote_host.trim().is_empty() => {
                state.show_forward_dialog = false;
                events.push(UiEvent::StartForward {
                    session,
                    bind: state.forward_bind.trim().to_string(),
                    remote_host: state.forward_remote_host.trim().to_string(),
                    remote_port: port,
                });
            }
            _ => {
                state.toast(
                    state.t("Pick a session and enter a valid remote host/port"),
                    true,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Snippet editor
// ---------------------------------------------------------------------------

fn snippet_editor(ctx: &egui::Context, state: &mut UiState, events: &mut Vec<UiEvent>) {
    let is_new = state.snippet_form.id.is_none();
    let mut save = false;
    let mut cancel = false;
    egui::Window::new(if is_new {
        state.t("New snippet")
    } else {
        state.t("Edit snippet")
    })
    .collapsible(false)
    .default_width(440.0)
    .show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(state.t("Title:"));
            ui.text_edit_singleline(&mut state.snippet_form.title);
        });
        ui.label(state.t("Command:"));
        ui.add(
            egui::TextEdit::multiline(&mut state.snippet_form.command)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.label(state.t("Description:"));
            ui.text_edit_singleline(&mut state.snippet_form.description);
        });
        ui.horizontal(|ui| {
            ui.label(state.t("Tags:"));
            ui.text_edit_singleline(&mut state.snippet_form.tags);
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(state.t("Save")).clicked() {
                save = true;
            }
            if ui.button(state.t("Cancel")).clicked() {
                cancel = true;
            }
        });
    });
    if save {
        if state.snippet_form.title.trim().is_empty()
            || state.snippet_form.command.trim().is_empty()
        {
            state.toast(state.t("Title and command are required"), true);
        } else {
            state.show_snippet_editor = false;
            events.push(UiEvent::SaveSnippet(std::mem::take(
                &mut state.snippet_form,
            )));
        }
    }
    if cancel {
        state.show_snippet_editor = false;
    }
}
