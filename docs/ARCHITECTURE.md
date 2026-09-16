# Architecture

MiniChat is two programs and one file: a Rust binary that serves both the API
and the built web client, and a SQLite database next to it. There is no
separate database server, no cache, no job queue.

```
                        ┌──────────────────────────┐
   browser / PWA ──443──│  Caddy (automatic HTTPS) │
   Windows app          └────────┬──────────┬──────┘
                                 │          │
                    /api, /uploads          │ wss (signalling)
                    static client           │
                                 ▼          ▼
                        ┌────────────┐  ┌──────────┐
                        │  minichat  │  │ LiveKit  │
                        │  (Rust)    │  │  (SFU)   │
                        └─────┬──────┘  └────┬─────┘
                              │              │
                       SQLite + uploads   UDP 50000-50200
                        (Docker volume)   TCP 7881  ← media, direct
```

Media never passes through the Rust server. MiniChat mints a signed LiveKit
token describing what the member is allowed to do, and the browser talks to
the SFU directly.

## Server

`server/src/`

| Module | Responsibility |
|---|---|
| `main.rs` | Wiring: database pool, migrations, static files, graceful shutdown |
| `config.rs` | Environment configuration; refuses to boot on a weak `JWT_SECRET` |
| `auth.rs` | Argon2 hashing, JWT issuing, the `Auth` request extractor |
| `perms.rs` | The permission bitflags, shared as a catalogue with the client |
| `access.rs` | Permission resolution, row hydration, audit writes |
| `gateway.rs` | The WebSocket gateway |
| `livekit.rs` | Access-token minting for voice and video |
| `mentions.rs` | Parsing `@name` out of message bodies and storing the results |
| `push.rs` | Web Push delivery, recipient resolution, VAPID key generation |
| `sweeper.rs` | Hourly cleanup of uploads nothing references |
| `routes/` | One module per API area |

### Authentication

Sessions are stateless JWTs (HS256, 30 days) carrying the user ID and a
`token_version`. The version is stored on the user row and compared on every
request, so bumping it — on a password change, a suspension, a ban, or "sign
out everywhere" — invalidates every outstanding token instantly without a
session table.

### Permissions

23 flags in an `i64`, resolved in three layers:

1. The union of the member's role permissions
2. `ADMINISTRATOR` implies everything else
3. Per-channel overwrites, applied deny-then-allow

Operators always receive `ADMINISTRATOR`, so an instance can never be locked
out of its own admin panel by a role mistake.

Two rules keep privilege escalation off the table: you cannot grant a
permission you do not hold yourself, and you cannot edit, delete or assign a
role ranked at or above your own.

The client receives its effective bits per channel in the `READY` payload and
refreshes them when roles or channel overrides change. That copy only decides
what to draw — every request is checked again on the server.

### The gateway

One WebSocket per open client at `/api/gateway`. The first frame must be
`identify` carrying the token, which keeps it out of proxy access logs.

Events are published to a `tokio::sync::broadcast` channel and each connection
filters them against its own viewer:

| Scope | Delivered to |
|---|---|
| `All` | every connection |
| `Channel(id)` | members who can view that channel |
| `User(id)` | that member's connections |
| `Permission(bits)` | members holding a permission — audit log entries, for instance |

Because filtering happens per connection, an event for a private channel is
never written to a socket that shouldn't see it.

Presence is derived from live connection counts rather than a stored flag, so a
crashed process or a closed laptop can't leave someone stuck "online".

### Database

SQLite in WAL mode with foreign keys on. Migrations in `server/migrations/` are
embedded into the binary at compile time and run at startup.

IDs are lexicographically sortable — 12 hex characters of millisecond
timestamp, a per-process counter, then randomness. Sorting by ID sorts by
creation time, so message pagination is a plain `WHERE id < ? ORDER BY id DESC`
with no secondary index or offset scan.

## Client

`web/src/`

| Path | Responsibility |
|---|---|
| `lib/api.ts` | Typed fetch wrapper; XHR for uploads so progress works |
| `lib/gateway.ts` | WebSocket client with exponential backoff and jitter |
| `lib/store.ts` | Zustand store; every gateway event is reduced here |
| `lib/voice.ts` | LiveKit room lifecycle, tracks, devices, moderation |
| `lib/markdown.tsx` | Markdown subset rendered to React nodes |
| `lib/push.ts` | Push subscription lifecycle and platform quirks |
| `lib/hotkeys.ts` | Push-to-talk, mute and deafen bindings |
| `sw.ts` | Service worker: precaching, push display, notification clicks |
| `routes/` | Setup wizard, auth, chat shell |
| `components/admin/` | Admin panel tabs |

Message sends are optimistic: the message appears immediately and is replaced
by the server's copy when it arrives, or marked failed with a retry button.

Markdown is parsed to React elements directly. Nothing is ever inserted as
HTML, so a message cannot inject markup no matter what it contains.

Permission bits are handled as `BigInt` — `ADMINISTRATOR` is `1 << 30` and the
flag space runs past what a JSON number can carry safely, so the API sends them
as strings.

## Notifications

Mentions are stored as rows rather than re-scanned from message text, so
"unread mentions in this channel" is one indexed query. `@everyone` is
materialised into a row per viewer at write time, which keeps that query
identical whether the mention was personal or broadcast.

Push recipients are resolved per message: anyone mentioned, plus anyone whose
notification mode for that channel is "all", minus the author, minus anyone who
can't view the channel, minus anyone who muted it. Delivery is spawned rather
than awaited — sending to a dozen endpoints should never hold up the HTTP
response for the message that triggered it. Subscriptions the browser has
retired (HTTP 404/410) are deleted on the spot instead of being retried
forever.

The service worker suppresses a notification when a MiniChat window is already
focused, so an open laptop doesn't double up with the in-app badge.

## Uploads

`POST /api/uploads` writes to disk before the caller decides whether to send
the message, so abandoned files would otherwise accumulate. An hourly sweep
compares the uploads directory against every URL the database still references
— attachments, avatars, banners, webhook icons, custom emoji — and deletes
unreferenced files older than a day. Treating the filesystem as the source of
truth avoids a bookkeeping table that could drift out of sync with it.

## Voice and video

1. Client asks `POST /api/voice/{channel}/token`
2. Server checks `CONNECT` on that channel, checks the user limit, and mints a
   LiveKit JWT whose publish grants mirror the member's `SPEAK`, `VIDEO` and
   `SCREEN_SHARE` permissions
3. Client connects to the SFU directly and announces its state over the gateway
4. Everyone else sees them join in the sidebar

A member without `SPEAK` gets a token that cannot publish audio at all — the
restriction is enforced by the SFU, not by hiding a button.

Device selection, per-member volume and hotkey bindings are client-side and
stored in `localStorage`: they describe one person's hardware and preferences,
not instance state, so there is nothing for the server to know.

Push-to-talk toggles the microphone track rather than the mute flag, so a held
key never clobbers a manual mute. In a browser the bindings only fire while the
window is focused, which is a platform limit; the desktop app registers the
same actions as OS-level shortcuts and injects them into the page as
`minichat:hotkey` events. That direction matters — the remote instance page has
no IPC access, so Rust pushes events in rather than the page calling out.
