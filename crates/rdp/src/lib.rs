//! Passwd Word
//!
//! RDP client on top of `ironrdp_client`: connection runs in a worker
//! thread with its own Tokio runtime; decoded frames land in the shared
//! [`FrameState`], input arrives through [`DesktopCommand`]s and is
//! translated via `ironrdp_input::Database` (unicode keys + scancodes for
//! special keys, mouse button diffing).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ironrdp::client::config::{ConfigBuilder, Destination};
use ironrdp::client::rdp::{RdpClient, RdpInputEvent, RdpOutputEvent};
use ironrdp::input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use opsdeck_desktop::{DesktopCommand, DesktopHandle, FrameState};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RdpError {
    #[error("bad RDP configuration: {0}")]
    Config(String),
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("connection timed out")]
    Timeout,
    #[error("session terminated")]
    Terminated,
}

/// Image buffers arrive as u32s holding [0, R, G, B] (big-endian packed).
fn u32buf_to_rgba(buffer: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buffer.len() * 4);
    for &px in buffer {
        out.push(((px >> 16) & 0xff) as u8); // R
        out.push(((px >> 8) & 0xff) as u8); // G
        out.push((px & 0xff) as u8); // B
        out.push(255);
    }
    out
}

/// X11 keysym -> Windows scancode for the non-printable keys we forward.
fn keysym_scancode(ks: u32) -> Option<Scancode> {
    let (ext, code) = match ks {
        0xff0d => (false, 0x1c), // Return
        0xff08 => (false, 0x0e), // BackSpace
        0xff09 => (false, 0x0f), // Tab
        0xff1b => (false, 0x01), // Escape
        0xffff => (true, 0x53),  // Delete
        0xff63 => (true, 0x52),  // Insert
        0xff50 => (true, 0x47),  // Home
        0xff57 => (true, 0x4f),  // End
        0xff55 => (true, 0x49),  // PageUp
        0xff56 => (true, 0x51),  // PageDown
        0xff51 => (true, 0x4b),  // Left
        0xff52 => (true, 0x48),  // Up
        0xff53 => (true, 0x4d),  // Right
        0xff54 => (true, 0x50),  // Down
        0xffe1 => (false, 0x2a), // Shift_L
        0xffe3 => (false, 0x1d), // Control_L
        0xffe9 => (false, 0x38), // Alt_L
        0xffbe..=0xffc9 => {
            // F1..F12
            let idx = (ks - 0xffbe) as u8;
            if idx < 10 {
                (false, 0x3b + idx)
            } else {
                (false, 0x57 + (idx - 10))
            }
        }
        _ => return None,
    };
    Some(Scancode::from_u8(ext, code))
}

/// Connect to an RDP server. Blocks until the first frame arrives (or an
/// error/timeout), mirroring the VNC connect contract.
pub fn connect(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    timeout: Duration,
) -> Result<DesktopHandle, RdpError> {
    let config = ConfigBuilder::new()
        .with_destination(Destination::from_parts(host.to_string(), port))
        .with_username(username.to_string())
        .with_password(password.to_string())
        .with_client_build(26100)
        .with_client_name("OpsDeck")
        .with_client_dir("C:\\Windows\\System32")
        .with_desktop_width(1280)
        .with_desktop_height(800)
        .build()
        .map_err(|e| RdpError::Config(e.to_string()))?;

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<DesktopCommand>();
    let frame: Arc<Mutex<FrameState>> = Arc::new(Mutex::new(FrameState::default()));
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

    let frame_w = frame.clone();
    let worker = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
        };
        rt.block_on(run(config, frame_w, cmd_rx, ready_tx));
    });

    match ready_rx.recv_timeout(timeout) {
        Ok(Ok(())) => Ok(DesktopHandle {
            commands: cmd_tx,
            frame,
            worker: Some(worker),
        }),
        Ok(Err(e)) => Err(RdpError::Connect(e)),
        Err(_) => {
            let _ = cmd_tx.send(DesktopCommand::Close);
            Err(RdpError::Timeout)
        }
    }
}

