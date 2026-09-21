# Changelog

## 0.6.1

- Fixed the native client panicking on every launch that had a saved instance to open. A `state.borrow()` in the scrutinee of an `if let` outlived its block, so the `borrow_mut()` inside could never succeed. Present since the lightbox work; nothing caught it because nothing ran the client with a saved instance.
- Fixed sign-in reporting the wrong reason. Every 401 became "Sign in again. Your session was rejected.", including the one the server sends for a wrong password — which is the only 401 anyone meets while trying to start a session. The server's own "Incorrect username or password." is kept now, as the web client has always kept it. A 401 during an established session still signs out.
- A `--signin` mode runs the whole client headlessly and drives the real screen, so `tests/live.sh` can check that a correct password reaches the app and that a wrong one says which. The existing `--live` tests went around the screen entirely, which is why neither bug above was caught.
- **The release ships installers rather than archives**, and the native client alone:

  ```
  MiniChat-0.6.1-x86_64-setup.exe    per-user, no administrator, Start Menu
                                     entry, uninstaller, /S and /INSTANCE=
  minichat-native_0.6.1_amd64.deb    on PATH and in the menu
  MiniChat-0.6.1-x86_64.AppImage     everywhere else, no root
  ```

  CI installs each one, runs the client from where the package put it, and uninstalls again. The `.deb`'s dependencies are the detected link-time set plus the libraries winit, FemtoVG and libwebrtc open by name at run time — `dpkg-shlibdeps` cannot see a `dlopen`, so a package built from its answer alone installs cleanly and then opens no window.
- **The Tauri shell is no longer released.** It moves to `tauri.yml`, which builds it and publishes nothing.

### Deployment

The Windows `.msi` and `.exe` from 0.6.0 are not superseded by an update — the native installer is a separate application and does not upgrade them. Camera and screen share remain unimplemented in the native client.

Images: `ghcr.io/scopeddlol/minichat:v0.6.1` for Linux amd64 and arm64.

## 0.6.0

- A Chromium-free native client in `native/`, written in Rust with Slint and no browser engine. It covers channels, messages, direct messages, voice, search, administration, friends, forwarding, the image lightbox, notifications and global voice hotkeys. It holds about 30 MB idle where the WebView2 shell holds several hundred, and runs as a single binary with no runtime installed beside it. This replaces the WPF preview started in 0.5.0, which is gone: LiveKit has no C# client SDK, and the screen-capture code already existed in Rust.
- **A Linux desktop client**, the first one. `minichat-native-0.6.0-x86_64-linux.tar.gz` is attached to the release. Unpack and run it; it needs fontconfig and the xkbcommon and xcb libraries, which a desktop system already has.
- The native client also ships for Windows as `minichat-native-0.6.0-x86_64-windows.zip`. It is a **preview** there: the `.msi` remains the recommended Windows download until the native client has been through accessibility testing.
- Released native binaries are built with the `voice` feature, so they carry audio rather than joining voice channels silently.
- An admin panel, direct calls, voice media with real audio, and global voice hotkeys in the native client.
- The Windows installers are branded, and are now built on every change rather than only at a tag, so a bundle that cannot be packaged is caught before a release depends on it. The installer's `/INSTANCE` switch is parsed without `GetOptions`.
- Fixed the CI failures on the voice job and two clippy lints.
- Fixed voice-enabled release linking on both platforms: the native UI uses FemtoVG to avoid Skia/WebRTC JPEG symbol collisions, and Windows uses the static MSVC runtime required by LiveKit. Release render checks run on both Linux and Windows.

### Deployment

Camera and screen share are **not** implemented in the native client — use the Tauri installer or the web client for those. The native client's session token is a plain user-only file rather than the OS credential store. There is no tray icon on Linux, so the window will not hide itself where there is nothing to restore it from. The `.msi` and `.exe` installers are unchanged in behaviour from 0.5.0.

