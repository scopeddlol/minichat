# Changelog

## 0.4.0

- A calmer, flatter interface with neutral dark and light surfaces, roomier channel and member navigation, more comfortable message spacing, and reduced-motion support.
- Context-aware right-click menus for members, message authors, channels, and channel navigation. Keyboard users can open menus with Shift+F10 and navigate with arrow keys. Member actions include direct messages, nicknames, and role management within the existing role hierarchy.
- Private one-to-one messages with persistent history, earlier-message pagination, unread counts, live updates, and independent conversation drafts.
- Private audio/video calls with incoming-call prompts, accept/decline/cancel/end controls, microphone, camera, and screen sharing through the existing LiveKit service. Unanswered calls expire after 45 seconds.
- Responsive inbox navigation, viewport-clamped context menus, bounded dialogs, and fixes for overflowing reply previews and long channel names.
- Membership checks isolate direct conversations and call tokens from community roles, including administrators. Existing channel data is preserved by an additive SQLite migration.

### Deployment

Back up the SQLite database before upgrading. The server applies migration 0004 on startup. Calls require the existing LiveKit configuration and HTTPS, just like community voice channels. Direct messages in this release support text; channel attachments, reactions, and search remain channel features. Calls ring while MiniChat is open; they do not wake an offline device. Messages are stored on the instance and are not end-to-end encrypted.

Images: `ghcr.io/scopeddlol/minichat:v0.4.0` for Linux amd64 and arm64. Windows MSI and EXE installers are attached to the draft GitHub release by the desktop workflow.
