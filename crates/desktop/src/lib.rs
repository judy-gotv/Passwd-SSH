//! Passwd Word
//!
//! Shared vocabulary for remote-desktop sessions (VNC, RDP): an RGBA frame
//! buffer behind `Arc<Mutex<..>>` that the protocol worker threads update,
//! plus a command channel for pointer/keyboard/close input coming from the UI.

use std::sync::{Arc, Mutex};

/// RGBA frame buffer updated in place by the protocol worker thread.
/// `version` bumps on every update so the UI knows when to re-upload.
#[derive(Debug, Default)]
pub struct FrameState {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
    pub version: u64,
}

impl FrameState {
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.pixels = vec![0; width as usize * height as usize * 4];
        self.version += 1;
    }

    /// Blit an RGBA rectangle at (left, top).
    pub fn put(&mut self, left: u16, top: u16, w: u16, h: u16, rgba: &[u8]) {
        let stride = self.width as usize * 4;
        let w = w as usize;
        for (row, chunk) in rgba.chunks_exact(w * 4).take(h as usize).enumerate() {
            let dst_row = (top as usize + row) * stride + left as usize * 4;
            if dst_row + w * 4 <= self.pixels.len() {
                self.pixels[dst_row..dst_row + w * 4].copy_from_slice(chunk);
            }
        }
        self.version += 1;
    }

    /// In-place rectangle copy (CopyRect encoding). Source rows are
    /// snapshotted first so overlapping copies are correct.
    pub fn copy_rect(&mut self, sx: u16, sy: u16, dx: u16, dy: u16, w: u16, h: u16) {
        let stride = self.width as usize * 4;
        let wlen = w as usize * 4;
        let mut tmp = Vec::with_capacity(wlen * h as usize);
        for row in 0..h as usize {
            let s = (sy as usize + row) * stride + sx as usize * 4;
            if s + wlen > self.pixels.len() {
                return;
            }
            tmp.extend_from_slice(&self.pixels[s..s + wlen]);
        }
        for row in 0..h as usize {
            let d = (dy as usize + row) * stride + dx as usize * 4;
            if d + wlen > self.pixels.len() {
                return;
            }
            self.pixels[d..d + wlen].copy_from_slice(&tmp[row * wlen..(row + 1) * wlen]);
        }
        self.version += 1;
    }
}

/// Commands the UI sends to a running desktop session.
#[derive(Debug, Clone)]
pub enum DesktopCommand {
    /// VNC-style button bitmask: bit0=left, bit1=middle, bit2=right, bit3=up, bit4=down.
    Pointer { buttons: u8, x: u16, y: u16 },
    /// Keycode is protocol-dependent (keysym for VNC).
    Key { keycode: u32, down: bool },
    /// End the session.
    Close,
}

/// Handle the app keeps for a running desktop session.
pub struct DesktopHandle {
    pub commands: std::sync::mpsc::Sender<DesktopCommand>,
    pub frame: Arc<Mutex<FrameState>>,
    pub worker: Option<std::thread::JoinHandle<()>>,
}

impl DesktopHandle {
    pub fn send(&self, cmd: DesktopCommand) {
        let _ = self.commands.send(cmd);
    }
}
