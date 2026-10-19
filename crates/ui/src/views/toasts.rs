//! Toast notifications.
//! Author: Passwd Word

use crate::UiState;

/// Draw active toasts bottom-right.
pub fn show(ctx: &egui::Context, state: &mut UiState) {
    if state.toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -36.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                for toast in &state.toasts {
                    let color = if toast.error {
                        egui::Color32::from_rgb(255, 90, 90)
                    } else {
                        egui::Color32::from_rgb(120, 200, 120)
                    };
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(30, 30, 30))
                        .stroke(egui::Stroke::new(1.0, color))
                        .corner_radius(6.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.label(&toast.text);
                        });
                    ui.add_space(4.0);
                }
            });
        });
}
