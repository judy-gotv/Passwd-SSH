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
                    chip_copy(ui, state, theme, &h.host);
                }
            });
        });
        ui.add_space(4.0);
        match data.sysinfo.get(host_id) {
            Some(info) => {
                let (os_main, os_ver) = split_os(&info.os_name);
                ui.horizontal(|ui| {
                    outline_chip(ui, &os_main);
                    if let Some(v) = &os_ver {
                        outline_chip(ui, v);
                    }
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(state.t("Timezone")).small().weak());
                    if let Some(tz) = &info.timezone {
                        ui.label(
                            egui::RichText::new(tz)
                                .small()
                                .color(super::filespanel::XGREEN),
                        );
                    }
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new(state.t("Uptime")).small().weak());
                    ui.label(
                        egui::RichText::new(fmt_uptime(state, info.uptime_secs))
                            .small()
                            .color(super::filespanel::XGREEN),
                    );
                });
                if !info.kernel.is_empty() {
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(format!("{}: {}", state.t("Kernel"), info.kernel))
                            .small()
                            .weak(),
                    );
                }
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
        let series: Vec<f32> = snaps
            .iter()
            .filter_map(|s| s.cpu_percent.map(|v| v as f32))
            .collect();
        if series.len() >= 2 {
            sparkline(ui, &series, super::filespanel::XGREEN);
            ui.add_space(4.0);
        }
        if !l.cpu_per_core.is_empty() {
            core_strip(ui, &l.cpu_per_core);
        } else {
            let ratio = (l.cpu_percent.unwrap_or(0.0) / 100.0) as f32;
            ui.add(egui::ProgressBar::new(ratio.clamp(0.0, 1.0)).fill(accent));
        }
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
                let cached = l.memory_cached_bytes.unwrap_or(0).min(total - used);
                let free = total - used - cached;
                let ratio = used as f32 / total as f32;
                ui.horizontal(|ui| {
                    donut3(
                        ui,
                        used as f32 / total as f32,
                        cached as f32 / total as f32,
                        &format!("{:.0}%", ratio * 100.0),
                    );
                    ui.vertical(|ui| {
                        for (color, label, value) in [
                            (egui::Color32::from_rgb(229, 72, 77), state.t("Used"), used),
                            (
                                egui::Color32::from_rgb(154, 154, 160),
                                state.t("Cached"),
                                cached,
                            ),
                            (super::filespanel::XGREEN, state.t("Free"), free),
                        ] {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(8.0, 8.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter_at(rect).circle_filled(rect.center(), 3.5, color);
                                ui.label(
                                    egui::RichText::new(format!("{}  {}", label, fmt_bytes(value)))
                                        .small(),
                                );
                            });
                        }
                    });
                });
                let _ = accent;
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
        if rx_series.len() >= 2 {
            sparkline(ui, &rx_series, super::filespanel::XGREEN);
            ui.add_space(4.0);
        }
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(state.t("Traffic")).small().weak());
                ui.add_space(26.0);
                ui.label(egui::RichText::new(state.t("Speed")).small().weak());
            });
        });
        for (color, label, rate, total) in [
            (
                super::filespanel::XGREEN,
                state.t("Upload"),
                tx_rate,
                l.network_tx_bytes.unwrap_or(0),
            ),
            (
                Color32::from_rgb(0, 144, 255),
                state.t("Download"),
                rx_rate,
                l.network_rx_bytes.unwrap_or(0),
            ),
        ] {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter_at(rect).circle_filled(rect.center(), 3.5, color);
                ui.label(egui::RichText::new(label).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(fmt_bytes(total)).small());
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(format!("{}/s", fmt_bytes_num(rate))).small());
                });
            });
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
        let mounts: &[opsdeck_ops::MountInfo] = data
            .sysinfo
            .get(host_id)
            .map(|i| i.mounts.as_slice())
            .unwrap_or(&[]);
        if mounts.is_empty() {
            match (l.disk_used_bytes, l.disk_total_bytes) {
                (Some(used), Some(total)) if total > 0 => {
                    ui.label(
                        egui::RichText::new(format!(
                            "/: {} / {}",
                            fmt_bytes(used),
                            fmt_bytes(total)
                        ))
                        .small(),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::ProgressBar::new((used as f32 / total as f32).clamp(0.0, 1.0))
                            .fill(accent),
                    );
                }
                _ => {
                    ui.label(egui::RichText::new("—").weak());
                }
            }
        } else {
            if let Some(root) = mounts.iter().find(|m| m.mount == "/") {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter_at(rect).circle_filled(
                        rect.center(),
                        3.5,
                        super::filespanel::XGREEN,
                    );
                    ui.label(egui::RichText::new("/").small().strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        chip(ui, theme, &root.fstype);
                    });
                });
                ui.add_space(4.0);
            }
            let (rr, wr) = disk_rates(snaps);
            ui.horizontal(|ui| {
                for (label, value) in [(state.t("Read"), rr), (state.t("Write"), wr)] {
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(format!("{label}/s")).small().weak());
                            ui.label(egui::RichText::new(fmt_bytes_num(value)).small().strong());
                        });
                }
            });
            ui.add_space(6.0);
            egui::Grid::new("dash_mounts")
                .num_columns(4)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for key in ["Mount", "Size", "Avail", "Used %"] {
                        ui.label(egui::RichText::new(state.t(key)).small().weak());
                    }
                    ui.end_row();
                    for m in mounts {
                        let pct = if m.total > 0 {
                            m.used as f64 / m.total as f64 * 100.0
                        } else {
                            0.0
                        };
                        ui.label(egui::RichText::new(&m.mount).small());
                        ui.label(egui::RichText::new(fmt_bytes(m.total as i64)).small());
                        ui.label(egui::RichText::new(fmt_bytes(m.avail as i64)).small());
                        let color = if pct < 60.0 {
                            super::filespanel::XGREEN
                        } else if pct < 85.0 {
                            Color32::from_rgb(230, 150, 40)
                        } else {
                            Color32::from_rgb(220, 60, 60)
                        };
                        egui::Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(
                                color.r(),
                                color.g(),
                                color.b(),
                                26,
                            ))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(5, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{pct:.0}%"))
                                        .small()
                                        .color(color),
                                );
                            });
                        ui.end_row();
                    }
                });
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
                        ui.vertical(|ui| {
                            let name = if p.name.chars().count() > 26 {
                                format!("{}…", p.name.chars().take(26).collect::<String>())
                            } else {
                                p.name.clone()
                            };
                            ui.label(egui::RichText::new(name).small());
                            ui.label(egui::RichText::new(format!("PID {}", p.pid)).small().weak());
                        });
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

