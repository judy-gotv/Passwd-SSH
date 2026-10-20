//! Vault dialog: create / unlock / lock, on demand. The app itself never
//! blocks on the vault — only credential access does, and those paths open
//! this dialog when the vault is locked.
//! Author: Passwd Word

use egui::TextEdit;

use crate::{UiData, UiEvent, UiState};

/// Draw the vault dialog when `state.show_vault_dialog` is set.
pub fn dialog(ctx: &egui::Context, state: &mut UiState, _data: &UiData, events: &mut Vec<UiEvent>) {
    if !state.show_vault_dialog {
        return;
    }
    let mut open = true;
    let mut lock_clicked = false;

    egui::Window::new(state.t("Vault"))
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .open(&mut open)
        .show(ctx, |ui| {
            let width = 320.0;
            if !state.vault_exists {
                ui.label(
                    state.t("Create a master password. It encrypts all stored credentials."),
                );
                ui.add_space(8.0);
                let pw_hint = state.t("Master password");
                let pw = TextEdit::singleline(&mut state.password_input)
                    .password(true)
                    .hint_text(pw_hint)
                    .desired_width(width);
                let resp = ui.add(pw);
                let confirm_hint = state.t("Confirm password");
                let confirm = TextEdit::singleline(&mut state.password_confirm)
                    .password(true)
                    .hint_text(confirm_hint)
                    .desired_width(width);
                ui.add(confirm);
                if resp.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                    try_create(state, events);
                }
                ui.add_space(8.0);
                if ui.button(state.t("Create vault")).clicked() {
                    try_create(state, events);
                }
            } else if !state.vault_unlocked {
                ui.label(state.t("Enter your master password to unlock."));
                ui.add_space(8.0);
                let pw_hint = state.t("Master password");
                let pw = TextEdit::singleline(&mut state.password_input)
                    .password(true)
                    .hint_text(pw_hint)
                    .desired_width(width);
                let resp = ui.add(pw);
                if state.unlock_error.is_none() && state.password_input.is_empty() {
                    resp.request_focus();
                }
                if resp.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                    try_unlock(state, events);
                }
                ui.add_space(8.0);
                if ui.button(state.t("Unlock")).clicked() {
                    try_unlock(state, events);
                }
            } else {
                ui.label(state.t("Vault is unlocked."));
                ui.add_space(8.0);
                if ui.button(state.t("🔒 Lock vault")).clicked() {
                    lock_clicked = true;
                }
            }

            if let Some(err) = state.unlock_error.clone() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::from_rgb(255, 90, 90), err);
            }

            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(state.t(
                    "Credentials never leave this device unencrypted.\nThere is no password recovery — don't forget it.",
                ))
                .small()
                .weak(),
            );
        });

    if lock_clicked {
        events.push(UiEvent::LockVault);
    }
    if !open {
        state.show_vault_dialog = false;
        state.password_input.clear();
        state.password_confirm.clear();
        state.unlock_error = None;
    }
}

fn try_create(state: &mut UiState, events: &mut Vec<UiEvent>) {
    if state.password_input.len() < 8 {
        state.unlock_error = Some(state.t("Use at least 8 characters.").to_string());
        return;
    }
    if state.password_input != state.password_confirm {
        state.unlock_error = Some(state.t("Passwords do not match.").to_string());
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
