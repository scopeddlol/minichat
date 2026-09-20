#!/usr/bin/env bash
# End-to-end check: a real server, a real sign-in, a real render.
#
# Builds nothing — it expects the server and client binaries to exist — then
# sets up an instance, seeds it, and has the native client sign in, connect
# the gateway and render what came back. This is the check that catches wire
# drift: every other test in this crate would pass happily against a server
# that changed the shape of READY.
#
#   native/tests/live.sh <path-to-minichat-server> <path-to-minichat-native>

set -euo pipefail

SERVER=${1:?usage: live.sh <minichat-server> <minichat-native>}
CLIENT=${2:?usage: live.sh <minichat-server> <minichat-native>}
PORT=${PORT:-8099}
ORIGIN="http://localhost:$PORT"
WORK=$(mktemp -d)
trap 'kill "${SERVER_PID:-}" 2>/dev/null || true; rm -rf "$WORK"' EXIT

mkdir -p "$WORK/web"
echo '<html></html>' > "$WORK/web/index.html"

MINICHAT_DATA_DIR="$WORK/data" \
BIND_ADDR="127.0.0.1:$PORT" \
JWT_SECRET="$(printf '%064d' 1)" \
PUBLIC_URL="$ORIGIN" \
WEB_DIR="$WORK/web" \
  "$SERVER" > "$WORK/server.log" 2>&1 &
SERVER_PID=$!

echo "waiting for the server"
for _ in $(seq 1 40); do
  if curl -fsS "$ORIGIN/healthz" > /dev/null 2>&1; then break; fi
  sleep 0.5
done
curl -fsS "$ORIGIN/healthz" > /dev/null

echo "setting the instance up"
TOKEN=$(curl -fsS -X POST "$ORIGIN/api/setup" -H 'Content-Type: application/json' -d '{
  "username":"ada","password":"correcthorsebattery","display_name":"Ada Lovelace",
  "instance_name":"Lovelace Works","tagline":"a small workshop",
  "accent_color":"#5b6ee8","registration_mode":"open","require_rules_accept":false,
  "default_role_name":"Member","default_permissions":4194347,
  "roles":[{"name":"Maintainer","color":"#34d399","permissions":1073741824,"hoist":true}],
  "channels":[
    {"name":"general","kind":"text","topic":"Anything and everything"},
    {"name":"design","kind":"text","category":"Text channels"},
    {"name":"releases","kind":"announcement","category":"Text channels"},
    {"name":"Lounge","kind":"voice","category":"Voice"}
  ]}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')

CHANNEL=$(curl -fsS "$ORIGIN/api/channels" -H "Authorization: Bearer $TOKEN" \
  | python3 -c 'import sys,json;print([c["id"] for c in json.load(sys.stdin) if c["name"]=="general"][0])')

curl -fsS -X POST "$ORIGIN/api/auth/register" -H 'Content-Type: application/json' \
  -d '{"username":"grace","password":"hopperhopperhopper","display_name":"Grace Hopper"}' > /dev/null

echo "seeding messages"
send() {
  curl -fsS -X POST "$ORIGIN/api/channels/$CHANNEL/messages" \
    -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
    -d "$(python3 -c 'import json,sys;print(json.dumps({"content":sys.argv[1]}))' "$1")" > /dev/null
}
# Every construction the message renderer has to handle.
send 'Plain text, **bold**, *italic*, __underline__, ~~strike~~ and `inline code`.'
send 'A link: https://example.com/a-fairly-long-url-that-should-be-elided-eventually'
send 'A spoiler: ||hidden|| and a mention of @grace.'
send '```rust
let laid = layout(&blocks, width, &measurer);
```'
send '> a quote
> spanning two lines'

echo "signing in from the native client"
OUT="$WORK/live.png"
REPORT=$("$CLIENT" --live "$ORIGIN" ada correcthorsebattery "$OUT")
echo "$REPORT"

fail() { echo "FAIL: $1" >&2; exit 1; }
grep -q 'Lovelace Works' <<< "$REPORT" || fail "the instance name did not come back"
grep -q 'Ada Lovelace'   <<< "$REPORT" || fail "the signed-in member did not come back"
grep -q 'admin panel: true' <<< "$REPORT" || fail "the operator's permissions did not parse"
grep -qE 'channels      4 in 2 categories' <<< "$REPORT" || fail "the channel list is wrong"
grep -qE '#general      [0-9]+ messages' <<< "$REPORT" || fail "no messages were loaded"

[ -f "$OUT" ] || fail "nothing was rendered"
SIZE=$(stat -c%s "$OUT")
[ "$SIZE" -gt 20000 ] || fail "the render looks empty ($SIZE bytes)"

echo "OK — signed in, gateway READY, ${SIZE} byte render"
