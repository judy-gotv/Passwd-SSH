//! End-to-end: our client against a fake RFC-854 telnet server.
//! Author: Passwd Word

use std::time::{Duration, Instant};

use opsdeck_ssh::{ShellChannel, ShellCommand, ShellEvent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const IAC: u8 = 255;
const DO: u8 = 253;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;

/// Fake server: negotiates TTYPE/NAWS/ECHO, then echoes every line back.
async fn fake_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                s.write_all(&[IAC, DO, 24]).await.ok();
                s.write_all(&[IAC, DO, 31]).await.ok();
                s.write_all(&[IAC, WILL, 1]).await.ok();
                let mut buf = [0u8; 4096];
                let mut line = Vec::new();
                loop {
                    let n = match s.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let mut i = 0;
                    while i < n {
                        if buf[i] == IAC {
                            if i + 2 < n && buf[i + 1] == SB {
                                let mut j = i + 2;
                                while j < n && buf[j] != SE {
                                    j += 1;
                                }
                                i = j + 1;
                            } else {
                                i += 3.min(n - i);
                            }
                            continue;
                        }
                        let b = buf[i];
                        line.push(b);
                        if b == b'\n' {
                            let mut out = b"server-echo: ".to_vec();
                            out.extend_from_slice(&line);
                            s.write_all(&out).await.ok();
                            line.clear();
                        }
                        i += 1;
                    }
                }
            });
        }
    });
    port
}

/// Read events until `needle` appears in output (or panic on timeout/close).
async fn drain_until(shell: &mut ShellChannel, needle: &str, secs: u64) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut out = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            panic!(
                "timeout waiting for {needle:?}; got {:?}",
                String::from_utf8_lossy(&out)
            );
        }
        match tokio::time::timeout(left, shell.events.recv()).await {
            Err(_) => panic!(
                "timeout waiting for {needle:?}; got {:?}",
                String::from_utf8_lossy(&out)
            ),
            Ok(None) => panic!("channel closed before {needle:?} arrived"),
            Ok(Some(ShellEvent::Closed)) => {
                panic!("session closed before {needle:?} arrived")
            }
            Ok(Some(ShellEvent::Error(e))) => panic!("shell error: {e}"),
            Ok(Some(ShellEvent::Data(bytes))) => {
                out.extend_from_slice(&bytes);
                if String::from_utf8_lossy(&out).contains(needle) {
                    println!("got {:?} ({})", needle, String::from_utf8_lossy(&out).len());
                    return;
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn telnet_roundtrip_with_negotiation() {
    let port = fake_server().await;
    let mut shell = opsdeck_telnet::open(
        "127.0.0.1",
        port,
        80,
        24,
        Some("xterm-256color"),
        Duration::from_secs(10),
    )
    .await
    .expect("telnet open");

    shell
        .commands
        .send(ShellCommand::Write(b"hello\n".to_vec()))
        .expect("write");
    drain_until(&mut shell, "server-echo: hello", 8).await;

    shell
        .commands
        .send(ShellCommand::Resize(100, 40))
        .expect("resize");
    shell.commands.send(ShellCommand::Close).expect("close");
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match shell.events.recv().await {
                Some(ShellEvent::Closed) | None => break true,
                Some(_) => continue,
            }
        }
    })
    .await;
    assert!(closed.unwrap_or(false), "no Closed after Close");
    println!("close OK");
}
