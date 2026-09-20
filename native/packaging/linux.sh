#!/usr/bin/env bash
# Package the native client for Linux: a .deb and an AppImage.
#
# Two, because Linux has no single answer. The .deb is what someone on
# Debian or Ubuntu expects — it puts the binary on PATH, the icon in the
# menu, and `apt remove minichat` takes it away again. The AppImage is for
# everyone else: one file, marked executable, run from anywhere, no root.
#
#   native/packaging/linux.sh <binary> <version> <outdir>
#
# Needs dpkg-deb (any Debian-ish build box has it). appimagetool is fetched
# if it is not already on PATH; without network, the AppImage is skipped and
# the .deb is still produced.

set -euo pipefail

BINARY=${1:?usage: linux.sh <binary> <version> <outdir>}
VERSION=${2:?usage: linux.sh <binary> <version> <outdir>}
OUTDIR=${3:?usage: linux.sh <binary> <version> <outdir>}

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
NATIVE=$(cd "$HERE/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$OUTDIR"
BINARY=$(cd "$(dirname "$BINARY")" && pwd)/$(basename "$BINARY")
OUTDIR=$(cd "$OUTDIR" && pwd)

[ -x "$BINARY" ] || { echo "not an executable: $BINARY" >&2; exit 1; }

# --- what both packages contain -----------------------------------------

stage_common() {
  local root=$1
  install -Dm755 "$BINARY" "$root/usr/bin/minichat-native"
  install -Dm644 "$HERE/linux/minichat.desktop" \
    "$root/usr/share/applications/minichat.desktop"
  install -Dm644 "$NATIVE/assets/icons/128x128.png" \
    "$root/usr/share/icons/hicolor/128x128/apps/minichat.png"
  install -Dm644 "$NATIVE/assets/icons/32x32.png" \
    "$root/usr/share/icons/hicolor/32x32/apps/minichat.png"
  install -Dm644 "$NATIVE/README.md" "$root/usr/share/doc/minichat/README.md"
  # The fonts are compiled into the binary, so their licences travel with it.
  for licence in "$NATIVE"/assets/fonts/*-LICENSE.txt; do
    install -Dm644 "$licence" "$root/usr/share/doc/minichat/$(basename "$licence")"
  done
}

# --- the .deb ------------------------------------------------------------

DEB_ROOT="$WORK/deb"
stage_common "$DEB_ROOT"
mkdir -p "$DEB_ROOT/DEBIAN"

# What it links against, asked of the binary rather than guessed — and then
# what it does not link against at all.
#
# The windowing and audio libraries are opened by name at run time: winit
# picks X11 or Wayland when it starts, FemtoVG asks for GL, libwebrtc's
# audio module asks for ALSA. None of that is in the binary's DT_NEEDED, so
# `dpkg-shlibdeps` cannot see any of it, and a package built from its answer
# alone installs cleanly and then opens no window. So the linked set is
# detected and the loaded set is stated.
LINKED=""
if command -v dpkg-shlibdeps > /dev/null 2>&1; then
  pushd "$DEB_ROOT" > /dev/null
  mkdir -p debian
  touch debian/control
  LINKED=$(dpkg-shlibdeps -O --ignore-missing-info "usr/bin/minichat-native" 2>/dev/null \
    | sed 's/^shlibs:Depends=//') || LINKED=""
  rm -rf debian
  popd > /dev/null
fi
if [ -z "$LINKED" ]; then
  LINKED="libc6, libfontconfig1, libgcc-s1"
  echo "dpkg-shlibdeps unavailable; falling back to a minimum linked set" >&2
fi

# `libasound2 | libasound2t64` because Ubuntu renamed the package in 24.04.
# The new one Provides the old name, but an alternative costs nothing and
# says what happened.
LOADED="libxkbcommon0, libxcb1, libx11-6, libxi6, libxrender1, libxcursor1,
 libxrandr2, libgl1, libfreetype6, libasound2 | libasound2t64"
LOADED=$(echo "$LOADED" | tr -d '\n')

# A Wayland session is served by these; an X11 one is not, and neither is a
# Wayland session with XWayland. Recommends rather than Depends so that
# installing the client does not drag Wayland onto a machine that has none.
SUGGESTED="libwayland-client0, libwayland-cursor0, libwayland-egl1, libxkbcommon-x11-0"

SIZE=$(du -ks "$DEB_ROOT" | cut -f1)
cat > "$DEB_ROOT/DEBIAN/control" <<CONTROL
Package: minichat
Version: $VERSION
Section: net
Priority: optional
Architecture: amd64
Depends: $LINKED, $LOADED
Recommends: $SUGGESTED
Installed-Size: $SIZE
Maintainer: MiniChat <noreply@users.noreply.github.com>
Homepage: https://github.com/scopeddlol/minichat
Description: Voice and text chat for one self-hosted community
 The native MiniChat client: a desktop app with no browser engine in it,
 which is why it holds tens of megabytes where an Electron or WebView shell
 holds hundreds. Point it at your instance and sign in.
CONTROL

DEB="$OUTDIR/minichat-native_${VERSION}_amd64.deb"
dpkg-deb --build --root-owner-group "$DEB_ROOT" "$DEB" > /dev/null
echo "built $DEB"

# --- the AppImage --------------------------------------------------------

APPIMAGETOOL=${APPIMAGETOOL:-$(command -v appimagetool || true)}
if [ -z "$APPIMAGETOOL" ]; then
  echo "fetching appimagetool"
  if curl -fsSL -o "$WORK/appimagetool" \
      "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"; then
    chmod +x "$WORK/appimagetool"
    APPIMAGETOOL="$WORK/appimagetool"
  else
    echo "could not fetch appimagetool; skipping the AppImage" >&2
    exit 0
  fi
fi

APPDIR="$WORK/MiniChat.AppDir"
stage_common "$APPDIR"
# An AppImage looks for these at the top of the AppDir, not in /usr.
cp "$HERE/linux/minichat.desktop" "$APPDIR/minichat.desktop"
cp "$NATIVE/assets/icons/128x128.png" "$APPDIR/minichat.png"
ln -sf minichat.png "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
# Run the client from wherever the AppImage was mounted.
HERE=$(dirname "$(readlink -f "$0")")
exec "$HERE/usr/bin/minichat-native" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

APPIMAGE="$OUTDIR/MiniChat-${VERSION}-x86_64.AppImage"
# --appimage-extract-and-run because a container has no FUSE, and the build
# box is not where the AppImage has to mount itself.
ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" "$APPIMAGE" > /dev/null 2>&1 \
  || ARCH=x86_64 "$APPIMAGETOOL" "$APPDIR" "$APPIMAGE"
chmod +x "$APPIMAGE"
echo "built $APPIMAGE"
