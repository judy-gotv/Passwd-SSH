//! Passwd Word
//!
//! Blocking parts: none. The whole client is one async task driving a TCP
//! stream; bytes flow to/from opsdeck-ssh's ShellChannel so the app and UI
//! treat a Telnet session exactly like an SSH shell.

use std::time::Duration;

use opsdeck_ssh::{ShellChannel, ShellCommand, ShellEvent};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum TelnetError {
    #[error("connect failed: {0}")]
    Connect(String),
    #[error("connection timed out")]
    Timeout,
}

// Telnet protocol bytes (RFC 854/855/856/857/858/1091/1073).
const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const OPT_ECHO: u8 = 1;
const OPT_SUPPRESS_GA: u8 = 3;
const OPT_TTYPE: u8 = 24;
const OPT_NAWS: u8 = 31;
const OPT_BINARY: u8 = 0;
const TTYPE_IS: u8 = 0;
const TTYPE_SEND: u8 = 1;

/// Open a Telnet session: connect, negotiate, and return a ShellChannel
/// plus the raw task channels are wired inside. `term_type` is what we
/// report when the server asks (defaults to xterm-256color).
pub async fn open(
    host: &str,
    port: u16,
    cols: u16,
    rows: u16,
    term_type: Option<&str>,
    timeout: Duration,
) -> Result<ShellChannel, TelnetError> {
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect((host, port)))
        .await
        .map_err(|_| TelnetError::Timeout)?
        .map_err(|e| TelnetError::Connect(e.to_string()))?;
    let _ = stream.set_nodelay(true);

    let (event_tx, event_rx) = mpsc::unbounded_channel::<ShellEvent>();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ShellCommand>();
    let term = term_type.unwrap_or("xterm-256color").to_string();

    let task = tokio::spawn(async move {
        run(stream, cols, rows, term, event_tx, cmd_rx).await;
    });

    Ok(ShellChannel::wrap(event_rx, cmd_tx, task))
}

/// The session driver: multiplexes socket reads (with IAC negotiation)
/// and ShellCommands.
async fn run(
    mut stream: tokio::net::TcpStream,
    mut cols: u16,
    mut rows: u16,
    term: String,
    event_tx: mpsc::UnboundedSender<ShellEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<ShellCommand>,
) {
    let mut nego = Negotiation::default();
    // Proactively ask the server to suppress go-ahead & echo remotely is
    // normally server-driven; we only answer requests (see Negotiation).

    let mut read_buf = vec![0u8; 8192];
    loop {
        tokio::select! {
            res = stream.read(&mut read_buf) => {
                match res {
                    Ok(0) => {
                        let _ = event_tx.send(ShellEvent::Closed);
                        break;
                    }
                    Ok(n) => {
                        let (data, replies) = nego.process(&read_buf[..n], &term, cols, rows);
                        if !replies.is_empty() && stream.write_all(&replies).await.is_err() {
                            let _ = event_tx.send(ShellEvent::Closed);
                            break;
                        }
                        if !data.is_empty()
                            && event_tx.send(ShellEvent::Data(data)).is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(ShellEvent::Error(e.to_string()));
                        break;
                    }
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(ShellCommand::Write(data)) => {
                        let escaped = escape_iac(&data);
                        if stream.write_all(&escaped).await.is_err() {
                            let _ = event_tx.send(ShellEvent::Closed);
                            break;
                        }
                    }
                    Some(ShellCommand::Resize(c, r)) => {
                        cols = c;
                        rows = r;
                        if nego.naws_agreed {
                            let msg = naws_message(cols, rows);
                            let _ = stream.write_all(&msg).await;
                        }
                    }
                    Some(ShellCommand::Close) | None => {
                        let _ = stream.shutdown().await;
                        let _ = event_tx.send(ShellEvent::Closed);
                        break;
                    }
                }
            }
        }
    }
}

/// Escape literal 0xFF bytes in outbound data (IAC IAC).
fn escape_iac(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        out.push(b);
        if b == IAC {
            out.push(IAC);
        }
    }
    out
}

fn naws_message(cols: u16, rows: u16) -> Vec<u8> {
    let mut v = vec![IAC, SB, OPT_NAWS];
    for part in [cols.to_be_bytes(), rows.to_be_bytes()] {
        for b in part {
            v.push(b);
            if b == IAC {
                v.push(IAC);
            }
        }
    }
    v.extend_from_slice(&[IAC, SE]);
    v
}

