//! Remote desktop viewer: texture upload, scaling, pointer + keyboard
//! forwarding (VNC-style keysyms).
//! Author: Passwd Word

use egui::{ColorImage, Key, Sense, TextureOptions};

use crate::{UiData, UiEvent, UiState};

/// Render the active desktop session into the central area.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut UiState,
    data: &UiData,
    events: &mut Vec<UiEvent>,
    id: u64,
) {
    // The frame buffer updates off-thread; poll at ~10fps while visible.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
    let Some(view) = data.desktops.iter().find(|d| d.id == id) else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(state.t("Desktop session is gone")).size(16.0));
        });
        return;
    };
    let (fw, fh, version, pixels) = {
        let f = view.frame.lock().unwrap();
        (f.width, f.height, f.version, f.pixels.clone())
    };
    if fw == 0 || fh == 0 {
        ui.centered_and_justified(|ui| {
            ui.spinner();
            ui.label(state.t("Connecting…"));
        });
        return;
    }
    // Upload (or re-upload) the framebuffer texture.
    if state.desktop_frame_versions.get(&id) != Some(&version)
        || !state.desktop_textures.contains_key(&id)
    {
        let image = ColorImage::from_rgba_unmultiplied([fw as usize, fh as usize], &pixels);
        match state.desktop_textures.get_mut(&id) {
            Some(tex) => tex.set(image, TextureOptions::LINEAR),
            None => {
                let tex =
                    ui.ctx()
                        .load_texture(format!("desktop-{id}"), image, TextureOptions::LINEAR);
                state.desktop_textures.insert(id, tex);
            }
        }
        state.desktop_frame_versions.insert(id, version);
    }
    let Some(tex) = state.desktop_textures.get(&id) else {
        return;
    };
    let tex_id = tex.id();
    let avail = ui.available_size();
    let scale = (avail.x / fw as f32).min(avail.y / fh as f32).max(0.05);
    let shown = egui::vec2(fw as f32 * scale, fh as f32 * scale);

    ui.vertical_centered(|ui| {
        let resp = ui
            .add(egui::Image::new((tex_id, shown)).sense(Sense::click_and_drag()))
            .on_hover_text(state.t("Click to capture the keyboard"));
        let rect = resp.rect;

        // ---- Pointer ---------------------------------------------------
        let input_snapshot = ui.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.button_down(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Middle),
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.button_released(egui::PointerButton::Primary)
                    || i.pointer.button_released(egui::PointerButton::Middle)
                    || i.pointer.button_released(egui::PointerButton::Secondary),
                i.smooth_scroll_delta.y,
            )
        });
        let (hover, p, m, s, released, scroll_y) = input_snapshot;
        if let Some(pos) = hover
            && rect.contains(pos)
            && (resp.dragged() || resp.hovered() || released)
        {
            let x = ((pos.x - rect.left()) / scale).clamp(0.0, (fw - 1) as f32) as u16;
            let y = ((pos.y - rect.top()) / scale).clamp(0.0, (fh - 1) as f32) as u16;
            let buttons = (p as u8) | ((m as u8) << 1) | ((s as u8) << 2);
            events.push(UiEvent::DesktopPointer { id, buttons, x, y });
        }
        // Wheel: VNC button 4 (up) / 5 (down) pulses.
        if resp.hovered() && scroll_y != 0.0 {
            let mask = if scroll_y > 0.0 { 8u8 } else { 16u8 };
            if let Some(pos) = hover {
                let x = ((pos.x - rect.left()) / scale).clamp(0.0, (fw - 1) as f32) as u16;
                let y = ((pos.y - rect.top()) / scale).clamp(0.0, (fh - 1) as f32) as u16;
                events.push(UiEvent::DesktopPointer {
                    id,
                    buttons: mask,
                    x,
                    y,
                });
                events.push(UiEvent::DesktopPointer {
                    id,
                    buttons: 0,
                    x,
                    y,
                });
            }
        }

        // ---- Keyboard --------------------------------------------------
        if resp.clicked() {
            resp.request_focus();
        }
        if resp.has_focus() {
            let key_events: Vec<egui::Event> = ui.input(|i| i.events.clone());
            for ev in key_events {
                match ev {
                    egui::Event::Text(text) => {
                        for ch in text.chars() {
                            let ks = char_keysym(ch);
                            events.push(UiEvent::DesktopKey {
                                id,
                                keysym: ks,
                                down: true,
                            });
                            events.push(UiEvent::DesktopKey {
                                id,
                                keysym: ks,
                                down: false,
                            });
                        }
                    }
                    egui::Event::Key {
                        key,
                        pressed,
                        repeat,
                        ..
                    } => {
                        if repeat {
                            continue;
                        }
                        if let Some(ks) = special_keysym(key) {
                            events.push(UiEvent::DesktopKey {
                                id,
                                keysym: ks,
                                down: pressed,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    });
}

/// Printable characters map to their unicode codepoint (X11 keysym).
fn char_keysym(ch: char) -> u32 {
    match ch {
        '\n' | '\r' => 0xff0d,
        '\t' => 0xff09,
        c => c as u32,
    }
}

/// egui keys without a text event -> X11 keysyms.
fn special_keysym(key: Key) -> Option<u32> {
    Some(match key {
        Key::Enter => 0xff0d,
        Key::Backspace => 0xff08,
        Key::Tab => 0xff09,
        Key::Escape => 0xff1b,
        Key::Delete => 0xffff,
        Key::Insert => 0xff63,
        Key::Home => 0xff50,
        Key::End => 0xff57,
        Key::PageUp => 0xff55,
        Key::PageDown => 0xff56,
        Key::ArrowLeft => 0xff51,
        Key::ArrowUp => 0xff52,
        Key::ArrowRight => 0xff53,
        Key::ArrowDown => 0xff54,
        Key::F1 => 0xffbe,
        Key::F2 => 0xffbf,
        Key::F3 => 0xffc0,
        Key::F4 => 0xffc1,
        Key::F5 => 0xffc2,
        Key::F6 => 0xffc3,
        Key::F7 => 0xffc4,
        Key::F8 => 0xffc5,
        Key::F9 => 0xffc6,
        Key::F10 => 0xffc7,
        Key::F11 => 0xffc8,
        Key::F12 => 0xffc9,
        _ => return None,
    })
}
