# MiniChat native client

A MiniChat desktop client with no browser engine: Rust, [Slint] for the
interface, and nothing else between the app and the window.

The shipped client in `desktop/` is a [Tauri] window pointed at your instance.
It renders nothing itself — the interface is the web client, running in
WebView2. That is why it idles at several hundred megabytes doing nothing, and
it is why moving off WebView2 means rewriting the client rather than porting
the shell.

This is that rewrite.

## Why Rust rather than the C# preview

`desktop-native/` began the same migration in .NET/WPF. Two things argue
against finishing it there:

- **LiveKit has no C# client SDK.** The official clients are JS, Swift,
  Kotlin, Flutter, Unity, React Native and **Rust**. Voice and video are this
  product's headline feature, so the language that has a client SDK for them
  is not a detail.
- **The capture code already exists in Rust.** `desktop/src/capture.rs` (screen
  and window capture via `xcap`) and `desktop/src/capture_audio.rs` (WASAPI
  process loopback) are written, working and reusable here. A C# client throws
  them away and reimplements process-loopback audio through P/Invoke.

## Running it

```bash
cargo run
```

On first launch it asks which instance to connect to and remembers it.

Development aids:

```bash
# Render one frame headlessly and write a PNG — no display needed.
cargo run -- --screenshot shot.png 1180 820

# Any palette, so a design change can be reviewed in both themes.
cargo run -- --screenshot shot.png 1180 820 light
cargo run -- --screenshot shot.png 1180 820 --accent '#e8734a' --tint '#2a1810'

# Any dialog, over demo data.
cargo run -- --screenshot shot.png 1180 820 --overlay settings
# settings | profile | emoji | menu | confirm | search
```

The screenshot path renders through Slint's software renderer rather than
Skia, so it previews layout, colour and type rather than capturing the release
build pixel for pixel. It is how several layout bugs in this client were
found, including every dialog collapsing to its title bar.

## How it is put together

| | |
|---|---|
| `src/theme.rs` | The palette, computed with the same oklab `color-mix` the stylesheet uses |
| `src/text/markdown.rs` | The markdown subset MiniChat messages use, with the web client's precedence rules |
| `src/text/layout.rs` | Inline flow layout: message bodies become positioned runs |
| `src/api/` | The HTTP API and its wire types |
| `src/gateway.rs` | The WebSocket gateway, with reconnection and backoff |
| `src/store.rs` | All state, and the rules for folding gateway events into it |
| `src/view.rs` | Store → what the UI draws |
| `src/app.rs` | The two threads, and every callback |
| `ui/*.slint` | The interface |

Three decisions are worth knowing before changing anything.

**The design system is the web client's, token for token.** `ui/theme.slint`
mirrors the custom properties in `web/src/index.css`, and `src/theme.rs`
reproduces the `color-mix(in oklab, …)` those properties are built from. An
instance's accent and surface tint therefore retint this client to the same
bytes as the web one. Measurements are the stylesheet's too, converted from
rem at a 16px root — 0.65rem of button padding is written as 10.4px, not
rounded to 10.

**Message bodies are laid out in Rust, not marked up in Slint.** Slint has no
rich text: a `Text` element carries one font, one weight, one colour. A chat
message is the opposite — bold mid-sentence, inline code, a mention pill
between two words, a custom emoji, all wrapping as one paragraph. So
`src/text/layout.rs` measures each run with `rustybuzz` against the *same font
files* the renderer draws with and hands Slint positioned pieces. That is why
`assets/fonts/` exists and why a run carries the size it was measured at:
paint a run at a size other than its measured one and the glyphs are wider
than the box measured for them, which clips them.

The same reasoning produces the other "why is this in Rust" answers — the
emoji picker's grid is chunked into rows, an attachment block's height is
measured — because Slint has no wrapping layout and an element inside an `if`
has no id the surrounding bindings can read.

**The store is only ever touched from the UI thread; the network only from
the runtime.** Slint owns the main thread and its state is not `Send`, so
HTTP, the gateway and image fetches run on a Tokio runtime on another thread
and post results back. Nothing is shared but the channels between them.

## What it does

Connect to an instance, sign in, and:

- Text channels grouped into categories, with unread and mention counts,
  collapsible categories, private-channel and announcement glyphs
- Messages with grouping, day dividers, an unread marker, replies, reactions,
  attachments, pins and edit marks
- The full markdown subset: bold, italic, underline, strike, spoilers, inline
  code, fenced code blocks, quotes, links, mentions and custom emoji
- Optimistic sending, with failures marked rather than silently dropped
- A live gateway: new messages, edits, deletions, reactions, typing,
  presence, role and channel changes, and permission changes
- Search, pinned messages, member profiles, an emoji picker, right-click
  menus, and a settings dialog with dark/light/instance themes
- Direct messages: a conversation list, unread counts, optimistic sending
- Inline message editing, file attachments through the native picker, and
  scroll-back pagination
- Voice channels: join, leave, mute, deafen, who is in the room and who is
  speaking. Audio itself needs the `voice` feature — see below
