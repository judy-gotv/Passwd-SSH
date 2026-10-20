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
