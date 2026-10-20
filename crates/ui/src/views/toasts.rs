//! Toast notifications: theme-aware cards at the bottom center of the
//! window, where the user is looking — never dark-on-dark.
//! Author: Passwd Word

use crate::{Theme, UiState};

/// Draw active toasts bottom-center.
pub fn show(ctx: &egui::Context, state: &mut UiState, theme: &Theme) {
    if state.toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("toasts"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -42.0))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                for toast in &state.toasts {
                    let accent = if toast.error {
                        egui::Color32::from_rgb(215, 70, 60)
                    } else {
                        egui::Color32::from_rgb(40, 174, 96)
                    };
                    egui::Frame::new()
                        .fill(theme.panel())
                        .stroke(egui::Stroke::new(1.0, accent))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 2],
                            blur: 8,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(60),
                        })
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&toast.text)
                                    .color(theme.text())
                                    .size(13.5),
                            );
                        });
                    ui.add_space(6.0);
                }
            });
        });
}