Images: `ghcr.io/scopeddlol/minichat:v0.6.0` for Linux amd64 and arm64.

## 0.5.0

- Fixed silent incoming voice: remote microphone and screen-share audio now attach to persistent playback elements, including while browsing text or DMs. Blocked autoplay has an explicit Enable call audio button; local microphones never play back.
- Fixed Windows channel dragging by disabling the Tauri native drop handler that intercepts HTML drag events.
- Context menus focus after becoming visible without scrolling their underlying channel/message away. Member menus now include permission- and hierarchy-checked kick, ban and voice disconnect actions with confirmation.
- Voice cues fall back to the default speaker if the saved output device is unavailable.
- Started a Chromium-free Windows client in `desktop-native/` using native WPF controls. The independently buildable preview supports sign-in, channel browsing and text messaging; it is not yet a voice/video replacement.
- Renamed the default branch to `scopeddlol/main` and included it in container publishing triggers.

These changes require an updated server/web build; the Windows drag fix also requires a rebuilt desktop installer. Existing v0.4.0 installations do not update themselves from this branch.

- Windows desktop screen sharing uses a bundled MiniChat screen/window picker with previews and an always-visible stop control. Capture requires an explicit selection in that local window. Optional audio shares the selected application, or other applications when sharing a display, excluding MiniChat's call audio.
- Camera sharing opens a custom preview and device selector before publishing. Microphone and speaker menus are available beside mute/deafen in channels and private calls.
- Voice join/leave cues play for participants already in the call as well as the person connecting/disconnecting. Mute, unmute, deafen, and undeafen have local cues routed to the selected output.
- Opening DMs clears the channel highlight and preserves unread messages and mentions in the previous channel.
- Members with Manage Channels permission can drag channels to reorder them or move them into another category. Synced permissions move atomically with the channel; invalid moves leave the previous order intact.

The native picker requires the updated Windows desktop client. Browsers retain their own screen-capture consent UI. Native application audio requires Windows process-loopback support; the picker reports an unavailable audio device and allows sharing without audio. Closing or minimizing a shared window ends its capture. Capture frame rates depend on source size and system performance.

## 0.4.0

- A calmer, flatter interface with neutral dark and light surfaces, roomier channel and member navigation, more comfortable message spacing, and reduced-motion support.
- Context-aware right-click menus for members, message authors, channels, and channel navigation. Keyboard users can open menus with Shift+F10 and navigate with arrow keys. Member actions include direct messages, nicknames, and role management within the existing role hierarchy.
- Private one-to-one messages with persistent history, earlier-message pagination, unread counts, live updates, and independent conversation drafts.
- Private audio/video calls with incoming-call prompts, accept/decline/cancel/end controls, microphone, camera, and screen sharing through the existing LiveKit service. Unanswered calls expire after 45 seconds.
- Responsive inbox navigation, viewport-clamped context menus, bounded dialogs, and fixes for overflowing reply previews and long channel names.
- Membership checks isolate direct conversations and call tokens from community roles, including administrators. Existing channel data is preserved by an additive SQLite migration.
- Friends, favourites, blocking, profile image framing, message forwarding, and message links are included alongside category permission inheritance and channel overrides.
- Blocking ends active private calls and prevents new contact. Permission changes refresh connected clients and hide inaccessible channel/category metadata and voice presence.

### Deployment

Back up the SQLite database before upgrading. The server applies migrations 0004 and 0005 on startup. Calls require the existing LiveKit configuration and HTTPS, just like community voice channels. Direct messages in this release support text; channel attachments, reactions, and search remain channel features. Calls ring while MiniChat is open; they do not wake an offline device. Messages are stored on the instance and are not end-to-end encrypted.

Images: `ghcr.io/scopeddlol/minichat:v0.4.0` for Linux amd64 and arm64. Windows MSI and EXE installers are attached to the draft GitHub release by the desktop workflow.
