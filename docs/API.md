# HTTP API

Base path `/api`. Authenticate with `Authorization: Bearer <token>`, obtained
from `/api/auth/login`, `/api/auth/register` or `/api/setup`.

Errors are JSON with a human-readable message, using conventional status codes:

```json
{ "error": "You don't have permission to do that in this channel." }
```

Permission bitfields are sent as **strings**, because `ADMINISTRATOR` is
`1 << 30` and the flag space exceeds what a JSON number holds safely.

## Public

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/meta` | Instance name, branding, registration mode, whether setup has run |
| `POST` | `/setup` | First-run wizard. Works once, then returns 409 |
| `POST` | `/auth/register` | Create an account (invite required unless registration is open) |
| `POST` | `/auth/login` | `{ username, password }` → `{ token }` |
| `GET` | `/invites/preview/{code}` | Validity and branding for an invite link |
| `POST` | `/webhooks/{id}/{token}` | Post as a webhook (outside `/api`) |

## Session

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/auth/me` | The current member |
| `POST` | `/auth/password` | Change password; signs out other devices |
| `POST` | `/auth/revoke-sessions` | Sign out everywhere else |
| `PATCH` | `/users/@me` | Update your profile |
| `GET` | `/users/{id}` | A member's public profile |
| `GET` | `/members` | Every member |

## Channels and messages

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/channels` | Channels you can view |
| `GET` | `/channels/permissions` | Your effective permission bits per channel |
| `POST` | `/channels` | Create a channel · `MANAGE_CHANNELS` |
| `PATCH` `DELETE` | `/channels/{id}` | Edit or delete · `MANAGE_CHANNELS` |
| `GET` `PUT` | `/channels/{id}/overwrites` | Per-role channel overrides · `MANAGE_ROLES` |
| `GET` `POST` | `/categories` | List and create categories |
| `GET` | `/channels/{id}/messages` | `?before=` `?after=` `?around=` `?limit=` (max 100) |
| `POST` | `/channels/{id}/messages` | `{ content, reply_to_id?, attachments? }` |
| `PATCH` `DELETE` | `/messages/{id}` | Edit your own; delete yours or, with `MANAGE_MESSAGES`, anyone's |
| `PUT` `DELETE` | `/messages/{id}/pin` | Pin and unpin · `PIN_MESSAGES` |
| `PUT` `DELETE` | `/messages/{id}/reactions/{emoji}` | React · `ADD_REACTIONS` |
| `GET` | `/channels/{id}/pins` | Pinned messages |
| `POST` | `/channels/{id}/ack` | Mark read up to a message |
| `GET` | `/search` | `?q=` `?channel_id=` `?author_id=` across channels you can see |
| `POST` | `/uploads` | `multipart/form-data`, field `file` · `ATTACH_FILES` |

Messages are paginated by ID. IDs sort chronologically, so `before` is the
oldest message you hold and `after` is the newest. `around` returns a window
centred on one message, which is how jumping to a search result or a reply
loads context that isn't on screen.

Attachments are two steps: `POST /uploads` returns an attachment object, then
you pass it in the message's `attachments` array. Only URLs this server issued
are accepted.

## Notifications and emoji

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/push/key` | VAPID public key and whether push is configured |
| `POST` | `/push/subscribe` | Register a browser subscription |
| `POST` | `/push/unsubscribe` | Drop one by endpoint |
| `POST` | `/push/test` | Send a test notification to your own devices |
| `GET` `PATCH` | `/notifications` | Notification mode, globally and per channel |
| `GET` | `/emojis` | Custom emoji |
| `POST` | `/emojis` | Add one · `MANAGE_EMOJI` |
| `PATCH` `DELETE` | `/emojis/{id}` | Rename or delete · `MANAGE_EMOJI` |

Mentions are plain `@username` in the message body — the server resolves them
on create and on edit, skipping anything inside code spans or fences so a code
sample never pings. `@everyone` and `@here` need `MENTION_EVERYONE`; without it
the text still renders but notifies nobody.

## Voice

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/voice/{channel_id}/token` | LiveKit token; publish grants mirror your permissions · `CONNECT` |
| `GET` | `/voice/states` | Who is in which voice channel |
| `POST` | `/voice/disconnect/{user_id}` | Drop someone from voice · `MOVE_MEMBERS` |

## Administration

| Method | Path | Permission |
|---|---|---|
| `GET` `PATCH` | `/admin/instance` | `MANAGE_INSTANCE` |
| `GET` | `/admin/stats` | `MANAGE_INSTANCE` |
| `POST` | `/admin/sweep-uploads` | `MANAGE_INSTANCE` |
| `GET` | `/admin/permissions` | any — the permission catalogue |
| `GET` `POST` | `/admin/roles` | `MANAGE_ROLES` |
| `PATCH` `DELETE` | `/admin/roles/{id}` | `MANAGE_ROLES` |
| `PUT` `DELETE` | `/admin/members/{id}/roles/{role_id}` | `MANAGE_ROLES` |
| `PATCH` | `/admin/members/{id}` | `MANAGE_NICKNAMES` / `KICK_MEMBERS` / operator |
| `DELETE` | `/admin/members/{id}` | `KICK_MEMBERS` |
| `GET` | `/admin/bans` | `BAN_MEMBERS` |
| `PUT` `DELETE` | `/admin/bans/{id}` | `BAN_MEMBERS` |
| `GET` | `/admin/audit` | `VIEW_AUDIT_LOG` |
| `GET` `POST` | `/admin/webhooks` | `MANAGE_WEBHOOKS` |
| `GET` `POST` `DELETE` | `/invites`, `/invites/{code}` | `CREATE_INVITES` |

## Gateway

`wss://<host>/api/gateway`. The first frame must be `identify`:

```json
{ "op": "identify", "token": "<jwt>" }
```

The server replies with `READY`, carrying the member, instance, roles,
categories, visible channels, members, voice states, unread counts and your
per-channel permissions — everything the client needs to render without
follow-up requests.

### Client frames

| `op` | Payload |
|---|---|
| `identify` | `{ token }` |
| `ping` | — |
| `typing` | `{ channel_id }` |
| `presence` | `{ presence: "online" \| "idle" \| "dnd" }` |
| `voice_state` | `{ channel_id \| null, muted, deafened, video, streaming }` |
| `ack` | `{ channel_id, message_id }` |

### Server events

Every event is `{ "t": "<NAME>", "d": { … } }`.

`READY` · `INVALID_SESSION` · `MESSAGE_CREATE` · `MESSAGE_UPDATE` ·
`MESSAGE_DELETE` · `REACTION_UPDATE` · `TYPING_START` · `PRESENCE_UPDATE` ·
`MEMBER_ADD` · `MEMBER_UPDATE` · `MEMBER_REMOVE` · `CHANNEL_CREATE` ·
`CHANNEL_UPDATE` · `CHANNEL_DELETE` · `CATEGORY_CREATE` · `CATEGORY_UPDATE` ·
`CATEGORY_DELETE` · `ROLE_CREATE` · `ROLE_UPDATE` · `ROLE_DELETE` ·
`INSTANCE_UPDATE` · `VOICE_STATE_UPDATE` · `VOICE_STATE_LEAVE` ·
`VOICE_FORCE_DISCONNECT` · `AUDIT_ENTRY` · `MENTION_ADD` · `EMOJI_CREATE` ·
`EMOJI_UPDATE` · `EMOJI_DELETE` · `PONG`

Events are filtered per connection. Channel events only reach members who can
view that channel, and `AUDIT_ENTRY` only reaches members holding
`VIEW_AUDIT_LOG`.