/// Uptime in the reference style: "64 天 3 小时" / "2 小时" / "12 分钟".
fn fmt_uptime(state: &UiState, secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d} {} {h} {}", state.t("days"), state.t("hours"))
    } else if h > 0 {
        format!("{h} {}", state.t("hours"))
    } else {
        format!("{m} {}", state.t("minutes"))
    }
}

/// Split "Debian GNU/Linux 12 (bookworm)" → ("Debian GNU/Linux",
/// Some("12 (bookworm)")) for the two OS chips.
fn split_os(os: &str) -> (String, Option<String>) {
    let Some(p) = os.find(" (") else {
        return (os.to_string(), None);
    };
    let head = &os[..p];
    let tail = &os[p + 1..];
    // The version starts at the first token beginning with a digit.
    let mut byte_idx = 0;
    for tok in head.split(' ') {
        if tok.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
            let main = head[..byte_idx].trim_end().to_string();
            return (main, Some(format!("{} {}", &head[byte_idx..], tail)));
        }
        byte_idx += tok.len() + 1;
    }
    (head.to_string(), Some(tail.to_string()))
}

/// Green-outlined pill used for the OS name / version chips.
fn outline_chip(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, super::filespanel::XGREEN))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small());
        });
}

/// IP chip that copies the address when clicked.
fn chip_copy(ui: &mut egui::Ui, state: &mut UiState, theme: &crate::Theme, text: &str) {
    let bg =
        Color32::from_rgba_unmultiplied(theme.text().r(), theme.text().g(), theme.text().b(), 16);
    let btn = egui::Button::new(egui::RichText::new(format!("{text} ⧉")).small())
        .fill(bg)
        .corner_radius(4.0);
    if ui.add(btn).on_hover_text(state.t("Copy")).clicked() {
        ui.ctx().copy_text(text.to_string());
        state.toast(state.t("Copied"), false);
    }
}

