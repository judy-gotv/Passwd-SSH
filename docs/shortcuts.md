# OpsDeck Keyboard Shortcuts

Author: Passwd Word

| Action | Windows / Linux | macOS |
|--------|-----------------|-------|
| New terminal tab | Ctrl+T | Cmd+T |
| Close tab | Ctrl+W | Cmd+W |
| Search terminal | Ctrl+F | Cmd+F |
| Host quick search | Ctrl+P | Cmd+P |
| Split left/right | Ctrl+Shift+E | Cmd+Shift+E |
| Split up/down | Ctrl+Shift+O | Cmd+Shift+O |
| Open SFTP | Ctrl+Shift+F | Cmd+Shift+F |
| Open monitor | Ctrl+Shift+M | Cmd+Shift+M |
| Copy selection | Ctrl+Shift+C | Cmd+C |
| Paste | Ctrl+V | Cmd+V |
| Switch tab 1–9 | Ctrl+1…9 | Cmd+1…9 |

## Terminal notes

- **Ctrl+C** with a selection copies it; without a selection it sends `^C`
  (SIGINT) to the remote shell.
- **Ctrl+V** pastes from the clipboard (the shell's quoted-insert is not
  available; use Ctrl+Shift+V in most shells or just type).
- **Shift+PageUp/PageDown** scroll the scrollback; **Shift+End** jumps back
  to the live output.
- **Alt+letter** sends ESC + letter (meta) on Linux/Windows.
- Function keys F1–F12, arrows, Home/End/Insert/Delete all send standard
  xterm sequences with modifier encoding (Shift=2, Alt=3, Ctrl=5…).
