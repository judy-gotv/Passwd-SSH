//! Vault lock screen: first-run setup and unlock.
//! Author: Passwd Word

use egui::TextEdit;

use crate::{Screen, UiData, UiEvent, UiState};

/// Draw the lock screen.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, _data: &UiData, events: &mut Vec<UiEvent>) {
    let first_run = matches!(state.screen, Screen::Locked { first_run: true });

    egui::CentralPanel::default().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(90.0);
            ui.heading(egui::RichText::new("OpsDeck").size(36.0));
            ui.label("SSH Workstation");
            ui.add_space(30.0);

            let width = 320.0;
            ui.set_max_width(width);

            if first_run {
                ui.label("Create a master password. It encrypts all stored credentials.");
                ui.add_space(8.0);
                let pw = TextEdit::singleline(&mut state.password_input)
                    .password(true)
                    .hint_text("Master password")
                    .desired_width(width);
                let resp = ui.add(pw);
                let confirm = TextEdit::singleline(&mut state.password_confirm)
                    .password(true)
                    .hint_text("Confirm password")
                    .desired_width(width);
                ui.add(confirm);
                if resp.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                    try_create(state, events);
                }
                ui.add_space(8.0);
                if ui.button("Create vault").clicked() {
                    try_create(state, events);
                }
            } else {
                ui.label("Enter your master password to unlock.");
                ui.add_space(8.0);
                let pw = TextEdit::singleline(&mut state.password_input)
                    .password(true)
                    .hint_text("Master password")
                    .desired_width(width);
                let resp = ui.add(pw);
                // Focus the field on first show.
                if state.unlock_error.is_none() && state.password_input.is_empty() {
                    resp.request_focus();
                }
                if resp.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                    try_unlock(state, events);
                }
                ui.add_space(8.0);
                if ui.button("Unlock").clicked() {
                    try_unlock(state, events);
                }
            }

            if let Some(err) = state.unlock_error.clone() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::from_rgb(255, 90, 90), err);
            }

            ui.add_space(24.0);
            ui.label(
                egui::RichText::new(
                    "Credentials never leave this device unencrypted.\nThere is no password recovery — don't forget it.",
                )
                .small()
                .weak(),
            );
        });
    });
}

fn try_create(state: &mut UiState, events: &mut Vec<UiEvent>) {
    if state.password_input.len() < 8 {
        state.unlock_error = Some("Use at least 8 characters.".to_string());
        return;
    }
    if state.password_input != state.password_confirm {
        state.unlock_error = Some("Passwords do not match.".to_string());
        return;
    }
    state.unlock_error = None;
    events.push(UiEvent::CreateVault(std::mem::take(
        &mut state.password_input,
    )));
    state.password_confirm.clear();
}

fn try_unlock(state: &mut UiState, events: &mut Vec<UiEvent>) {
    if state.password_input.is_empty() {
        return;
    }
    state.unlock_error = None;
    events.push(UiEvent::UnlockVault(std::mem::take(
        &mut state.password_input,
    )));
}
