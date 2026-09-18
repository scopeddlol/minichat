# Changelog

## Unreleased

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
