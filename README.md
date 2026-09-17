<div align="center">

# MiniChat

**Voice, video and text chat for one community — on your own server.**

Discord's channels and voice rooms, Slack's calm. Self-hosted with Docker,
installable as a mobile PWA and a Windows desktop app.

</div>

---

## What it is

MiniChat runs a single community — one instance, one set of channels, one
member list. No servers-within-servers, no directory, no discovery. You invite
the people you want and that's who's there.

| | |
|---|---|
| **Text channels** | Markdown, replies, reactions, pins, edits, search, file uploads, slow mode |
| **Mentions** | `@name` autocomplete, a separate mention badge, and an unread divider so you can see where you left off |
| **Notifications** | Web Push to phone and desktop, per-channel muting, and a test button |
| **Custom emoji** | Upload your own, use them as `:name:` or as reactions |
| **Voice & video** | LiveKit rooms per channel — camera, screen share, device pickers, per-member volume, push-to-talk, speaking indicators |
| **Announcements** | Read-only channels where only the people you allow can post |
| **Roles** | 23 permission flags, rank-based hierarchy, per-channel overrides |
| **Invites** | Link-based signup with expiry, use limits and optional auto-granted roles |
| **Admin panel** | Stats, branding, channels, roles, members, bans, invites, webhooks, live audit log |
| **Profiles** | Avatar, banner, bio, pronouns, favourite game, status, personal accent colour |
| **Apps** | Installable PWA (iOS, Android, desktop) and a native Windows build |

Built with **Rust** (axum + SQLite) on the server and **TypeScript** (React +
Vite) on the client. One container, one database file, no external services.

---

## Quick start

You need a machine with Docker, and a domain pointed at it. HTTPS is not
optional — browsers refuse microphone and camera access without it, so voice
and video simply will not work over plain HTTP.

### 1. Point two DNS records at your server

```
chat.example.com          A     203.0.113.10
livekit.chat.example.com  A     203.0.113.10
```

Both must resolve before you start, because Caddy verifies them when it
requests certificates.

### 2. Get the files and write your `.env`

```bash
mkdir minichat && cd minichat
curl -O https://raw.githubusercontent.com/scopeddlol/minichat/v0.2.0/compose.yml
curl -o .env https://raw.githubusercontent.com/scopeddlol/minichat/v0.2.0/.env.example
```

Two files is all you need — the image is prebuilt, so there's nothing to
compile and no repository to clone.

Open `.env` and set your two domains, then generate the secrets:

```bash
openssl rand -hex 32   # JWT_SECRET
openssl rand -hex 32   # LIVEKIT_API_SECRET
openssl rand -hex 16   # SETUP_TOKEN (optional but recommended)
```

For push notifications, generate a VAPID keypair and paste both halves in:

```bash
docker run --rm ghcr.io/scopeddlol/minichat:latest minichat-server generate-vapid
```

### 3. Start it

```bash
docker compose up -d
```

This pulls `ghcr.io/scopeddlol/minichat`, built for `linux/amd64` and
`linux/arm64` — so a Raspberry Pi, an ARM VPS and an x86 box all work the same
way.

Caddy requests certificates on first boot; give it a few seconds. Then open
**https://chat.example.com** and the setup wizard walks you through:

1. **Operator account** — the account that can never be locked out
2. **Branding** — name, tagline, icon, banner, accent colour (which retints the whole app)
3. **Community** — rules, welcome message, who's allowed to sign up
4. **Roles** — what everyone can do by default, plus any extra roles
5. **Channels** — text, voice and announcement channels, grouped into categories

The wizard runs exactly once. If `SETUP_TOKEN` is set, it asks for that token
first — worth doing if your instance is reachable from the internet before you
get a chance to run it.

### 4. Invite people

Admin panel → **Invites** → **New invite**. Set an expiry, a use limit, and
optionally a role the invite grants on join. Share the link; they pick a
username and they're in.

---

### Pin a version

`.env` defaults to `MINICHAT_TAG=latest`. For anything you rely on, pin a
release instead, so upgrading is something you choose rather than something a
restart does to you:

```bash
MINICHAT_TAG=v0.2.0
```

### Build from source instead

```bash
git clone https://github.com/scopeddlol/minichat.git
cd minichat
cp .env.example .env
docker compose -f compose.yml -f compose.build.yml up -d --build
```

## Ports

| Port | Who needs it | What for |
|---|---|---|
| 80, 443 | public | HTTP/HTTPS, plus HTTP/3 on 443/udp |
| 7881 | public | WebRTC over TCP — the fallback when UDP is blocked |
| 50000–50200/udp | public | WebRTC media (voice and video) |

Everything else stays on the internal Docker network. If members behind
restrictive corporate firewalls can't connect to voice, enable LiveKit's
built-in TURN relay in `livekit.yaml`.

---

## The apps

### Mobile and desktop PWA

Open the instance in a browser and install it: **Add to Home Screen** on iOS,
the install prompt on Android, or the install icon in the address bar on
desktop Chrome and Edge. Settings → **Apps** has a button that triggers it
directly where the browser supports it.

Installing matters on iPhone and iPad: Safari only delivers push notifications
to apps on the Home Screen, never to a browser tab. Settings → **Notifications**
says so in place of the toggle when it detects that situation.

### Windows desktop app

A native window built with Tauri. Tag a release and GitHub Actions builds the
installers for you:

```bash
git tag v0.1.0 && git push --tags
```

