#!/bin/bash
# End-to-end check on who can see which channel.
#
# Channel visibility is resolved in four separate places on the server
# (channel_permissions, channel_permission_map, visible_channel_ids and
# channel_viewers), so the only honest test is over HTTP against a running
# instance with two real accounts: an operator and a plain member.
#
# Usage: start a server on a FRESH database, then
#   MINICHAT_DATA_DIR=... ./target/debug/minichat-server &
#   bash server/tests/channel-permissions.sh
#
# Expects an un-setup instance on 127.0.0.1:8099 and prints what the member can
# see at each step; every "should" in the output is the assertion.

set -e
API=http://127.0.0.1:8099/api
j() { python3 "$(dirname "$0")/jq.py" "$@"; }

# Operator + a plain member, so privacy can be checked from a non-admin seat.
OP=$(curl -s -X POST $API/setup -H 'content-type: application/json' -d '{
 "username":"operator","password":"correct horse battery staple","instance_name":"Perms",
 "channels":[{"name":"General Chat","kind":"text","category":"Text"}]}' | j token)
echo "operator token: ${#OP} chars"

# Name with a space and capitals survived?
curl -s $API/channels -H "authorization: Bearer $OP" | python3 -c "
import sys,json
for c in json.load(sys.stdin): print('  channel:', repr(c['name']), 'sync:', c['sync_category'])
"

INV=$(curl -s -X POST $API/invites -H "authorization: Bearer $OP" -H 'content-type: application/json' -d '{}' | j code)
MEM=$(curl -s -X POST $API/auth/register -H 'content-type: application/json' -d "{\"username\":\"member\",\"password\":\"another good passphrase\",\"invite\":\"$INV\",\"accept_rules\":true}" | j token)
echo "member token: ${#MEM} chars"

ROLES=$(curl -s $API/admin/roles -H "authorization: Bearer $OP")
DEFAULT=$(echo "$ROLES" | j 0 id)
echo "roles: $(echo "$ROLES" | python3 -c "import sys,json;print([(r['name'],r['id']) for r in json.load(sys.stdin)])")"

# A second role, to be the one allowed into the private channel.
STAFF=$(curl -s -X POST $API/admin/roles -H "authorization: Bearer $OP" -H 'content-type: application/json' -d '{"name":"Staff","permissions":0}' | j id)
echo "staff role: $STAFF"

echo
echo "=== private channel admitting only Staff ==="
PRIV=$(curl -s -X POST $API/channels -H "authorization: Bearer $OP" -H 'content-type: application/json' -d "{\"name\":\"Staff Room\",\"kind\":\"text\",\"is_private\":true,\"allowed_role_ids\":[\"$STAFF\"]}" | j id)
echo "created: $PRIV"
echo "member sees:   $(curl -s $API/channels -H "authorization: Bearer $MEM" | python3 -c "import sys,json;print([c['name'] for c in json.load(sys.stdin)])")"
echo "allowed roles: $(curl -s $API/access/$PRIV/roles -H "authorization: Bearer $OP")"

echo
echo "=== give the member Staff; they should now see it ==="
MEMID=$(curl -s $API/auth/me -H "authorization: Bearer $MEM" | j id)
curl -s -X PUT $API/admin/members/$MEMID/roles/$STAFF -H "authorization: Bearer $OP" -o /dev/null -w "  add role: %{http_code}\n"
echo "member sees:   $(curl -s $API/channels -H "authorization: Bearer $MEM" | python3 -c "import sys,json;print([c['name'] for c in json.load(sys.stdin)])")"

echo
echo "=== private CATEGORY admitting only Staff, with a synced channel ==="
CAT=$(curl -s -X POST $API/categories -H "authorization: Bearer $OP" -H 'content-type: application/json' -d "{\"name\":\"Staff Area\",\"is_private\":true,\"allowed_role_ids\":[\"$STAFF\"]}" | j id)
SYNCED=$(curl -s -X POST $API/channels -H "authorization: Bearer $OP" -H 'content-type: application/json' -d "{\"name\":\"Planning Notes\",\"kind\":\"text\",\"category_id\":\"$CAT\"}" | j id)
echo "category $CAT, synced channel $SYNCED"
curl -s $API/channels -H "authorization: Bearer $OP" | python3 -c "
import sys,json
for c in json.load(sys.stdin):
    if c['id']=='$SYNCED': print('  sync_category =', c['sync_category'])
"
echo "member (has Staff) sees: $(curl -s $API/channels -H "authorization: Bearer $MEM" | python3 -c "import sys,json;print([c['name'] for c in json.load(sys.stdin)])")"

echo
echo "=== drop Staff from the category; the synced channel must follow ==="
curl -s -X PATCH $API/categories/$CAT -H "authorization: Bearer $OP" -H 'content-type: application/json' -d '{"name":"Staff Area","is_private":true,"allowed_role_ids":[]}' -o /dev/null -w "  patch: %{http_code}\n"
echo "member sees: $(curl -s $API/channels -H "authorization: Bearer $MEM" | python3 -c "import sys,json;print([c['name'] for c in json.load(sys.stdin)])")"
echo "  (Planning Notes should be gone; Staff Room should remain)"

echo
echo "=== unsync the channel, then re-open the category: channel stays shut ==="
curl -s -X PATCH $API/channels/$SYNCED -H "authorization: Bearer $OP" -H 'content-type: application/json' -d '{"sync_category":false}' -o /dev/null -w "  unsync: %{http_code}\n"
curl -s -X PATCH $API/categories/$CAT -H "authorization: Bearer $OP" -H 'content-type: application/json' -d "{\"name\":\"Staff Area\",\"is_private\":true,\"allowed_role_ids\":[\"$STAFF\"]}" -o /dev/null -w "  reopen category: %{http_code}\n"
echo "member sees: $(curl -s $API/channels -H "authorization: Bearer $MEM" | python3 -c "import sys,json;print([c['name'] for c in json.load(sys.stdin)])")"
echo "  (Planning Notes should still be hidden - it no longer inherits)"
