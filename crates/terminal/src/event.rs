//! Terminal event channel types (docs §6.5).
//!
//! These carry bytes between the SSH transport task and the UI thread.
//!
//! Author: Passwd Word

use tokio::sync::mpsc;

/// Events flowing from the terminal backend to the UI.
#[derive(Debug)]
pub enum TerminalEvent {
    /// Raw PTY / SSH stdout bytes.
    Output(Vec<u8>),
    /// The pty/ssh side changed size.
    Resize { cols: u16, rows: u16 },
    /// The session closed.
    Closed,
    /// Transport error message.
    Error(String),
}

/// Commands flowing from the UI to the terminal backend.
#[derive(Debug)]
pub enum TerminalCommand {
    /// Raw stdin bytes.
    Input(Vec<u8>),
    /// Resize request.
    Resize { cols: u16, rows: u16 },
    /// Paste text (converted to bytes).
    Paste(String),
    /// Clear scrollback.
    Clear,
    /// Close the terminal.
    Close,
}

/// Bidirectional handle for one terminal (docs §6.5).
pub struct TerminalHandle {
    pub command_tx: mpsc::Sender<TerminalCommand>,
    pub event_rx: mpsc::Receiver<TerminalEvent>,
}

impl TerminalHandle {
    /// Create a connected pair with a bounded command queue.
    pub fn new() -> (Self, TerminalIo) {
        let (command_tx, command_rx) = mpsc::channel(256);
        let (event_tx, event_rx) = mpsc::channel(256);
        (
            Self { command_tx, event_rx },
            TerminalIo { command_rx, event_tx },
        )
    }
}

impl Default for TerminalHandle {
    fn default() -> Self {
        Self::new().0
    }
}

/// The backend half of a [`TerminalHandle`].
pub struct TerminalIo {
    pub command_rx: mpsc::Receiver<TerminalCommand>,
    pub event_tx: mpsc::Sender<TerminalEvent>,
}
