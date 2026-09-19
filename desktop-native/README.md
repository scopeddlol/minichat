# Chromium-free Windows client preview

This is the first migration slice: a standalone .NET 10 / WPF executable with no
Tauri, WebView2, Electron, browser engine, or JavaScript runtime dependency.
The existing `desktop/` client remains the full-featured release until this
client reaches parity. Windows is the currently shipped desktop platform.

## Run

Install the .NET 10 SDK on Windows, then from the repository root:

```powershell
dotnet run --project desktop-native/MiniChat.Native.csproj
```

For a portable build that includes the .NET runtime:

```powershell
dotnet publish desktop-native/MiniChat.Native.csproj -c Release -r win-x64 --self-contained true -o desktop-native/bin/portable
```

The executable uses native controls and calls the existing MiniChat HTTP API.
It supports HTTPS instance selection (HTTP only for localhost development),
username/password sign-in, visible text and announcement channels, the most
recent 100 messages, and sending text. Server permissions remain authoritative.
Session tokens stay in memory and are discarded on sign-out/exit. Redirects are
rejected to avoid forwarding a login to an unexpected origin.

This preview polls every three seconds; it does not advertise online presence
or acknowledge messages as read. Attachments show filenames only. It is not yet
a replacement for the released client. No passwords or sessions are migrated
from WebView2, and no browser fallback is embedded.

## Remaining migration work

- Replace polling with the authenticated gateway, reconnect/backoff, presence,
  read acknowledgements, and incremental state updates.
- Port message history pagination, rich content, attachments, DMs, channel drag
  ordering, role-aware context menus, and administration.
- Integrate native LiveKit/WebRTC media, WASAPI capture/playback, device selection,
  push-to-talk, cues, camera and screen sharing. Validate two-client calls before
  switching the release installer. The preview deliberately has no voice buttons.
- Add tray integration, notifications, protected session persistence and packaging.
- Switch the release workflow and remove `desktop/` and WebView2 only after parity
  and accessibility testing. CI builds this preview separately in the meantime.

WPF reference: https://learn.microsoft.com/en-us/dotnet/desktop/wpf/overview/
