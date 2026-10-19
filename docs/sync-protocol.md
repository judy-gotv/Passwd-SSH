# OpsDeck Sync Protocol (V1.1 framework)

Author: Passwd Word

The sync layer replicates **hosts**, **host groups** and **snippets** between
a user's devices. Everything is end-to-end encrypted: the remote only stores
opaque records.

## Record

```json
{
  "id": "hosts:6f1c…",
  "collection": "hosts",
  "entity_id": "6f1c…",
  "version": 1730000000,
  "updated_at": 1730000000,
  "deleted": false,
  "device_id": "…",
  "nonce": [12 bytes],
  "ciphertext": [ChaCha20-Poly1305 of the entity JSON]
}
```

`version`/`updated_at` are unix seconds (monotonic per device). `deleted`
marks tombstones. The plaintext entity JSON is what `storage` holds for that
collection.

## Change tracking

Each device keeps a local manifest (`sync.manifest` setting):
`record_id → {version, updated_at}`. Export seals every entity whose
`updated_at` is newer than the manifest entry; entities in the manifest but
missing from storage become tombstones.

## Transports

### FileRemote (works today)

One JSON file per record in a shared directory (`<sanitized-id>.json`).
Any folder-sync tool (Syncthing, Nextcloud, rsync) replicates it.

### HttpRemote (server spec)

Base URL = the configured endpoint.

- `GET {base}/v1/health` → `200 {"ok":true}`
- `POST {base}/v1/push` — body `{"device_id":"…","records":[…]}`
  → `200` (server keeps the newest `version` per record id)
- `GET {base}/v1/pull?since=<unix>` → `200 {"records":[…]}`

The server is a dumb encrypted-blob store: it never sees plaintext and needs
no user accounts beyond whatever fronts it (TLS + firewall/VPN is the
expected deployment).

## Conflicts

`resolve(local, remote, local_changed_since_base)`:

- Remote not newer → keep local, no conflict.
- Remote newer, local unchanged since the manifest → apply remote.
- Remote newer, local changed too → **last-write-wins** by `updated_at`
  (ties → remote); a `Conflict` record is logged and counted so a future UI
  can offer manual merge.

## Security properties

See [security.md](security.md): ciphertext-only remote, no credential sync,
per-record random nonces, device ids are random UUIDs.
