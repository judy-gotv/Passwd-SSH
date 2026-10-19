//! Monitor view: host metrics charts + top processes.
//! Author: Passwd Word

use egui::{Color32, Pos2, Stroke, Vec2};

use crate::{UiData, UiEvent, UiState};

/// Draw the Monitor tab.
pub fn show(ctx: &egui::Context, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Host:");
            let current = state
                .monitor_host
                .clone()
                .and_then(|id| data.host(&id))
                .map(|h| h.name.clone())
                .unwrap_or_else(|| "— select —".to_string());
            egui::ComboBox::from_id_salt("monitor_host_picker")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for host in data.hosts {
                        if ui
                            .selectable_label(
                                state.monitor_host.as_deref() == Some(host.id.as_str()),
                                &host.name,
                            )
                            .clicked()
                        {
                            events.push(UiEvent::MonitorSelect(host.id.clone()));
                        }
                    }
                });
            ui.checkbox(&mut state.monitor_auto, "Auto refresh");
            ui.label(egui::RichText::new("Linux hosts via /proc").small().weak());
        });
        ui.separator();

        let Some(host_id) = state.monitor_host.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Select a host to monitor.").weak());
            });
            return;
        };

        let snapshots = data.metrics.get(&host_id);
        let empty: Vec<opsdeck_storage::models::MetricSnapshot> = Vec::new();
        let snapshots = snapshots.unwrap_or(&empty);

        if snapshots.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.label("Collecting metrics…");
                    ui.label(
                        egui::RichText::new(
                            "The host must be connected. Metrics are polled every few seconds.",
                        )
                        .small()
                        .weak(),
                    );
                });
            });
            return;
        }

        let latest = &snapshots[snapshots.len() - 1];

        // Summary cards
        ui.horizontal(|ui| {
            stat_card(
                ui,
                "CPU",
                latest
                    .cpu_percent
                    .map(|v| format!("{v:.1}%"))
                    .unwrap_or_else(|| "—".to_string()),
            );
            let mem = match (latest.memory_used_bytes, latest.memory_total_bytes) {
                (Some(u), Some(t)) if t > 0 => format!("{:.1}%", u as f64 / t as f64 * 100.0),
                _ => "—".to_string(),
            };
            stat_card(ui, "Memory", mem);
            let disk = match (latest.disk_used_bytes, latest.disk_total_bytes) {
                (Some(u), Some(t)) if t > 0 => format!("{:.1}%", u as f64 / t as f64 * 100.0),
                _ => "—".to_string(),
            };
            stat_card(ui, "Disk /", disk);
            stat_card(
                ui,
                "Load",
                latest
                    .load_1
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".to_string()),
            );
        });
        ui.add_space(8.0);

        // Charts
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("CPU %");
                let vals: Vec<f32> = snapshots
                    .iter()
                    .filter_map(|s| s.cpu_percent.map(|v| v as f32))
                    .collect();
                line_chart(ui, &vals, Color32::from_rgb(78, 193, 255), 0.0, 100.0);
            });
            ui.vertical(|ui| {
                ui.label("Memory %");
                let vals: Vec<f32> = snapshots
                    .iter()
                    .filter_map(|s| match (s.memory_used_bytes, s.memory_total_bytes) {
                        (Some(u), Some(t)) if t > 0 => Some(u as f32 / t as f32 * 100.0),
                        _ => None,
                    })
                    .collect();
                line_chart(ui, &vals, Color32::from_rgb(120, 220, 120), 0.0, 100.0);
            });
        });
        ui.add_space(8.0);

        // Processes
        ui.label(egui::RichText::new("Top processes").strong());
        egui::Grid::new("proc_grid")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("PID").strong());
                ui.label(egui::RichText::new("Name").strong());
                ui.label(egui::RichText::new("CPU %").strong());
                ui.label(egui::RichText::new("MEM %").strong());
                ui.end_row();
                if let Some(procs) = data.processes.get(&host_id) {
                    for p in procs.iter().take(20) {
                        ui.label(p.pid.to_string());
                        ui.label(&p.name);
                        ui.label(format!("{:.1}", p.cpu_percent));
                        ui.label(format!("{:.1}", p.mem_percent));
                        ui.end_row();
                    }
                } else {
                    ui.label(egui::RichText::new("—").weak());
                    ui.end_row();
                }
            });
    });
}

fn stat_card(ui: &mut egui::Ui, title: &str, value: String) {
    egui::Frame::NONE
        .fill(Color32::from_rgb(37, 37, 38))
        .corner_radius(6.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).small().weak());
                ui.label(egui::RichText::new(value).size(20.0).strong());
            });
        });
}

/// Simple filled line chart.
fn line_chart(ui: &mut egui::Ui, values: &[f32], color: Color32, min: f32, max: f32) {
    let (rect, _resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(420.0), 120.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 24, 24));
    if values.len() < 2 {
        return;
    }
    let span = (max - min).max(f32::EPSILON);
    let n = values.len();
    let pt = |i: usize| {
        let x = rect.left() + (i as f32 / (n - 1) as f32) * rect.width();
        let v = values[i].clamp(min, max);
        let y = rect.bottom() - ((v - min) / span) * rect.height();
        Pos2::new(x, y)
    };
    // Fill under the curve.
    let mut fill_pts: Vec<Pos2> = (0..n).map(pt).collect();
    fill_pts.push(Pos2::new(rect.right(), rect.bottom()));
    fill_pts.push(Pos2::new(rect.left(), rect.bottom()));
    painter.add(egui::Shape::convex_polygon(
        fill_pts,
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 40),
        Stroke::NONE,
    ));
    // Line.
    let line_pts: Vec<Pos2> = (0..n).map(pt).collect();
    painter.add(egui::Shape::line(line_pts, Stroke::new(1.5, color)));
}
