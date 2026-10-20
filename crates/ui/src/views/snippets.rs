//! Snippets view: command library + batch execution.
//! Author: Passwd Word

use crate::{SnippetForm, UiData, UiEvent, UiState};

/// Draw the Snippets tab.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(state.t("Snippets"));
            let search_hint = state.t("Search snippets");
            ui.add(
                egui::TextEdit::singleline(&mut state.snippet_search)
                    .hint_text(search_hint)
                    .desired_width(220.0),
            );
            if ui.small_button(state.t("＋ New")).clicked() {
                state.snippet_form = SnippetForm::default();
                state.show_snippet_editor = true;
            }
        });
        ui.separator();

        let query = state.snippet_search.to_lowercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for snippet in data.snippets {
                if !query.is_empty()
                    && !snippet.title.to_lowercase().contains(&query)
                    && !snippet.command.to_lowercase().contains(&query)
                {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&snippet.title).strong());
                        ui.label(egui::RichText::new(&snippet.command).monospace().small());
                        if let Some(desc) = &snippet.description
                            && !desc.is_empty()
                        {
                            ui.label(egui::RichText::new(desc).small().weak());
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("🗑")
                            .on_hover_text(state.t("Delete"))
                            .clicked()
                        {
                            state.confirm = Some(crate::ConfirmDialog {
                                title: state.t("Delete snippet").to_string(),
                                message: state
                                    .tf("Delete snippet “{title}”?", &[("title", &snippet.title)]),
                                confirm_label: state.t("Delete").to_string(),
                                action: format!("delete_snippet:{}", snippet.id),
                            });
                        }
                        if ui
                            .small_button("✎")
                            .on_hover_text(state.t("Edit"))
                            .clicked()
                        {
                            state.snippet_form = SnippetForm {
                                id: Some(snippet.id.clone()),
                                title: snippet.title.clone(),
                                command: snippet.command.clone(),
                                description: snippet.description.clone().unwrap_or_default(),
                                tags: snippet.tag_list().join(", "),
                            };
                            state.show_snippet_editor = true;
                        }
                        if ui
                            .small_button("▶")
                            .on_hover_text(state.t("Run on active terminal"))
                            .clicked()
                        {
                            if let Some(pane) = state.active_pane().cloned() {
                                events.push(UiEvent::RunSnippet {
                                    snippet_id: snippet.id.clone(),
                                    session: pane.session,
                                });
                            } else {
                                state.toast(state.t("No active terminal"), true);
                            }
                        }
                    });
                });
                ui.separator();
            }
            if data.snippets.is_empty() {
                ui.label(
                    egui::RichText::new(
                        state.t("No snippets yet. Built-in examples are seeded on first run."),
                    )
                    .weak(),
                );
            }
        });

        // Batch run
        ui.separator();
        ui.label(egui::RichText::new(state.t("Batch run")).strong());
        ui.horizontal(|ui| {
            ui.label(state.t("Command:"));
            ui.add(
                egui::TextEdit::singleline(&mut state.batch_command).desired_width(f32::INFINITY),
            );
        });
        ui.horizontal(|ui| {
            ui.label(state.t("Targets:"));
            for tab in &state.tabs {
                for pane in &tab.panes {
                    let mut selected = state.batch_selected.contains(&pane.session);
                    if ui.checkbox(&mut selected, &pane.title).changed() {
                        if selected {
                            state.batch_selected.push(pane.session);
                        } else {
                            state.batch_selected.retain(|s| *s != pane.session);
                        }
                    }
                }
            }
            if state.tabs.is_empty() {
                ui.label(egui::RichText::new(state.t("no open sessions")).weak());
            }
        });
        ui.horizontal(|ui| {
            let dangerous_label = state.t("Allow dangerous commands");
            ui.checkbox(&mut state.batch_allow_dangerous, dangerous_label);
            if ui.button(state.t("▶ Run on selected")).clicked() {
                if state.batch_command.trim().is_empty() {
                    state.toast(state.t("Enter a command first"), true);
                } else if state.batch_selected.is_empty() {
                    state.toast(state.t("Select at least one session"), true);
                } else {
                    events.push(UiEvent::BatchRun {
                        sessions: state.batch_selected.clone(),
                        command: state.batch_command.clone(),
                        allow_dangerous: state.batch_allow_dangerous,
                    });
                }
            }
            if ui.small_button(state.t("Clear output")).clicked() {
                state.batch_output.clear();
            }
        });
        if !state.batch_output.is_empty() {
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for (title, output) in &state.batch_output {
                        ui.label(egui::RichText::new(format!("── {title}")).strong().small());
                        ui.label(egui::RichText::new(output).monospace().small());
                    }
                });
        }
    });
}
