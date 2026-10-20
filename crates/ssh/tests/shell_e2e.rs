//! End-to-end shell test against a real OpenSSH server.
//!
//! Proves that `SessionManager::open_shell` survives a real connection:
//! OpenSSH sends channel-request replies (`Success`), and the fixed read
//! task must ignore them instead of treating them as "closed".
//!
//! Requires a local sshd on 127.0.0.1:2222 with password auth for a test
//! user (see task setup). If the port is not listening, the test skips.
//!
//! Author: Passwd Word

use std::net::TcpStream;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use opsdeck_ssh::{
    CredentialProvider, HostKeyDecision, HostKeyPrompt, SessionManager, ShellChannel, ShellCommand,
    ShellEvent, SshError,
};
use opsdeck_storage::models::{AuthMethod, HostConfig};
use opsdeck_vault::{SecretBytes, SecretString};
use tokio::sync::mpsc;
use uuid::Uuid;

struct TestCreds;

#[async_trait]
impl CredentialProvider for TestCreds {
    async fn password_for(&self, _config: &HostConfig) -> Result<SecretString, SshError> {
        Ok(SecretString::new("testpass123"))
    }

    async fn private_key_for(
        &self,
        _config: &HostConfig,
    ) -> Result<(SecretBytes, Option<SecretString>), SshError> {
        Err(SshError::Connection("no key in e2e test".into()))
    }
}

fn port_listening() -> bool {
    TcpStream::connect_timeout(
        &"127.0.0.1:2222".parse().expect("valid addr"),
        Duration::from_secs(2),
    )
    .is_ok()
}

/// Drain shell events for `dur`, failing on Closed/Error, collecting Data.
async fn drain_stable(shell: &mut ShellChannel, dur: Duration, out: &mut Vec<u8>) {
    let deadline = Instant::now() + dur;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, shell.events.recv()).await {
            Ok(Some(ShellEvent::Data(bytes))) => out.extend_from_slice(&bytes),
            Ok(Some(ShellEvent::Closed)) => {
                panic!("shell closed unexpectedly during stable window")
            }
            Ok(Some(ShellEvent::Error(e))) => {
                panic!("shell error during stable window: {e}")
            }
            Ok(None) => panic!("shell event stream ended during stable window"),
            Err(_) => break, // timeout: window elapsed
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn shell_survives_real_openssh_session() {
    if !port_listening() {
        eprintln!("SKIP: no sshd on 127.0.0.1:2222");
        return;
    }

    let (prompt_tx, mut prompt_rx) = mpsc::unbounded_channel::<HostKeyPrompt>();
    let kh_path = std::env::temp_dir().join(format!(
        "opsdeck-e2e-known-hosts-{}.json",
        std::process::id()
    ));
    let manager = SessionManager::new(prompt_tx, &kh_path);

    // Auto-trust host keys (test fixture).
    tokio::spawn(async move {
        while let Some(prompt) = prompt_rx.recv().await {
            let _ = prompt.responder.send(HostKeyDecision::TrustAlways);
        }
    });

    let config = HostConfig {
        id: Uuid::new_v4(),
        name: "e2e".to_string(),
        host: "127.0.0.1".to_string(),
        port: 2222,
        username: "testssh".to_string(),
        auth: AuthMethod::Password,
        proxy_jump: None,
        startup_command: None,
        connect_timeout_secs: None,
        keepalive_secs: None,
        term_type: None,
        default_path: None,
        proxy_url: None,
        env: Vec::new(),
        keepalive_cmd: None,
        keepalive_cmd_secs: None,
        tags: vec![],
    };
    let creds = TestCreds;

    let session = manager
        .connect(&config, &creds)
        .await
        .expect("connect to local sshd");
    let mut shell = manager
        .open_shell(session, 80, 24)
        .await
        .expect("open shell");

    // The old bug killed the shell within ~1s (Success reply / window
    // adjustments were treated as Closed). Stay alive for 3s.
    let mut out = Vec::new();
    drain_stable(&mut shell, Duration::from_secs(3), &mut out).await;
    println!(
        "shell alive after 3s, {} initial bytes: {:?}",
        out.len(),
        String::from_utf8_lossy(&out)
            .chars()
            .take(80)
            .collect::<String>()
    );

    // Round trip: send a command, expect its echo/output back.
    shell
        .commands
        .send(ShellCommand::Write(b"echo hello-from-opsdeck\n".to_vec()))
        .expect("write command");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw = false;
    while Instant::now() < deadline && !saw {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, shell.events.recv()).await {
            Ok(Some(ShellEvent::Data(bytes))) => {
                out.extend_from_slice(&bytes);
                if String::from_utf8_lossy(&out).contains("hello-from-opsdeck") {
                    saw = true;
                }
            }
            Ok(Some(ShellEvent::Closed)) => panic!("shell closed before echo arrived"),
            Ok(Some(ShellEvent::Error(e))) => panic!("shell error before echo arrived: {e}"),
            Ok(None) => panic!("event stream ended before echo arrived"),
            Err(_) => break,
        }
    }
    assert!(
        saw,
        "expected 'hello-from-opsdeck' in shell output, got: {:?}",
        String::from_utf8_lossy(&out)
    );
    println!("echo round-trip OK ({} total bytes)", out.len());

    // A real close must still propagate.
    shell
        .commands
        .send(ShellCommand::Close)
        .expect("close command");
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match shell.events.recv().await {
                Some(ShellEvent::Closed) | None => break true,
                Some(_) => continue,
            }
        }
    })
    .await;
    assert!(closed.unwrap_or(false), "no Closed event after Close");
    println!("close propagation OK");

    let _ = std::fs::remove_file(&kh_path);
}

