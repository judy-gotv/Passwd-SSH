//! Session monitor dashboard (Xterminal-style right panel): system facts,
//! CPU bar, memory donut, network rates + sparkline, disk, top processes.
//! Author: Passwd Word

use egui::{Color32, Pos2, Stroke};

use crate::{MainTab, UiData, UiState};

/// Draw the monitoring dashboard for `host_id` as a right-side panel.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, host_id: &str) {
    egui::Panel::right("term_monitor")
        .default_size(288.0)
        .size_range(230.0..=430.0)
        .frame(egui::Frame::new().fill(data.theme.bg()).inner_margin(8.0))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                dashboard_body(ui, state, data, host_id);
            });
        });
}

fn dashboard_body(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, host_id: &str) {
    let theme = data.theme;
    let accent = Color32::from_rgb(theme.accent[0], theme.accent[1], theme.accent[2]);

    // System card
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(state.t("System")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(h) = data.host(host_id) {
                    chip(ui, theme, &h.host);
                }
            });
        });
        ui.add_space(4.0);
        match data.sysinfo.get(host_id) {
            Some(info) => {
                ui.label(egui::RichText::new(&info.os_name).small());
                if !info.kernel.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("{}: {}", state.t("Kernel"), info.kernel))
                            .small()
                            .weak(),
                    );
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{}: {}",
                        state.t("Uptime"),
                        fmt_uptime(info.uptime_secs)
                    ))
                    .small()
                    .weak(),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new(state.t("Collecting metrics…"))
                        .small()
                        .weak(),
                );
            }
        }
    });

    let snapshots = data.metrics.get(host_id);
    let latest = snapshots.and_then(|v| v.last());
    let (Some(snaps), Some(l)) = (snapshots, latest) else {
        ui.add_space(6.0);
        card(ui, theme, |ui| {
            ui.label(
                egui::RichText::new(state.t("Collecting metrics…"))
                    .small()
                    .weak(),
            );
        });
        return;
    };

    // CPU card
    ui.add_space(6.0);
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(state.t("CPU")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(
                        l.cpu_percent
                            .map(|v| format!("{v:.1}%"))
                            .unwrap_or_else(|| "—".to_string()),
                    )
                    .strong(),
                );
            });
        });
        ui.add_space(4.0);
        let ratio = (l.cpu_percent.unwrap_or(0.0) / 100.0) as f32;
        ui.add(egui::ProgressBar::new(ratio.clamp(0.0, 1.0)).fill(accent));
        if let Some(load) = l.load_1 {
            ui.label(
                egui::RichText::new(format!(
                    "{}: {:.2} / {:.2} / {:.2}",
                    state.t("Load"),
                    load,
                    l.load_5.unwrap_or(0.0),
                    l.load_15.unwrap_or(0.0)
                ))
                .small()
                .weak(),
            );
        }
    });

    // Memory card (donut)
    ui.add_space(6.0);
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(state.t("Memory")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(total) = l.memory_total_bytes {
                    chip(ui, theme, &fmt_bytes(total));
                }
            });
        });
        ui.add_space(4.0);
        match (l.memory_used_bytes, l.memory_total_bytes) {
            (Some(used), Some(total)) if total > 0 => {
                let ratio = used as f32 / total as f32;
                ui.horizontal(|ui| {
                    donut(ui, ratio, accent, &format!("{:.0}%", ratio * 100.0));
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}: {}",
                                state.t("Used"),
                                fmt_bytes(used)
                            ))
                            .small(),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{}: {}",
                                state.t("Total"),
                                fmt_bytes(total)
                            ))
                            .small()
                            .weak(),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{}: {}",
                                state.t("Free"),
                                fmt_bytes(total - used)
                            ))
                            .small()
                            .weak(),
                        );
                    });
                });
                let _ = ();
            }
            _ => {
                ui.label(egui::RichText::new("—").weak());
            }
        }
    });

    // Network card
    ui.add_space(6.0);
    card(ui, theme, |ui| {
        ui.label(egui::RichText::new(state.t("Network")).strong());
        ui.add_space(4.0);
        let (rx_rate, tx_rate, rx_series) = net_rates(snaps);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "↑ {}: {}/s ({})",
                    state.t("Upload"),
                    fmt_bytes_num(tx_rate),
                    fmt_bytes(l.network_tx_bytes.unwrap_or(0))
                ))
                .small(),
            );
        });
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "↓ {}: {}/s ({})",
                    state.t("Download"),
                    fmt_bytes_num(rx_rate),
                    fmt_bytes(l.network_rx_bytes.unwrap_or(0))
                ))
                .small(),
            );
        });
        if rx_series.len() >= 2 {
            ui.add_space(4.0);
            sparkline(ui, &rx_series, Color32::from_rgb(120, 220, 120));
        }
    });

    // Disk card
    ui.add_space(6.0);
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(state.t("Disk usage")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let (Some(u), Some(t)) = (l.disk_used_bytes, l.disk_total_bytes) {
                    chip(ui, theme, &format!("{} / {}", fmt_bytes(u), fmt_bytes(t)));
                }
            });
        });
        ui.add_space(4.0);
        match (l.disk_used_bytes, l.disk_total_bytes) {
            (Some(used), Some(total)) if total > 0 => {
                ui.label(
                    egui::RichText::new(format!("/: {} / {}", fmt_bytes(used), fmt_bytes(total)))
                        .small(),
                );
                ui.add_space(2.0);
                ui.add(
                    egui::ProgressBar::new((used as f32 / total as f32).clamp(0.0, 1.0))
                        .fill(accent),
                );
                let _ = ();
            }
            _ => {
                ui.label(egui::RichText::new("—").weak());
            }
        }
    });

    // Processes card
    ui.add_space(6.0);
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(state.t("Processes")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(state.t("View all")).clicked() {
                    state.monitor_host = Some(host_id.to_string());
                    state.main_tab = MainTab::Monitor;
                }
            });
        });
        ui.add_space(4.0);
        if let Some(procs) = data.processes.get(host_id) {
            egui::Grid::new("dash_procs")
                .num_columns(3)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(state.t("Name")).small().weak());
                    ui.label(egui::RichText::new(state.t("CPU %")).small().weak());
                    ui.label(egui::RichText::new(state.t("MEM %")).small().weak());
                    ui.end_row();
                    for p in procs.iter().take(6) {
                        ui.label(egui::RichText::new(&p.name).small());
                        ui.label(
                            egui::RichText::new(format!("{:.1}%", p.cpu_percent))
                                .small()
                                .weak(),
                        );
                        ui.label(
                            egui::RichText::new(format!("{:.1}%", p.mem_percent))
                                .small()
                                .weak(),
                        );
                        ui.end_row();
                    }
                });
        } else {
            ui.label(egui::RichText::new("—").weak());
        }
    });
}