- Desktop notifications, honouring the member's per-channel preferences
- A tray icon with the same menu the Tauri shell has, and close-to-tray
- Keyboard shortcuts: Ctrl/Cmd+K and Ctrl/Cmd+F to search, Alt+Up and
  Alt+Down to walk channels, Ctrl/Cmd+Shift+M and +D for mute and deafen,
  Enter to send and Shift+Enter for a newline, Escape to back out of
  whatever is open
- Frameless window with its own caption bar, matching the Tauri shell's

Not yet: **voice and video media** (see below), the admin panel, direct
calls, and global hotkeys that work while the app is in the background.

## Testing

`cargo test` covers the parts that have an answer worth pinning: the oklab
palette, the markdown grammar and its precedence, the flow layout's wrapping
and decorations, the permission bits, the store's gateway handling, the
notification rules.

Two kinds of check exist because unit tests cannot see two kinds of bug:

- `src/uitest.rs` builds the real window headlessly, dispatches real key
  events and asserts on what came out. It exists because binding send to
  `TextInput.accepted` compiled, rendered, and silently never sent anything —
  Slint only raises that callback on a single-line input.
- `tests/live.sh` starts a real server, sets an instance up, seeds it, and has
  the client sign in, read the gateway and render. Every other test would pass
  against a server that changed the shape of READY.

Note for anyone extending the UI tests: a `draw_if_needed` whose closure
ignores the renderer settles nothing. Slint evaluates properties lazily during
a render pass and `changed` handlers run as part of it, so the harness has to
render for real.

## Memory

The point of the exercise. Two things matter as much as the engine:

- **History is capped.** `MAX_MESSAGES_PER_CHANNEL` and `MAX_CACHED_CHANNELS`
  in `src/store.rs`. The web client keeps every message it has ever loaded in
  a map that is never trimmed and renders all of them into the DOM; that is a
  large part of the idle memory this client exists to avoid, and repeating it
  here would have wasted the exercise.
- **Images are bounded.** `src/images.rs` caps how many decoded images are
  held and scales them before keeping them. An instance decides what it
  serves; it must not also decide how much memory the client spends.

Measured on this branch: a signed-in client, connected to an instance with
its gateway open, sitting idle, holds **30.6 MB** resident and stays flat —
peak equals current over a minute, no sawtooth. The release binary is 26 MB,
of which 2.3 MB is the bundled fonts, and it has no runtime to install
alongside it.

Two caveats on that number, because it was taken in a container: it is the
software renderer, since there is no GPU there — Skia on a real one moves
work to the GPU rather than adding to resident memory, but it is not the same
measurement. And it is a small instance with a handful of messages and no
images loaded; the caps in `store.rs` and `images.rs` are what keep a busy one
from being a different story.

Note that once voice lands,
libwebrtc comes with it and a client *in a call* will be in the hundreds
whatever the UI toolkit — WebRTC is WebRTC. The win is concentrated in the
idle case, which is the case being complained about.

## Outstanding

- **Voice and video media.** The control plane is done — joining, leaving,
  the room's membership, mute and deafen, the sidebar panel. What is missing
  is the audio itself: `src/voice.rs` defines the `Engine` trait and says
  exactly what an implementation has to do, in the order the join path calls
  it. Beyond that trait: I420 frame rendering for camera and screen share,
  device enumeration and hot-plug, and the audio processing module (echo
  cancellation, gain, noise suppression).

  Build it with `cargo build --features voice`. That pulls in `livekit` and,
  under it, roughly 200MB of prebuilt libwebrtc. The artifact downloads
  cleanly; linking it on Linux needs **clang 21 or newer**, because the
  hermetic libc++ it ships with requires it. Ubuntu 24.04 tops out at clang
  18, so a Linux build needs a toolchain from apt.llvm.org. Windows links
  against MSVC and has no such constraint.

  This is the largest remaining piece by a wide margin, and it is where the
  schedule will be decided.
- **The session token is a plain file.** `src/settings.rs` writes it
  user-only where the platform allows, which is no worse than the web
  client's `localStorage`, but it is not the OS credential store and should
  be.
- **The tray is Windows and macOS only.** `tray-icon` needs GTK or
  libayatana-appindicator on Linux, which is a system dependency the rest of
  this client does not have. `tray::available()` reports it, and the window
  refuses to hide itself where there is no tray to restore it from.
- **Colour emoji depend on the system font.** Windows has Segoe UI Emoji, so
  the ship target is fine; a Linux box with no colour emoji font renders them
  monochrome.
- **Slint's fontique API is behind `unstable-fontique-011`.** It is how a
  single-binary client registers embedded fonts in 1.18; the alternative is
  pointing `SLINT_FONT_PATH` at files on disk. Pinned with `~1.18`.

## Licences

`assets/fonts/` carries Inter and JetBrains Mono, both SIL Open Font
License 1.1, with their licences beside them. The icons are traced from
[Lucide], ISC licensed, which the web client already depends on.

[Slint]: https://slint.dev
[Tauri]: https://tauri.app
[Lucide]: https://lucide.dev