/// Per-core strip: one thin vertical bar per core, filled by busy %.
fn core_strip(ui: &mut egui::Ui, cores: &[f64]) {
    let n = cores.len().max(1);
    let avail = ui.available_width();
    let gap = 3.0;
    let w = ((avail - gap * (n as f32 - 1.0)) / n as f32).clamp(2.0, 7.0);
    let total_w = w * n as f32 + gap * (n as f32 - 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, 15.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let track = ui.visuals().weak_text_color().gamma_multiply(0.25);
    for (i, pct) in cores.iter().enumerate() {
        let x = rect.left() + i as f32 * (w + gap);
        let bar =
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, rect.height()));
        painter.rect_filled(bar, 1.0, track);
        let p = (*pct as f32 / 100.0).clamp(0.0, 1.0);
        let color = if *pct < 60.0 {
            super::filespanel::XGREEN
        } else if *pct < 85.0 {
            Color32::from_rgb(230, 150, 40)
        } else {
            Color32::from_rgb(220, 60, 60)
        };
        let filled = egui::Rect::from_min_max(
            egui::pos2(x, rect.bottom() - rect.height() * p),
            bar.right_bottom(),
        );
        if p > 0.0 {
            painter.rect_filled(filled, 1.0, color);
        }
    }
}

/// Three-segment donut: used (red) / cached (gray) / free (green).
fn donut3(ui: &mut egui::Ui, used_r: f32, cached_r: f32, center_text: &str) {
    let size = 84.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let center = rect.center();
    let radius = size / 2.0 - 5.0;
    let painter = ui.painter_at(rect);
    let segments = [
        (used_r, Color32::from_rgb(229, 72, 77)),
        (cached_r, Color32::from_rgb(154, 154, 160)),
        (
            (1.0 - used_r - cached_r).max(0.0),
            super::filespanel::XGREEN,
        ),
    ];
    let mut start = -std::f32::consts::FRAC_PI_2;
    for (ratio, color) in segments {
        let sweep = ratio.clamp(0.0, 1.0) * std::f32::consts::TAU;
        if sweep > 0.01 {
            let steps = 40usize;
            let pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let a = start + sweep * (i as f32 / steps as f32);
                    center + egui::vec2(radius * a.cos(), radius * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(9.0, color)));
        }
        start += sweep;
    }
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        center_text,
        egui::FontId::proportional(15.0),
        ui.visuals().text_color(),
    );
}

/// (read rate, write rate) bytes/sec from the last two snapshots.
fn disk_rates(snaps: &[opsdeck_storage::models::MetricSnapshot]) -> (u64, u64) {
    let Some(w) = snaps.windows(2).last() else {
        return (0, 0);
    };
    let (a, b) = (&w[0], &w[1]);
    let dt = (b.collected_at - a.collected_at).max(1) as f64;
    let rate = |x: Option<i64>, y: Option<i64>| match (x, y) {
        (Some(p), Some(q)) => ((q - p).max(0) as f64 / dt) as u64,
        _ => 0,
    };
    (
        rate(a.disk_read_bytes, b.disk_read_bytes),
        rate(a.disk_write_bytes, b.disk_write_bytes),
    )
}
