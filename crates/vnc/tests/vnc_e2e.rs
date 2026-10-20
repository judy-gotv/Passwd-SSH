//! End-to-end: our VNC client against a fake RFB 3.8 server.
//! Author: Passwd Word

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opsdeck_desktop::DesktopCommand;

fn w16(s: &mut TcpStream, v: u16) {
    s.write_all(&v.to_be_bytes()).unwrap();
}

/// Minimal RFB 3.8 server: no auth, 4x4 screen, answers every
/// FramebufferUpdateRequest with one red Raw rect. Records input events.
fn fake_server(log: Arc<Mutex<Vec<String>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let log = log.clone();
            std::thread::spawn(move || {
                let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
                // Version handshake: the server speaks first.
                s.write_all(b"RFB 003.008\n").unwrap();
                let mut ver = [0u8; 12];
                if s.read_exact(&mut ver).is_err() {
                    return;
                }
                // Security: 1 type = None.
                s.write_all(&[1u8, 1u8]).unwrap();
                // Client's security type selection, then SecurityResult=OK.
                let mut one = [0u8; 1];
                s.read_exact(&mut one).ok();
                s.write_all(&0u32.to_be_bytes()).unwrap();
                // ClientInit shared flag.
                s.read_exact(&mut one).ok();
                // ServerInit: 4x4, 32bpp BGRX, name "test".
                w16(&mut s, 4);
                w16(&mut s, 4);
                s.write_all(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0])
                    .unwrap();
                s.write_all(&4u32.to_be_bytes()).unwrap();
                s.write_all(b"test").unwrap();
                let red: Vec<u8> = [0, 0, 255, 0].repeat(16);
                let mut typ = [0u8; 1];
                loop {
                    if s.read_exact(&mut typ).is_err() {
                        break;
                    }
                    match typ[0] {
                        0 => {
                            // SetPixelFormat
                            let mut rest = [0u8; 19];
                            s.read_exact(&mut rest).ok();
                        }
                        2 => {
                            // SetEncodings
                            let mut n = [0u8; 3];
                            s.read_exact(&mut n).ok();
                            let count = u16::from_be_bytes([n[1], n[2]]) as usize;
                            let mut encs = vec![0u8; count * 4];
                            s.read_exact(&mut encs).ok();
                        }
                        3 => {
                            // FramebufferUpdateRequest
                            let mut rest = [0u8; 9];
                            s.read_exact(&mut rest).ok();
                            // Answer: one Raw 4x4 red rect.
                            s.write_all(&[0u8, 0u8]).ok();
                            s.write_all(&1u16.to_be_bytes()).ok();
                            w16(&mut s, 0);
                            w16(&mut s, 0);
                            w16(&mut s, 4);
                            w16(&mut s, 4);
                            s.write_all(&0i32.to_be_bytes()).ok();
                            s.write_all(&red).ok();
                        }
                        4 => {
                            // KeyEvent
                            let mut rest = [0u8; 7];
                            s.read_exact(&mut rest).ok();
                            let down = rest[0] != 0;
                            let key = u32::from_be_bytes([rest[3], rest[4], rest[5], rest[6]]);
                            log.lock().unwrap().push(format!("key {key:#x} {down}"));
                        }
                        5 => {
                            // PointerEvent
                            let mut rest = [0u8; 5];
                            s.read_exact(&mut rest).ok();
                            let buttons = rest[0];
                            let x = u16::from_be_bytes([rest[1], rest[2]]);
                            let y = u16::from_be_bytes([rest[3], rest[4]]);
                            log.lock()
                                .unwrap()
                                .push(format!("pointer {buttons} {x} {y}"));
                        }
                        6 => {
                            // ClientCutText
                            let mut n = [0u8; 7];
                            s.read_exact(&mut n).ok();
                            let len = u32::from_be_bytes([n[3], n[4], n[5], n[6]]) as usize;
                            let mut buf = vec![0u8; len];
                            s.read_exact(&mut buf).ok();
                        }
                        _ => break,
                    }
                }
            });
        }
    });
    port
}

#[test]
fn vnc_roundtrip_against_fake_server() {
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let port = fake_server(log.clone());

    let handle = opsdeck_vnc::connect("127.0.0.1", port, None, Duration::from_secs(10))
        .expect("vnc connect");
    // Wait until the first pixel is actually red (set_format consumes the
    // first full update internally, so version bumps alone are not enough).
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if handle.frame.lock().unwrap().pixels.first() == Some(&255) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no red frame from server; version={}",
            handle.frame.lock().unwrap().version
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    {
        let f = handle.frame.lock().unwrap();
        assert_eq!((f.width, f.height), (4, 4));
        assert_eq!(f.pixels.len(), 4 * 4 * 4);
        // Server sent red (R=255) BGRX; we convert to RGBA.
        assert_eq!(f.pixels[0..3], [255, 0, 0]);
        assert_eq!(f.pixels[3], 255);
    }
    println!("frame OK: 4x4 red rect received");

    handle.send(DesktopCommand::Key {
        keycode: 0xff0d,
        down: true,
    });
    handle.send(DesktopCommand::Pointer {
        buttons: 1,
        x: 2,
        y: 2,
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let l = log.lock().unwrap();
        if l.iter().any(|e| e.starts_with("key")) && l.iter().any(|e| e.starts_with("pointer")) {
            println!("input OK: {l:?}");
            break;
        }
        drop(l);
        assert!(
            Instant::now() < deadline,
            "input events never reached server"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    handle.send(DesktopCommand::Close);
    if let Some(w) = handle.worker {
        let ok = std::thread::spawn(move || {
            let _ = w.join();
        })
        .join()
        .is_ok();
        assert!(ok, "worker thread failed");
    }
    println!("close OK");
}
