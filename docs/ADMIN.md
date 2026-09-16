# Running an instance

Everything here is in the admin panel: the shield icon at the bottom of the
sidebar, visible to anyone holding an admin permission.

## Permissions

| Flag | Lets a member… |
|---|---|
| `VIEW_CHANNELS` | See channels at all |
| `SEND_MESSAGES` | Post in text channels |
| `MANAGE_MESSAGES` | Delete other people's messages, bypass slow mode |
| `PIN_MESSAGES` | Pin and unpin |
| `ATTACH_FILES` | Upload files, avatars and banners |
| `ADD_REACTIONS` | React to messages |
| `MENTION_EVERYONE` | Use `@everyone` and `@here` |
| `CONNECT` | Join voice channels |
| `SPEAK` | Publish audio |
| `VIDEO` | Publish camera |
| `SCREEN_SHARE` | Publish a screen share |
| `MUTE_MEMBERS` | Mute others in voice |
| `MOVE_MEMBERS` | Disconnect others from voice, bypass user limits |
| `MANAGE_CHANNELS` | Create, edit, reorder and delete channels |
| `MANAGE_ROLES` | Create and edit roles, assign them, set channel overrides |
| `MANAGE_INSTANCE` | Branding, rules, registration mode, upload limits |
| `CREATE_INVITES` | Create and revoke their own invite links |
| `KICK_MEMBERS` | Remove and suspend members |
| `BAN_MEMBERS` | Ban and unban |
| `VIEW_AUDIT_LOG` | Read the audit log |
| `MANAGE_WEBHOOKS` | Create and delete integrations |
| `MANAGE_NICKNAMES` | Rename other members |
| `ADMINISTRATOR` | Everything, including permissions added in future versions |

### Rank

Every role has a rank. A role ranked at or above your own highest role is one
you cannot edit, delete or assign — which is what stops a moderator from
promoting themselves. You also cannot grant a permission you do not hold.

Operators sit outside this system: they always have every permission and can
only be promoted or demoted by another operator.

### Channel overrides

Channels → pick a channel → **Permission overrides**. Each permission is
allow (✓), deny (✕) or inherit (/). Deny always wins over allow, and
`ADMINISTRATOR` bypasses overrides entirely.

Two common setups:

- **Announcements** — deny `SEND_MESSAGES` for the default role, allow it for
  the role that posts. New announcement channels get this automatically.
- **Private channel** — deny `VIEW_CHANNELS` for the default role, allow it for
  the roles that should see it. Ticking "Private channel" at creation time sets
  the deny for you.

## Members

Members → per-row actions:

- **Shield** (operators only) — promote or demote another operator
- **Suspend** — the account stays but can't sign in; reversible
- **Ban** — suspends and records a ban; the Bans view lifts it

All three end the member's live sessions immediately. Removing a member deletes
their account but keeps their messages, so conversations stay readable.

## Registration

Instance → **Joining**:

- **Invite only** (default) — accounts need a valid invite link
- **Open** — anyone who can reach the URL can sign up
- **Closed** — no new accounts at all

Invites can carry an expiry, a use limit and a role granted on join. Only
someone with `MANAGE_ROLES` can attach a role to an invite, so plain invite
rights can't be used to hand out administrator.

## Integrations

Integrations → **New webhook** gives you a URL that posts into one channel:

```bash
curl -X POST 'https://chat.example.com/webhooks/<id>/<token>' \
  -H 'Content-Type: application/json' \
  -d '{"content": "Backup completed", "username": "cron"}'
```

`text` is accepted as an alias for `content`. The `username` field overrides
the display name per message. The URL is the only credential — anyone with it
can post, so rotate it by deleting and recreating the webhook.

## Audit log

Every moderation and configuration change is recorded with who did it, what
changed and when. Entries stream in live over the gateway, and they are only
ever sent to members holding `VIEW_AUDIT_LOG`.

## Troubleshooting

**Voice says "not configured".** `LIVEKIT_URL`, `LIVEKIT_API_KEY` and
`LIVEKIT_API_SECRET` must all be set. Restart after changing `.env`.

**Voice connects but nobody hears anything.** Almost always blocked media
ports. Check that UDP 50000–50200 and TCP 7881 reach the host. If members are
behind firewalls that block both, enable the built-in TURN relay in
`livekit.yaml`.

**Microphone permission never gets asked.** The page isn't on HTTPS. Browsers
only grant capture on secure origins.

**Certificates won't issue.** Both DNS records must resolve to the server
before Caddy can validate them, and ports 80 and 443 must be reachable.
`docker compose logs caddy` says which check failed.

**Locked out of the admin panel.** Operators always keep every permission, so
this shouldn't happen — but if the operator account is gone, stop the stack and
promote a user directly:

```bash
docker compose run --rm --entrypoint sh minichat -c \
  "apt-get update -qq && apt-get install -y -qq sqlite3 && \
   sqlite3 /data/minichat.db \"UPDATE users SET is_operator = 1 WHERE username = 'you';\""
```
