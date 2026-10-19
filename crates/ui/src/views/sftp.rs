//! SFTP browser + transfer queue view.
//! Author: Passwd Word

use opsdeck_sftp::FileKind;

use crate::{UiData, UiEvent, UiState};

/// Draw the Files tab.
pub fn show(ctx: &egui::Context, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // Host picker
        ui.horizontal(|ui| {
            ui.label("Host:");
            let current = state.sftp_host.clone().and_then(|id| data.host(&id)).map(|h| h.name.clone()).unwrap_or_else(|| "— select —".to_string());
            egui::ComboBox::from_id_salt("sftp_host_picker")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for host in data.hosts {
                        if ui.selectable_label(state.sftp_host.as_deref() == Some(host.id.as_str()), &host.name).clicked() {
                            state.sftp_host = Some(host.id.clone());
                            let path = state.sftp_path(&host.id);
                            events.push(UiEvent::SftpNavigate { host_id: host.id.clone(), path });
                        }
                    }
                });
            if ui.small_button("🔄").on_hover_text("Refresh").clicked() {
                if let Some(host_id) = state.sftp_host.clone() {
                    let path = state.sftp_path(&host_id);
                    events.push(UiEvent::SftpNavigate { host_id, path });
                }
            }
        });
        ui.separator();

        let Some(host_id) = state.sftp_host.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Select a host to browse its files.").weak());
            });
            return;
        };

        // Path bar
        let path = state.sftp_path(&host_id);
        ui.horizontal(|ui| {
            if ui.small_button("⬆").on_hover_text("Parent directory").clicked() {
                let parent = parent_of(&path);
                state.sftp_paths.insert(host_id.clone(), parent.clone());
                events.push(UiEvent::SftpNavigate { host_id: host_id.clone(), path: parent });
            }
            let mut path_edit = path.clone();
            let path_resp = ui.add(
                egui::TextEdit::singleline(&mut path_edit).desired_width(f32::INFINITY),
            );
            if path_resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                state.sftp_paths.insert(host_id.clone(), path_edit.clone());
                events.push(UiEvent::SftpNavigate { host_id: host_id.clone(), path: path_edit });
            }
            if ui.small_button("Go").clicked() {
                state.sftp_paths.insert(host_id.clone(), path.clone());
                events.push(UiEvent::SftpNavigate { host_id: host_id.clone(), path: path.clone() });
            }
            if ui.small_button("＋ Dir").clicked() {
                state.sftp_mkdir_open = true;
                state.sftp_new_dir_name.clear();
            }
        });
        if state.sftp_mkdir_open {
            ui.horizontal(|ui| {
                ui.label("New directory:");
                ui.text_edit_singleline(&mut state.sftp_new_dir_name);
                if ui.small_button("Create").clicked() && !state.sftp_new_dir_name.is_empty() {
                    let new_path = join(&path, &state.sftp_new_dir_name);
                    events.push(UiEvent::SftpMkdir { host_id: host_id.clone(), path: new_path });
                    state.sftp_mkdir_open = false;
                }
                if ui.small_button("Cancel").clicked() {
                    state.sftp_mkdir_open = false;
                }
            });
        }
        ui.separator();

        // Listing
        let listing = data.sftp_listings.get(&host_id);
        if listing.map(|l| l.loading).unwrap_or(false) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading…");
            });
        }
        if let Some(err) = listing.and_then(|l| l.error.clone()) {
            ui.colored_label(egui::Color32::from_rgb(255, 120, 120), format!("Error: {err}"));
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("sftp_grid")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Name").strong());
                    ui.label(egui::RichText::new("Size").strong());
                    ui.label(egui::RichText::new("Modified").strong());
                    ui.label(egui::RichText::new("Perms").strong());
                    ui.label(egui::RichText::new("Actions").strong());
                    ui.end_row();

                    if let Some(listing) = listing {
                        for entry in &listing.entries {
                            let icon = match entry.kind {
                                FileKind::Directory => "📁",
                                FileKind::Symlink => "🔗",
                                _ => "📄",
                            };
                            let label = format!("{icon} {}", entry.name);
                            let resp = ui.selectable_label(false, &label);
                            if resp.double_clicked() && entry.is_dir() {
                                let p = entry.path.clone();
                                state.sftp_paths.insert(host_id.clone(), p.clone());
                                events.push(UiEvent::SftpNavigate { host_id: host_id.clone(), path: p });
                            }
                            ui.label(human_size(entry.size));
                            ui.label(entry.modified_at.map(fmt_time).unwrap_or_else(|| "—".to_string()));
                            ui.label(entry.permissions.clone().unwrap_or_else(|| "—".to_string()));
                            ui.horizontal(|ui| {
                                if !entry.is_dir() && ui.small_button("⬇").on_hover_text("Download").clicked() {
                                    let local = if state.sftp_download_local.is_empty() {
                                        default_download_path(&entry.name)
                                    } else {
                                        join(&state.sftp_download_local, &entry.name)
                                    };
                                    events.push(UiEvent::SftpDownload {
                                        host_id: host_id.clone(),
                                        remote: entry.path.clone(),
                                        local,
                                    });
                                }
                                if ui.small_button("✎").on_hover_text("Rename").clicked() {
                                    state.sftp_rename_from = Some(entry.path.clone());
                                    state.sftp_rename_to = entry.name.clone();
                                }
                                if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                                    state.confirm = Some(crate::ConfirmDialog {
                                        title: "Delete".to_string(),
                                        message: format!("Delete “{}”?", entry.path),
                                        confirm_label: "Delete".to_string(),
                                        action: format!("sftp_delete:{}:{}", host_id, entry.path),
                                    });
                                }
                            });
                            ui.end_row();
                        }
                    }
                });

            // Rename inline row
            if let Some(from) = state.sftp_rename_from.clone() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(format!("Rename {} →", from));
                    ui.text_edit_singleline(&mut state.sftp_rename_to);
                    if ui.small_button("OK").clicked() {
                        let to = join(&parent_of(&from), &state.sftp_rename_to);
                        events.push(UiEvent::SftpRename { host_id: host_id.clone(), from, to });
                        state.sftp_rename_from = None;
                    }
                    if ui.small_button("Cancel").clicked() {
                        state.sftp_rename_from = None;
                    }
                });
            }

            // Upload row
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Upload local file:");
                ui.add(egui::TextEdit::singleline(&mut state.sftp_upload_local).hint_text("/path/to/local/file").desired_width(260.0));
                if ui.small_button("⬆ Upload").clicked() && !state.sftp_upload_local.is_empty() {
                    let local = state.sftp_upload_local.clone();
                    let file_name = local.rsplit('/').next().unwrap_or("upload");
                    let remote = join(&path, file_name);
                    events.push(UiEvent::SftpUpload { host_id: host_id.clone(), local, remote });
                    state.sftp_upload_local.clear();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Download dir:");
                ui.add(egui::TextEdit::singleline(&mut state.sftp_download_local).hint_text("default: ~/Downloads").desired_width(260.0));
            });
        });

        // Transfer queue
        ui.separator();
        ui.label(egui::RichText::new("Transfers").strong());
        let host_transfers: Vec<_> = data.transfers.iter().filter(|t| t.host_id == host_id).collect();
        if host_transfers.is_empty() {
            ui.label(egui::RichText::new("No transfers yet.").small().weak());
        } else {
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                for t in host_transfers {
                    ui.horizontal(|ui| {
                        let dir_icon = if t.direction == "upload" { "⬆" } else { "⬇" };
                        let file = t.remote_path.rsplit('/').next().unwrap_or(&t.remote_path);
                        ui.label(format!("{dir_icon} {file}"));
                        let total = t.bytes_total.unwrap_or(0).max(1) as f32;
                        let frac = (t.bytes_done as f32 / total).clamp(0.0, 1.0);
                        ui.add(egui::ProgressBar::new(frac).desired_width(140.0).show_percentage());
                        ui.label(status_text(t));
                        match t.status.as_str() {
                            "running" => {
                                if ui.small_button("⏸").on_hover_text("Pause").clicked() {
                                    events.push(UiEvent::TransferPause(t.id.clone()));
                                }
                            }
                            "paused" | "failed" => {
                                if ui.small_button("▶").on_hover_text("Retry").clicked() {
                                    events.push(UiEvent::TransferRetry(t.id.clone()));
                                }
                            }
                            _ => {}
                        }
                        if ui.small_button("✕").on_hover_text("Cancel").clicked() {
                            events.push(UiEvent::TransferCancel(t.id.clone()));
                        }
                    });
                    if let Some(err) = &t.error {
                        ui.label(egui::RichText::new(format!("  ↳ {err}")).small().color(egui::Color32::from_rgb(255, 120, 120)));
                    }
                }
            });
        }
    });
}

fn status_text(t: &opsdeck_storage::models::TransferTask) -> String {
    match t.status.as_str() {
        "running" => format!("{} / {}", human_size(t.bytes_done as u64), t.bytes_total.map(human_size).unwrap_or_else(|| "?".to_string())),
        "paused" => "paused".to_string(),
        "done" => "done".to_string(),
        "failed" => "failed".to_string(),
        "canceled" => "canceled".to_string(),
        s => s.to_string(),
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

fn fmt_time(ts: i64) -> String {
    // Simple UTC rendering without extra deps.
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

// Howard Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn parent_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
    }
}

fn join(dir: &str, name: &str) -> String {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn default_download_path(file_name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    format!("{home}/Downloads/{file_name}")
}