/// Card frame: raised theme surface with a visible border and a soft
/// shadow, so sections read as distinct cards on the page background.
fn card(ui: &mut egui::Ui, theme: &crate::Theme, add: impl FnOnce(&mut egui::Ui)) {
    let border =
        Color32::from_rgba_unmultiplied(theme.text().r(), theme.text().g(), theme.text().b(), 26);
    egui::Frame::new()
        .fill(theme.panel())
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(8.0)
        .inner_margin(10.0)
        .shadow(egui::epaint::Shadow {
            offset: [0, 1],
            blur: 5,
            spread: 0,
            color: Color32::from_black_alpha(28),
        })
        .show(ui, add);
}

/// Small right-aligned value chip used in card headers.
fn chip(ui: &mut egui::Ui, theme: &crate::Theme, text: &str) {
    let bg =
        Color32::from_rgba_unmultiplied(theme.text().r(), theme.text().g(), theme.text().b(), 16);
    egui::Frame::new()
        .fill(bg)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().weak());
        });
}

/// Donut ring showing `ratio`, with centered text.
fn donut(ui: &mut egui::Ui, ratio: f32, color: Color32, center_text: &str) {
    let size = 84.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let center = rect.center();
    let radius = size / 2.0 - 5.0;
    let painter = ui.painter_at(rect);
    let track = ui.visuals().weak_text_color().gamma_multiply(0.3);
    painter.circle_stroke(center, radius, Stroke::new(9.0, track));
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = ratio.clamp(0.0, 1.0) * std::f32::consts::TAU;
    let steps = 48usize;
    let pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = start + sweep * (i as f32 / steps as f32);
            center + egui::vec2(radius * a.cos(), radius * a.sin())
        })
        .collect();
    if sweep > 0.0 {
        painter.add(egui::Shape::line(pts, Stroke::new(9.0, color)));
    }
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        center_text,
        egui::FontId::proportional(15.0),
        ui.visuals().text_color(),
    );
}