`.github/workflows/desktop.yml` produces an `.msi` and an `.exe` and attaches
them to the release. To build locally on Windows:

```bash
cd desktop
npm install
npm run build
```

On first launch the app asks for your instance address and remembers it.
**File → Switch instance…** changes it later.

If the connect screen says it can't reach the app's internals, the build was
made without `withGlobalTauri` enabled in `desktop/tauri.conf.json` — the
connect screen has no bundler, so it reads the injected `window.__TAURI__`
global rather than importing the API.

The desktop app also registers **global voice hotkeys**, so push-to-talk works
while you're in a game or another window — something a browser tab fundamentally
cannot do. Defaults are `F8` (push to talk), `F9` (mute) and `F10` (deafen);
change them in `settings.json` inside the app's config directory, or turn them
off from **File → Toggle global voice hotkeys**.

---

## Development

Run the server and client separately with hot reload:

```bash
# Terminal 1 — API on :8080
cd server
export JWT_SECRET=$(openssl rand -hex 32)
cargo run

# Terminal 2 — client on :5173, proxying /api to :8080
cd web
npm install
npm run dev
```

Voice and video stay disabled until you set `LIVEKIT_URL`, `LIVEKIT_API_KEY`
and `LIVEKIT_API_SECRET`; everything else works without them.

```
minichat/
├── compose.yml          # Caddy + MiniChat + LiveKit
├── Caddyfile            # Automatic HTTPS and reverse proxying
├── livekit.yaml         # SFU configuration
├── server/              # Rust: axum, SQLite, WebSocket gateway
│   ├── migrations/      # Schema, applied on boot
│   └── src/routes/      # One module per API area
├── web/                 # TypeScript: React, Vite, Tailwind, PWA
│   └── src/lib/         # API client, gateway, store, LiveKit
└── desktop/             # Tauri shell for Windows
```

Further reading: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
[docs/ADMIN.md](docs/ADMIN.md), [docs/API.md](docs/API.md).

---

## Integrations

Admin panel → **Integrations** creates an incoming webhook URL for a channel:

```bash
curl -X POST 'https://chat.example.com/webhooks/<id>/<token>' \
  -H 'Content-Type: application/json' \
  -d '{"content": "Deploy finished ✅", "username": "CI"}'
```

`text` works as an alias for `content`, so most Slack-style senders work
unchanged. Treat the URL as a secret — anyone holding it can post.

---

## Disk usage

Uploads that are never attached to anything — a file picked and then abandoned,
an avatar chosen but not saved — are swept hourly once they're a day old. The
sweep compares the uploads directory against every URL the database still
references, so nothing in use is ever touched. Admin panel → **Overview** shows
total upload size, and `POST /api/admin/sweep-uploads` runs it on demand.

## Backups

Everything lives in one Docker volume: the SQLite database and the uploads.

```bash
docker run --rm -v minichat_minichat_data:/data -v "$PWD":/backup \
  debian:bookworm-slim tar czf /backup/minichat-backup.tar.gz -C /data .
```

Restore by extracting it back into the volume with the stack stopped. Keep
`.env` alongside the backup — without `JWT_SECRET` every member gets signed
out, and without `LIVEKIT_API_SECRET` voice stops working.

---

## Upgrading

```bash
docker compose pull
docker compose up -d
```

Migrations run automatically at startup. Back up first. If you build from
source, `git pull` and add `-f compose.build.yml --build` as above.

## Continuous integration

| Workflow | Runs on | Does |
|---|---|---|
| `ci.yml` | every push and PR | `cargo fmt`, Clippy with warnings denied, release build, `tsc`, web build |
| `docker.yml` | pushes, PRs, `v*` tags | Builds the image for amd64 and arm64, smoke-tests each one, publishes to GHCR |
| `desktop.yml` | `v*` tags, manual | Builds the Windows installers and attaches them to a draft release |

`docker.yml` builds each architecture on a runner of that architecture rather
than under emulation, then merges the two into one manifest — an emulated arm64
Rust build takes roughly thirty minutes, a native one takes the same time as
amd64. Every build is started, queried over HTTP and checked for a clean
migration run before it is published, so a broken image never reaches the
registry.

### Publishing a release

```bash
git tag v0.2.0
git push origin v0.2.0
```

That publishes the image as `ghcr.io/scopeddlol/minichat:v0.2.0` (and `:0.2`)
and opens a **draft** GitHub release with the Windows installers attached, for
you to review before making it public.

### Making the image public

The first publish creates the GHCR package as **private**, which means
`docker compose up` fails with `denied` for everyone else. Fix it once, in
**Package settings → Danger Zone → Change visibility → Public**, at
`https://github.com/users/scopeddlol/packages/container/minichat/settings`.
While it's private, pulling needs `docker login ghcr.io` with a token that has
`read:packages`.

---

## Security notes

- Passwords are hashed with Argon2; sessions are JWTs carrying a version
  number, so changing a password or suspending an account ends every live
  session immediately.
- Permissions are enforced on the server for every request. The client's copy
  only decides what to draw.
- Private channels are filtered out of the realtime gateway per connection —
  events for a channel you can't see are never sent to you.
- Uploads are served with `nosniff`, a restrictive CSP and a sandbox header.
  SVG uploads are rejected, since they can carry script and would be served
  from the instance's own origin.
- An operator can never be locked out by role misconfiguration, and can't be
  kicked or banned by anyone else.

---

## Licence

MIT.
