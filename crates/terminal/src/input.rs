//! Keyboard → terminal byte-sequence mapping (xterm-style).
//!
//! The UI crate translates egui keys into [`TermKey`]; this module turns them
//! into the byte sequences a PTY expects. Printable characters bypass this
//! module (they are sent as UTF-8 directly); Ctrl+letter combos are handled
//! by the UI layer too.
//!
//! Author: Passwd Word

/// Special keys the terminal understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermKey {
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// F1..F12 (values 1..=12).
    F(u8),
}

/// Modifier state for a key press.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyMods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyMods {
    /// xterm modifyOtherKeys-style parameter: 1 + shift + 2*alt + 4*ctrl.
    fn param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }
}

/// Translate a special key + modifiers into terminal bytes.
pub fn key_to_bytes(key: TermKey, mods: KeyMods) -> Vec<u8> {
    // Application-cursor / modified arrows: ESC [ 1 ; <param> <final>.
    fn modified_arrow(final_byte: u8, mods: KeyMods) -> Vec<u8> {
        let p = mods.param();
        if p == 1 {
            vec![0x1b, b'[', final_byte]
        } else {
            format!("\x1b[1;{p}{}", final_byte as char).into_bytes()
        }
    }
    // Modified Home/End/PgUp/PgDn etc: ESC [ <n> ; <param> ~.
    fn modified_tilde(n: u8, mods: KeyMods) -> Vec<u8> {
        let p = mods.param();
        if p == 1 {
            format!("\x1b[{n}~").into_bytes()
        } else {
            format!("\x1b[{n};{p}~").into_bytes()
        }
    }

    match key {
        TermKey::Enter => vec![b'\r'],
        TermKey::Tab if mods.shift => b"\x1b[Z".to_vec(),
        TermKey::Tab => vec![b'\t'],
        TermKey::Backspace => vec![0x7f],
        TermKey::Escape => vec![0x1b],
        TermKey::Insert => modified_tilde(2, mods),
        TermKey::Delete => modified_tilde(3, mods),
        TermKey::PageUp => modified_tilde(5, mods),
        TermKey::PageDown => modified_tilde(6, mods),
        TermKey::Up => modified_arrow(b'A', mods),
        TermKey::Down => modified_arrow(b'B', mods),
        TermKey::Right => modified_arrow(b'C', mods),
        TermKey::Left => modified_arrow(b'D', mods),
        TermKey::Home => {
            let p = mods.param();
            if p == 1 { b"\x1b[H".to_vec() } else { format!("\x1b[1;{p}H").into_bytes() }
        }
        TermKey::End => {
            let p = mods.param();
            if p == 1 { b"\x1b[F".to_vec() } else { format!("\x1b[1;{p}F").into_bytes() }
        }
        TermKey::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
    }
}

/// Ctrl+letter → control byte (Ctrl+A = 0x01 … Ctrl+Z = 0x1A).
/// Returns `None` for characters without a control mapping.
pub fn ctrl_byte(c: char) -> Option<u8> {
    let upper = c.to_ascii_uppercase();
    if ('A'..='Z').contains(&upper) {
        Some(upper as u8 - b'A' + 1)
    } else {
        match c {
            ' ' => Some(0x00),
            '[' => Some(0x1b),
            '\\' => Some(0x1c),
            ']' => Some(0x1d),
            '^' => Some(0x1e),
            '_' => Some(0x1f),
            '?' => Some(0x7f),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_special_keys() {
        let none = KeyMods::default();
        assert_eq!(key_to_bytes(TermKey::Enter, none), b"\r");
        assert_eq!(key_to_bytes(TermKey::Up, none), b"\x1b[A");
        assert_eq!(key_to_bytes(TermKey::Down, none), b"\x1b[B");
        assert_eq!(key_to_bytes(TermKey::Delete, none), b"\x1b[3~");
        assert_eq!(key_to_bytes(TermKey::F(1), none), b"\x1bOP");
        assert_eq!(key_to_bytes(TermKey::F(5), none), b"\x1b[15~");
        assert_eq!(key_to_bytes(TermKey::Home, none), b"\x1b[H");
    }

    #[test]
    fn modified_keys() {
        let ctrl = KeyMods { ctrl: true, ..Default::default() };
        assert_eq!(key_to_bytes(TermKey::Up, ctrl), b"\x1b[1;5A");
        let shift = KeyMods { shift: true, ..Default::default() };
        assert_eq!(key_to_bytes(TermKey::Tab, shift), b"\x1b[Z");
        let alt = KeyMods { alt: true, ..Default::default() };
        assert_eq!(key_to_bytes(TermKey::Left, alt), b"\x1b[1;3D");
    }

    #[test]
    fn ctrl_letters() {
        assert_eq!(ctrl_byte('c'), Some(0x03));
        assert_eq!(ctrl_byte('C'), Some(0x03));
        assert_eq!(ctrl_byte('z'), Some(0x1a));
        assert_eq!(ctrl_byte('1'), None);
    }
}
