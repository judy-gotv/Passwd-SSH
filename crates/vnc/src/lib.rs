//! Passwd Word
//!
//! RFB client: blocking handshake in a worker thread, frame updates flow
//! into the shared [`FrameState`] owned by [`opsdeck_desktop`], and input
//! arrives through [`DesktopCommand`]s.

use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opsdeck_desktop::{DesktopCommand, DesktopHandle, FrameState};
use thiserror::Error;
use vnc::client::{AuthChoice, AuthMethod, Event};
use vnc::{Client, Encoding, PixelFormat, Rect};

#[derive(Debug, Error)]
pub enum VncError {
    #[error("could not resolve or reach {0}:{1}")]
    Connect(String, u16),
    #[error("authentication required but no password is set on the host")]
    AuthRequired,
    #[error("server requires an unsupported auth method")]
    AuthUnsupported,
    #[error("vnc handshake failed: {0}")]
    Handshake(String),
    #[error("session closed: {0}")]
    Closed(String),
}

/// 32-bit BGRX pixel format: bytes on the wire are [B, G, R, pad].
fn format32() -> PixelFormat {
    PixelFormat {
        bits_per_pixel: 32,
        depth: 24,
        big_endian: false,
        true_colour: true,
        red_max: 255,
        green_max: 255,
        blue_max: 255,
        red_shift: 16,
        green_shift: 8,
        blue_shift: 0,
    }
}

/// Convert BGRX bytes to RGBA in place.
fn bgrx_to_rgba(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len());
    for px in pixels.as_chunks::<4>().0 {
        out.push(px[2]); // R
        out.push(px[1]); // G
        out.push(px[0]); // B
        out.push(255); // A
    }
    out
}

/// Connect to a VNC server and start the frame pump. Returns the handle the
/// app keeps; the worker thread lives until [`DesktopCommand::Close`] or a
/// disconnect event.
pub fn connect(
    host: &str,
    port: u16,
    password: Option<&str>,
    timeout: Duration,
) -> Result<DesktopHandle, VncError> {
    let addr: SocketAddr = format!("{host}:{port}")
        .to_socket_addrs()
        .map_err(|_| VncError::Connect(host.to_string(), port))?
        .next()
        .ok_or_else(|| VncError::Connect(host.to_string(), port))?;
    let stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|_| VncError::Connect(host.to_string(), port))?;
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<DesktopCommand>();
    let frame = Arc::new(Mutex::new(FrameState::default()));

    let client = Client::from_tcp_stream(stream, true, |methods: &[AuthMethod]| {
        if methods.iter().any(|m| matches!(m, AuthMethod::None)) {
            Some(AuthChoice::None)
        } else if methods.iter().any(|m| matches!(m, AuthMethod::Password)) {
            password.map(|pw| {
                let mut key = [0u8; 8];
                let bytes = pw.as_bytes();
                let n = bytes.len().min(8);
                key[..n].copy_from_slice(&bytes[..n]);
                AuthChoice::Password(key)
            })
        } else {
            None
        }
    })
    .map_err(|e| match e {
        vnc::Error::AuthenticationUnavailable => {
            if password.is_none() {
                VncError::AuthRequired
            } else {
                VncError::AuthUnsupported
            }
        }
        other => VncError::Handshake(other.to_string()),
    })?;

    let frame_w = frame.clone();
    let worker = std::thread::spawn(move || {
        run(client, frame_w, cmd_rx);
    });

    Ok(DesktopHandle {
        commands: cmd_tx,
        frame,
        worker: Some(worker),
    })
}

fn request_full(client: &mut Client) -> io::Result<()> {
    let (w, h) = client.size();
    client
        .request_update(
            Rect {
                left: 0,
                top: 0,
                width: w,
                height: h,
            },
            false,
        )
        .map_err(|e| io::Error::other(e.to_string()))
}

/// The frame pump: forwards UI commands, drains events into the shared
/// frame, and periodically asks for incremental updates.
fn run(
    mut client: Client,
    frame: Arc<Mutex<FrameState>>,
    cmds: std::sync::mpsc::Receiver<DesktopCommand>,
) {
    if client.set_format(format32()).is_err() {
        return;
    }

    if client
        .set_encodings(&[Encoding::Zrle, Encoding::CopyRect, Encoding::Raw])
        .is_err()
    {
        return;
    }
    let (w, h) = client.size();
    {
        let mut f = frame.lock().unwrap();
        f.resize(w, h);
    }
    if request_full(&mut client).is_err() {
        return;
    }

    let mut tick = 0u64;
    loop {
        // Input commands.
        while let Ok(cmd) = cmds.try_recv() {
            let ok = match cmd {
                DesktopCommand::Pointer { buttons, x, y } => {
                    client.send_pointer_event(buttons, x, y).is_ok()
                }
                DesktopCommand::Key { keycode, down } => {
                    client.send_key_event(down, keycode).is_ok()
                }
                DesktopCommand::Close => {
                    let _ = client.disconnect();
                    return;
                }
            };
            if !ok {
                tracing::warn!("vnc: failed to send input, closing");
                return;
            }
        }
        // Events.
        let mut saw_frame = false;
        while let Some(ev) = client.poll_event() {
            match ev {
                Event::Disconnected(err) => {
                    tracing::info!("vnc: disconnected ({err:?})");
                    return;
                }
                Event::Resize(w, h) => {
                    frame.lock().unwrap().resize(w, h);
                    if request_full(&mut client).is_err() {
                        return;
                    }
                }
                Event::PutPixels(rect, pixels) => {
                    let rgba = bgrx_to_rgba(&pixels);
                    frame
                        .lock()
                        .unwrap()
                        .put(rect.left, rect.top, rect.width, rect.height, &rgba);
                    saw_frame = true;
                }
                Event::CopyPixels { src, dst } => {
                    frame
                        .lock()
                        .unwrap()
                        .copy_rect(src.left, src.top, dst.left, dst.top, dst.width, dst.height);
                    saw_frame = true;
                }
                Event::EndOfFrame => {
                    saw_frame = true;
                }
                _ => {}
            }
        }
        tick += 1;
        if !saw_frame && tick.is_multiple_of(5) {
            // Ask for incremental updates twice a second.
            let (w, h) = client.size();
            if client
                .request_update(
                    Rect {
                        left: 0,
                        top: 0,
                        width: w,
                        height: h,
                    },
                    true,
                )
                .is_err()
            {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgrx_to_rgba_reorders() {
        // B=10 G=20 R=30 pad
        let out = bgrx_to_rgba(&[10, 20, 30, 0]);
        assert_eq!(out, vec![30, 20, 10, 255]);
    }

    #[test]
    fn format32_is_bgrx() {
        let f = format32();
        assert_eq!(f.bits_per_pixel, 32);
        assert_eq!(f.red_shift, 16);
        assert_eq!(f.green_shift, 8);
        assert_eq!(f.blue_shift, 0);
    }
}