/// Run a minimal no-auth SOCKS5 relay in the background, returning its port.
async fn start_socks5_relay() -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind socks5");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut greet = [0u8; 2];
                if s.read_exact(&mut greet).await.is_err() {
                    return;
                }
                let mut methods = vec![0u8; greet[1] as usize];
                if s.read_exact(&mut methods).await.is_err() {
                    return;
                }
                if s.write_all(&[0x05, 0x00]).await.is_err() {
                    return;
                }
                let mut hdr = [0u8; 4];
                if s.read_exact(&mut hdr).await.is_err() {
                    return;
                }
                let host = match hdr[3] {
                    0x01 => {
                        let mut ip = [0u8; 4];
                        if s.read_exact(&mut ip).await.is_err() {
                            return;
                        }
                        std::net::IpAddr::from(ip).to_string()
                    }
                    0x03 => {
                        let mut len = [0u8; 1];
                        if s.read_exact(&mut len).await.is_err() {
                            return;
                        }
                        let mut name = vec![0u8; len[0] as usize];
                        if s.read_exact(&mut name).await.is_err() {
                            return;
                        }
                        String::from_utf8_lossy(&name).into_owned()
                    }
                    _ => return,
                };
                let mut portb = [0u8; 2];
                if s.read_exact(&mut portb).await.is_err() {
                    return;
                }
                let port = u16::from_be_bytes(portb);
                match tokio::net::TcpStream::connect(format!("{host}:{port}")).await {
                    Ok(mut up) => {
                        let _ = s
                            .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await;
                        let _ = tokio::io::copy_bidirectional(&mut s, &mut up).await;
                    }
                    Err(_) => {
                        let _ = s
                            .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await;
                    }
                }
            });
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread")]
async fn shell_via_socks5_proxy() {
    if !port_listening() {
        eprintln!("SKIP: no sshd on 127.0.0.1:2222");
        return;
    }
    let proxy_port = start_socks5_relay().await;

    let (prompt_tx, mut prompt_rx) = mpsc::unbounded_channel::<HostKeyPrompt>();
    let kh_path = std::env::temp_dir().join(format!(
        "opsdeck-e2e-proxy-known-hosts-{}.json",
        std::process::id()
    ));
    let manager = SessionManager::new(prompt_tx, &kh_path);
    tokio::spawn(async move {
        while let Some(prompt) = prompt_rx.recv().await {
            let _ = prompt.responder.send(HostKeyDecision::TrustAlways);
        }
    });

    let config = HostConfig {
        id: Uuid::new_v4(),
        name: "e2e-proxy".to_string(),
        host: "127.0.0.1".to_string(),
        port: 2222,
        username: "testssh".to_string(),
        auth: AuthMethod::Password,
        proxy_jump: None,
        startup_command: None,
        connect_timeout_secs: Some(10),
        keepalive_secs: Some(15),
        term_type: None,
        default_path: None,
        proxy_url: Some(format!("socks5://127.0.0.1:{proxy_port}")),
        env: Vec::new(),
        keepalive_cmd: None,
        keepalive_cmd_secs: None,
        tags: vec![],
    };
    let creds = TestCreds;

    let session = manager
        .connect(&config, &creds)
        .await
        .expect("connect through socks5");
    let mut shell = manager
        .open_shell(session, 80, 24)
        .await
        .expect("open shell through socks5");

    shell
        .commands
        .send(ShellCommand::Write(b"echo proxy-works-ok\n".to_vec()))
        .expect("write command");
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut out = Vec::new();
    let mut saw = false;
    while Instant::now() < deadline && !saw {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, shell.events.recv()).await {
            Ok(Some(ShellEvent::Data(bytes))) => {
                out.extend_from_slice(&bytes);
                if String::from_utf8_lossy(&out).contains("proxy-works-ok") {
                    saw = true;
                }
            }
            Ok(Some(ShellEvent::Closed)) => panic!("shell closed before proxy echo"),
            Ok(Some(ShellEvent::Error(e))) => panic!("shell error before proxy echo: {e}"),
            Ok(None) => panic!("event stream ended before proxy echo"),
            Err(_) => break,
        }
    }
    assert!(
        saw,
        "expected 'proxy-works-ok' via socks5, got: {:?}",
        String::from_utf8_lossy(&out)
    );
    println!("socks5 proxy echo OK");

    shell
        .commands
        .send(ShellCommand::Close)
        .expect("close command");
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match shell.events.recv().await {
                Some(ShellEvent::Closed) | None => break true,
                Some(_) => continue,
            }
        }
    })
    .await;
    assert!(closed.unwrap_or(false), "no Closed event after Close");
    println!("close propagation OK (proxy)");

    let _ = std::fs::remove_file(&kh_path);
}
