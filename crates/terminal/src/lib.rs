//! OpsDeck Terminal: VT/xterm emulation state.
//!
//! - [`Terminal`]: feed bytes in, read a styled [`RenderedTerminal`] out.
//! - [`input`]: keyboard → byte-sequence mapping.
//! - [`event`]: [`TerminalEvent`]/[`TerminalCommand`] channel types (docs §6.5).
//!
//! Author: Passwd Word

pub mod event;
pub mod input;
mod term;

pub use event::{TerminalCommand, TerminalEvent, TerminalHandle, TerminalIo};
pub use input::{KeyMods, TermKey, ctrl_byte, key_to_bytes};
pub use term::{
    RenderLine, RenderedTerminal, RgbColor, StyledSpan, Terminal, TerminalError,
};
