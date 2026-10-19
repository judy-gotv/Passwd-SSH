# OpsDeck Security Model

Author: Passwd Word

## Vault

- Master password → **Argon2id** (default params, 32-byte output) → key.
- Every credential is encrypted with **ChaCha20-Poly1305** (random 96-bit
  nonce per encryption).
- The verifier is an encrypted check value: unlocking with the wrong password
  fails authentication — the app cannot distinguish "wrong password" from
  "corrupt vault", and reports only "Wrong password."
- `SecretBytes` / `SecretString` are `zeroize`d on drop and never `Debug`-
  printed (redacted).
- Rekey (`ChangeMasterPassword`) decrypts every credential with the old key
  and re-encrypts with the new key in one background task.

## Host keys (TOFU)

- First connection to `host:port`: the full OpenSSH public-key line is shown
  and the user must choose **Trust once / Trust always / Reject**.
- A *changed* key is **never** silently accepted — the dialog warns about a
  possible man-in-the-middle attack and defaults to Reject on timeout (120 s).
- "Trust always" persists to `known_hosts.json` (JSON, next to the config).

## Sync (§7.3)

- Only **hosts, host groups and snippets** sync. Credentials, vault data and
  terminal contents **never** leave the device.
- Records are sealed with the vault key (`SyncRecord.seal`) — the remote
  (shared folder or sync server) only ever sees ciphertext + metadata
  (`updated_at`, `deleted`, `device_id`).
- Conflict resolution: last-write-wins by `updated_at`; ties go to the
  remote; conflicts are logged and counted for a future manual-merge UI.

## Transport

- SSH via `russh` (Rust). No shell-outs to the system `ssh` binary.
- The sync HTTP client pins no certificates beyond the OS trust store
  (native certs, Mozilla fallback) — a corporate proxy that MITMs TLS will
  fail closed.

## Local data

- SQLite at the platform data dir (`OpsDeck/opsdeck.db`), WAL mode.
- The DB contains host metadata and transfer history in cleartext — full-disk
  encryption is the expected protection there; secrets are always encrypted.
- Clipboard: terminal selections are copied as plain text (standard).

## Non-goals

- No password recovery: forgetting the master password loses the credentials
  (by design).
- No protection against a compromised local OS user (keyloggers, memory
  scraping) beyond zeroization.
