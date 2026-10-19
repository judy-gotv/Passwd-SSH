# OpsDeck Architecture

Author: Passwd Word

## Crate map

```
┌─────────────────────────────────────────────────────────┐
│ app  (eframe binary)                                    │
│  owns: tokio runtime, Services, UiState, event loop     │
└──┬──────────┬──────────┬──────────┬──────────┬──────────┘
   │          │          │          │          │
   ▼          ▼          ▼          ▼          ▼
  ui ──► terminal    ssh ◄── sftp   ops      sync
   │       (render   (sessions,    (engine   (records,
   │        model)    shells)       queue)    remotes)
   │          │          │          │          │
   └──────────┴────► storage ◄──────┴──────────┘
                     (SQLite)
   vault ──► used by app's CredentialProvider; never by UI directly
```

## Layer rules (docs §12.1)

1. **UI never touches the DB or the network.** `opsdeck-ui` receives a
   `UiData` snapshot per frame and returns `UiEvent`s. All I/O lives in `app`.
2. **Terminal is a pure render model.** `opsdeck-terminal` wraps
   `alacritty_terminal`; the UI draws `RenderedTerminal` (spans + cursor) with
   the egui painter — no retained widget state in egui itself.
3. **SSH is storage-agnostic.** `opsdeck-ssh` resolves secrets through the
   `CredentialProvider` trait, which the app implements on top of
   vault + storage. The ssh crate never imports storage or vault.
4. **Sync sees ciphertext only.** `opsdeck-sync` encrypts records with the
   vault key before they reach any `Remote`.

## Frame flow

```
frame N:
  app.drain_messages()          # shells → bytes, prompts, transfers, metrics
  app.refresh_cache()           # DB → CachedData (1 Hz)
  ui::render(ctx, state, data)  # draw; collect UiEvents
  app.handle_event(ev)          # spawn backend work / mutate state
  ctx.request_repaint()         # while sessions/transfers/sync are live
```

Shell output path: russh channel → `ShellChannel.events` (unbounded mpsc) →
`UiState::feed_terminal` (alacritty `Term::advance`) → next frame renders.
Input path: egui key/text events → `UiEvent::TerminalInput(bytes)` →
`ShellCommand::Write` → russh channel writer.

## Session model

`SessionManager` owns `ManagedSession`s keyed by `SessionId` (UUID).
`connect_chain(&[HostConfig])` dials jumps first, target last, tunneling each
subsequent TCP connect through the previous session
(`channel_open_direct_tcpip`). Shells are separate `open_shell` calls so one
session can host a terminal, SFTP channels and exec commands concurrently.

## SFTP

`RusshSftpEngine` multiplexes over `SessionManager`: each operation opens a
fresh `session` channel + `sftp` subsystem (cheap, avoids shared-state bugs).
`TransferQueue` runs one worker task per transfer with 250 ms / 256 KiB
progress throttling, pause (abort + mark), retry, cancel and resume-after-
restart (unfinished tasks are re-queued as paused).

## Monitoring

`MetricsCollector` runs a fixed set of `/proc`-based commands over `exec`
(`loadavg`, `free -b`, `df -B1`, two `/proc/stat` samples 1 s apart,
`/proc/net/dev`, `ps`). Every command degrades independently — a failed parse
leaves that field `None` instead of failing the snapshot. The app polls every
5 s per monitored host and keeps the last 120 snapshots for charts.