/// Stateful IAC negotiation filter.
///
/// Policy: we agree to BINARY, SUPPRESS-GO-AHEAD, TERMINAL-TYPE and NAWS;
/// everything else is refused. Data bytes are passed through unmodified
/// (IAC IAC un-escapes to one 0xFF).
#[derive(Default)]
struct Negotiation {
    naws_agreed: bool,
    // leftover bytes of a partial multibyte sequence across reads
    state: NegoState,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum NegoState {
    #[default]
    Data,
    Iac,
    /// verb byte seen (DO/DONT/WILL/WONT), awaiting option
    Verb(u8),
    /// inside subnegotiation: (option, collected payload)
    Sub(u8),
    /// inside subnegotiation, last byte was IAC
    SubIac(u8),
}

impl Negotiation {
    fn process(&mut self, input: &[u8], term: &str, cols: u16, rows: u16) -> (Vec<u8>, Vec<u8>) {
        let mut data = Vec::with_capacity(input.len());
        let mut replies = Vec::new();
        for &b in input {
            match self.state {
                NegoState::Data => {
                    if b == IAC {
                        self.state = NegoState::Iac;
                    } else {
                        data.push(b);
                    }
                }
                NegoState::Iac => {
                    self.state = match b {
                        IAC => {
                            data.push(IAC);
                            NegoState::Data
                        }
                        WILL | DO | WONT | DONT => NegoState::Verb(b),
                        SB => NegoState::Sub(0xff), // option read next
                        _ => NegoState::Data,       // NOP, GA, etc.: ignore
                    };
                }
                NegoState::Verb(verb) => {
                    self.answer(verb, b, &mut replies, cols, rows);
                    self.state = NegoState::Data;
                }
                NegoState::Sub(0xff) => {
                    self.state = NegoState::Sub(b);
                }
                NegoState::Sub(opt) => {
                    if b == IAC {
                        self.state = NegoState::SubIac(opt);
                    } else if opt == OPT_TTYPE && b == TTYPE_SEND {
                        // Server asks for our terminal type: defer reply
                        // until SE so the reply stays in-band; we answer
                        // immediately anyway (servers accept both).
                        let mut msg = vec![IAC, SB, OPT_TTYPE, TTYPE_IS];
                        msg.extend_from_slice(term.as_bytes());
                        msg.extend_from_slice(&[IAC, SE]);
                        replies.extend_from_slice(&msg);
                    }
                }
                NegoState::SubIac(opt) => {
                    if b == SE {
                        self.state = NegoState::Data;
                    } else {
                        // escaped IAC inside subnegotiation payload: ignore
                        self.state = NegoState::Sub(opt);
                    }
                }
            }
        }
        (data, replies)
    }

    fn answer(&mut self, verb: u8, opt: u8, replies: &mut Vec<u8>, cols: u16, rows: u16) {
        let supported = matches!(
            opt,
            OPT_ECHO | OPT_SUPPRESS_GA | OPT_TTYPE | OPT_NAWS | OPT_BINARY
        );
        match verb {
            DO => {
                if supported {
                    replies.extend_from_slice(&[IAC, WILL, opt]);
                    if opt == OPT_NAWS {
                        self.naws_agreed = true;
                        replies.extend_from_slice(&naws_message(cols, rows));
                    }
                } else {
                    replies.extend_from_slice(&[IAC, WONT, opt]);
                }
            }
            WILL => {
                if supported {
                    replies.extend_from_slice(&[IAC, DO, opt]);
                } else {
                    replies.extend_from_slice(&[IAC, DONT, opt]);
                }
            }
            WONT => {
                replies.extend_from_slice(&[IAC, DONT, opt]);
            }
            DONT => {
                if opt == OPT_NAWS {
                    self.naws_agreed = false;
                }
                replies.extend_from_slice(&[IAC, WONT, opt]);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_data_passes_through() {
        let mut n = Negotiation::default();
        let (data, replies) = n.process(b"hello\r\n", "xterm-256color", 80, 24);
        assert_eq!(data, b"hello\r\n");
        assert!(replies.is_empty());
    }

    #[test]
    fn negotiation_answers() {
        let mut n = Negotiation::default();
        // IAC DO NAWS -> WILL NAWS + size subnegotiation
        let (data, replies) = n.process(&[IAC, DO, OPT_NAWS], "xterm-256color", 100, 40);
        assert!(data.is_empty());
        assert_eq!(&replies[..3], &[IAC, WILL, OPT_NAWS]);
        assert!(n.naws_agreed);
        // IAC WILL ECHO -> DO ECHO
        let (_, replies) = n.process(&[IAC, WILL, OPT_ECHO], "xterm-256color", 100, 40);
        assert_eq!(replies, vec![IAC, DO, OPT_ECHO]);
        // IAC DO 99 (unsupported) -> WONT 99
        let (_, replies) = n.process(&[IAC, DO, 99], "xterm-256color", 100, 40);
        assert_eq!(replies, vec![IAC, WONT, 99]);
    }

    #[test]
    fn escaped_iac_is_data() {
        let mut n = Negotiation::default();
        let (data, _) = n.process(&[0x41, IAC, IAC, 0x42], "xterm", 80, 24);
        assert_eq!(data, vec![0x41, IAC, 0x42]);
        assert_eq!(escape_iac(&[1, IAC, 2]), vec![1, IAC, IAC, 2]);
    }

    #[test]
    fn partial_sequences_survive_split_reads() {
        let mut n = Negotiation::default();
        let (d1, _) = n.process(&[0x41, IAC], "xterm", 80, 24);
        let (d2, r2) = n.process(&[DO, OPT_ECHO, 0x42], "xterm", 80, 24);
        assert_eq!(d1, vec![0x41]);
        assert_eq!(r2, vec![IAC, WILL, OPT_ECHO]);
        assert_eq!(d2, vec![0x42]);
    }
}
