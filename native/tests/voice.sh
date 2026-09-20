#!/usr/bin/env bash
# The media engine against a real LiveKit server.
#
# `cargo test --features voice` compiles the engine and covers everything it
# can without a room to join. This is the rest: a real server, a real
# microphone track, and a second participant watching from the outside to
# confirm the audio is really going out.
#
# Needs clang 21 or newer for libwebrtc, and audio devices. On a machine with
# neither a sound card nor PulseAudio (a CI runner, usually) start a null sink
# first:
#
#   pulseaudio --start --exit-idle-time=-1
#   pactl load-module module-null-sink sink_name=speaker
#   pactl load-module module-null-source source_name=mic
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here/.."

version="${LIVEKIT_VERSION:-1.9.1}"
bin="${LIVEKIT_SERVER:-}"
if [[ -z "$bin" ]]; then
    bin="$(command -v livekit-server || true)"
fi
if [[ -z "$bin" ]]; then
    bin="$here/../target/livekit-server"
    if [[ ! -x "$bin" ]]; then
        echo "fetching livekit-server $version"
        mkdir -p "$(dirname "$bin")"
        curl -sSL "https://github.com/livekit/livekit/releases/download/v$version/livekit_${version}_linux_amd64.tar.gz" \
            | tar -xz -C "$(dirname "$bin")" livekit-server
    fi
fi

# --dev is the documented devkey/secret pair; the test mints its own tokens
# with it. Nothing here touches a real instance.
"$bin" --dev --bind 127.0.0.1 >/tmp/livekit-server.log 2>&1 &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT

for _ in $(seq 1 40); do
    if curl -sf -m 1 http://127.0.0.1:7880 >/dev/null 2>&1; then break; fi
    sleep 0.25
done

LIVEKIT_URL=ws://127.0.0.1:7880 \
LIVEKIT_API_KEY=devkey \
LIVEKIT_API_SECRET=secret \
    cargo test --features voice -- --ignored --nocapture a_real_room_hears_the_microphone

echo "voice: ok"
