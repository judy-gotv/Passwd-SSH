//! Default keyboard shortcuts (docs §8.3).
//! Author: Passwd Word

/// A documented shortcut.
#[derive(Debug, Clone)]
pub struct Shortcut {
    /// Action id (stable).
    pub action: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// Windows / Linux binding.
    pub windows_linux: &'static str,
    /// macOS binding.
    pub macos: &'static str,
}

/// Default shortcuts (docs §8.3).
pub fn default_shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut { action: "new_tab", description: "New terminal tab", windows_linux: "Ctrl+T", macos: "Cmd+T" },
        Shortcut { action: "close_tab", description: "Close tab", windows_linux: "Ctrl+W", macos: "Cmd+W" },
        Shortcut { action: "find", description: "Search terminal", windows_linux: "Ctrl+F", macos: "Cmd+F" },
        Shortcut { action: "host_search", description: "Host quick search", windows_linux: "Ctrl+P", macos: "Cmd+P" },
        Shortcut { action: "split_h", description: "Split left/right", windows_linux: "Ctrl+Shift+E", macos: "Cmd+Shift+E" },
        Shortcut { action: "split_v", description: "Split up/down", windows_linux: "Ctrl+Shift+O", macos: "Cmd+Shift+O" },
        Shortcut { action: "open_sftp", description: "Open SFTP", windows_linux: "Ctrl+Shift+F", macos: "Cmd+Shift+F" },
        Shortcut { action: "open_monitor", description: "Open monitor", windows_linux: "Ctrl+Shift+M", macos: "Cmd+Shift+M" },
        Shortcut { action: "copy", description: "Copy selection", windows_linux: "Ctrl+Shift+C", macos: "Cmd+C" },
        Shortcut { action: "paste", description: "Paste", windows_linux: "Ctrl+Shift+V", macos: "Cmd+V" },
    ]
}