async fn run(
    config: ironrdp::client::config::Config,
    frame: Arc<Mutex<FrameState>>,
    cmds: std::sync::mpsc::Receiver<DesktopCommand>,
    ready_tx: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<RdpOutputEvent>(16);
    let client = RdpClient::new(config, out_tx);
    let input_tx = client.input_sender();
    // RdpClient's run() future is !Send (it holds GUI-thread-bound clipboard
    // state); a current-thread runtime + LocalSet keeps it on this thread.
    let local = tokio::task::LocalSet::new();
    let client_task = local.spawn_local(client.run());

    let mut ready_tx = Some(ready_tx);
    let mut db = Database::default();
    let mut prev_buttons: u8 = 0;

    local
        .run_until(async {
            loop {
                tokio::select! {
                    out = out_rx.recv() => {
                        match out {
                            Some(RdpOutputEvent::Image { buffer, width, height }) => {
                                let (w, h) = (width.get(), height.get());
                                let rgba = u32buf_to_rgba(&buffer);
                                {
                                    let mut f = frame.lock().unwrap();
                                    if (f.width, f.height) != (w, h) {
                                        f.resize(w, h);
                                    }
                                    f.put(0, 0, w, h, &rgba);
                                }
                                if let Some(tx) = ready_tx.take() {
                                    let _ = tx.send(Ok(()));
                                }
                            }
                            Some(RdpOutputEvent::ConnectionFailure(e)) => {
                                if let Some(tx) = ready_tx.take() {
                                    let _ = tx.send(Err(e.to_string()));
                                }
                                tracing::warn!("rdp: connection failed: {e}");
                                break;
                            }
                            Some(RdpOutputEvent::Terminated(_)) | None => {
                                if let Some(tx) = ready_tx.take() {
                                    let _ = tx.send(Err("terminated".to_string()));
                                }
                                break;
                            }
                            Some(_) => {}
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(40)) => {
                        // Drain UI commands.
                        while let Ok(cmd) = cmds.try_recv() {
                            if !handle_command(cmd, &mut db, &mut prev_buttons, &input_tx) {
                                client_task.abort();
                                return;
                            }
                        }
                    }
                }
            }
        })
        .await;
    client_task.abort();
}

/// Returns false when the session should end (Close).
fn handle_command(
    cmd: DesktopCommand,
    db: &mut Database,
    prev_buttons: &mut u8,
    input_tx: &tokio::sync::mpsc::UnboundedSender<RdpInputEvent>,
) -> bool {
    match cmd {
        DesktopCommand::Close => {
            let _ = input_tx.send(RdpInputEvent::Close);
            false
        }
        DesktopCommand::Pointer { buttons, x, y } => {
            let mut ops: Vec<Operation> = vec![Operation::MouseMove(MousePosition { x, y })];
            for (bit, btn) in [
                (1u8, MouseButton::Left),
                (2u8, MouseButton::Middle),
                (4u8, MouseButton::Right),
            ] {
                let was = *prev_buttons & bit != 0;
                let now = buttons & bit != 0;
                if now && !was {
                    ops.push(Operation::MouseButtonPressed(btn));
                } else if !now && was {
                    ops.push(Operation::MouseButtonReleased(btn));
                }
            }
            if buttons & 8 != 0 {
                ops.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: true,
                    rotation_units: 120,
                }));
            }
            if buttons & 16 != 0 {
                ops.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: true,
                    rotation_units: -120,
                }));
            }
            *prev_buttons = buttons & 0x07;
            let events = db.apply(ops);
            if !events.is_empty() {
                let _ = input_tx.send(RdpInputEvent::FastPath(events));
            }
            true
        }
        DesktopCommand::Key { keycode, down } => {
            let op = match keysym_scancode(keycode) {
                Some(sc) => {
                    if down {
                        Operation::KeyPressed(sc)
                    } else {
                        Operation::KeyReleased(sc)
                    }
                }
                None => {
                    let ch = char::from_u32(keycode);
                    match ch {
                        Some(c) if down => Operation::UnicodeKeyPressed(c),
                        Some(c) => Operation::UnicodeKeyReleased(c),
                        None => return true,
                    }
                }
            };
            let events = db.apply([op]);
            if !events.is_empty() {
                let _ = input_tx.send(RdpInputEvent::FastPath(events));
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u32buf_converts() {
        // [0, R=0x11, G=0x22, B=0x33]
        let out = u32buf_to_rgba(&[0x00112233]);
        assert_eq!(out, vec![0x11, 0x22, 0x33, 255]);
    }

    #[test]
    fn keysym_mapping() {
        let enter = keysym_scancode(0xff0d).unwrap();
        assert_eq!(enter.as_u16(), 0x1c);
        let up = keysym_scancode(0xff52).unwrap();
        assert_eq!(up.as_u16(), 0xe048);
        assert!(keysym_scancode(b'a' as u32).is_none());
        assert!(keysym_scancode(0xffbe).is_some()); // F1
    }
}
