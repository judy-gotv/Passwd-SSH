# OpsDeck — SSH Workstation

A local-first SSH workstation for Windows, macOS and Linux: tabbed terminals,
SFTP file management, host monitoring, command snippets and end-to-end
encrypted sync — all in one Rust desktop app.

> Author: Passwd Word

## Features

- **Terminal** — GPU-accelerated egui UI, full xterm emulation (alacritty_terminal),
  tabs, split panes, search, 10k-line scrollback, CJK-safe rendering
- **SSH** — password, private-key and SSH-agent auth; ProxyJump chains; host-key
  TOFU verification with change warnings; keepalive + reconnect
- **Hosts** — groups, favorites, search, tags, JSON import/export
- **SFTP** — browse, upload/download with resume, mkdir/rename/delete,
  background transfer queue with pause/retry
- **Monitor** — CPU/memory/disk/load charts and top processes (Linux hosts)
- **Snippets** — command library with one-click run + batch execution
- **Port forwarding** — local forwards (`-L` style) per session
- **Vault** — Argon2id + ChaCha20-Poly1305 encrypted credential store,
  unlocked by one master password
- **Sync** — end-to-end encrypted sync of hosts/groups/snippets via a shared
  folder or an OpsDeck sync server (docs/sync-protocol.md)
- **Auto-update** — checks GitHub Releases for newer versions

## Quick start

### From a release (recommended)

Download the package for your OS from
[Releases](https://github.com/judy-gotv/Passwd-SSH/releases):

| OS      | File                      |
|---------|---------------------------|
| Windows | `opsdeck-<ver>-windows-x86_64.zip` |
| macOS   | `opsdeck-<ver>-macos-<arch>.tar.gz` |
| Linux   | `opsdeck-<ver>-linux-x86_64.tar.gz` / `.deb` |

Unpack and run `opsdeck`. On first launch, create a master password —
it encrypts every stored credential and cannot be recovered.

### From source

Requires Rust 1.85+ (edition 2024).

```bash
git clone https://github.com/judy-gotv/Passwd-SSH.git
cd Passwd-SSH
cargo run --release -p opsdeck
```

Linux needs the usual GUI dev libraries (X11/Wayland); see
`.github/workflows/ci.yml` for the `apt` list used in CI.

## Layout

```
crates/
  app/        eframe binary: wires every crate together
  ui/         egui views (never touches the DB or network directly)
  terminal/   alacritty_terminal wrapper: input, render model, search
  ssh/        russh sessions: auth, shells, exec, forwarding, ProxyJump
  sftp/       russh-sftp engine + transfer queue
  ops/        metrics collector, snippets, batch runner
  vault/      Argon2id + ChaCha20-Poly1305 credential vault
  storage/    SQLite (sqlx) persistence: hosts, groups, snippets, transfers
  sync/       encrypted sync records, file/HTTP remotes, update checker
docs/         architecture, security model, shortcuts, sync protocol
```

See [docs/architecture.md](docs/architecture.md) for the full picture and
[docs/security.md](docs/security.md) for the threat model.

## Data

- Linux: `~/.local/share/OpsDeck/opsdeck.db`, config in `~/.config/OpsDeck/`
- macOS: `~/Library/Application Support/OpsDeck/`
- Windows: `%APPDATA%\PasswdWord\OpsDeck\`

The database holds host metadata and transfer history. Secrets live only in
the vault-encrypted `credentials` table. Sync uploads ciphertext only.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