/// Small filled sparkline for a rate series.
fn sparkline(ui: &mut egui::Ui, values: &[f32], color: Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    if values.len() < 2 {
        return;
    }
    let max = values.iter().copied().fold(1.0f32, f32::max);
    let n = values.len();
    let pt = |i: usize| {
        let x = rect.left() + (i as f32 / (n - 1) as f32) * rect.width();
        let y = rect.bottom() - (values[i] / max) * (rect.height() - 4.0) - 2.0;
        Pos2::new(x, y)
    };
    let mut fill: Vec<Pos2> = (0..n).map(pt).collect();
    fill.push(Pos2::new(rect.right(), rect.bottom()));
    fill.push(Pos2::new(rect.left(), rect.bottom()));
    painter.add(egui::Shape::convex_polygon(
        fill,
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 45),
        Stroke::NONE,
    ));
    let line: Vec<Pos2> = (0..n).map(pt).collect();
    painter.add(egui::Shape::line(line, Stroke::new(1.5, color)));
}

/// (rx rate, tx rate, rx-rate series) from snapshot history.
fn net_rates(snaps: &[opsdeck_storage::models::MetricSnapshot]) -> (u64, u64, Vec<f32>) {
    let mut series = Vec::new();
    for w in snaps.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let dt = (b.collected_at - a.collected_at).max(1) as f64;
        if let (Some(r1), Some(r2)) = (a.network_rx_bytes, b.network_rx_bytes) {
            series.push(((r2 - r1).max(0) as f64 / dt) as f32);
        }
    }
    let last = snaps.windows(2).last();
    let (rx, tx) = match last {
        Some(w) => {
            let (a, b) = (&w[0], &w[1]);
            let dt = (b.collected_at - a.collected_at).max(1) as f64;
            let rx = match (a.network_rx_bytes, b.network_rx_bytes) {
                (Some(x), Some(y)) => ((y - x).max(0) as f64 / dt) as u64,
                _ => 0,
            };
            let tx = match (a.network_tx_bytes, b.network_tx_bytes) {
                (Some(x), Some(y)) => ((y - x).max(0) as f64 / dt) as u64,
                _ => 0,
            };
            (rx, tx)
        }
        None => (0, 0),
    };
    (rx, tx, series)
}

/// Human byte size: 973.2M / 29.7G style.
pub fn fmt_bytes(bytes: i64) -> String {
    fmt_bytes_num(bytes.max(0) as u64)
}

fn fmt_bytes_num(mut v: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut f = v as f64;
    let mut unit = 0;
    while f >= 1024.0 && unit < UNITS.len() - 1 {
        f /= 1024.0;
        unit += 1;
        v /= 1024;
    }
    if unit == 0 {
        format!("{v}B")
    } else {
        format!("{f:.1}{}", UNITS[unit])
    }
}

/// Compact uptime: "39d 15h" / "2h 5m" / "12m".
fn fmt_uptime(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}
