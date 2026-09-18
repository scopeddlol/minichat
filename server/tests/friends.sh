#!/bin/bash
# End-to-end check on friends, favourites and blocks.
#
# Friendship is a pair of directed rows and favouriting is a separate
# one-sided table, so the interesting cases are all about two accounts
# disagreeing: who sees a request as outgoing vs incoming, whether a
# favourite survives an unfriend, and whether a block can be lifted by
# accident.
#
# Usage: start a server on a FRESH database, then
#   SCRATCH=server/tests bash server/tests/friends.sh
#
# Expects an un-setup instance on 127.0.0.1:8099. Every "should" in the
# output is the assertion.

set -e
API=http://127.0.0.1:8099/api
j() { python3 "$(dirname "$0")/jq.py" "$@"; }

OP=$(curl -s -X POST $API/setup -H 'content-type: application/json' -d '{
 "username":"operator","password":"correct horse battery staple","instance_name":"Friends",
 "channels":[{"name":"general","kind":"text"}]}' | j token)
INV=$(curl -s -X POST $API/invites -H "authorization: Bearer $OP" -H 'content-type: application/json' -d '{}' | j code)
A=$(curl -s -X POST $API/auth/register -H 'content-type: application/json' -d "{\"username\":\"ada\",\"password\":\"a good long passphrase\",\"invite\":\"$INV\",\"accept_rules\":true}" | j token)
INV2=$(curl -s -X POST $API/invites -H "authorization: Bearer $OP" -H 'content-type: application/json' -d '{}' | j code)
B=$(curl -s -X POST $API/auth/register -H 'content-type: application/json' -d "{\"username\":\"bo\",\"password\":\"another good passphrase\",\"invite\":\"$INV2\",\"accept_rules\":true}" | j token)
AID=$(curl -s $API/auth/me -H "authorization: Bearer $A" | j id)
BID=$(curl -s $API/auth/me -H "authorization: Bearer $B" | j id)
echo "ada=$AID bo=$BID"

show() { echo "  $1 sees: $(curl -s $API/relationships -H "authorization: Bearer $2")"; }

echo "=== ada requests bo ==="
curl -s -X POST $API/relationships -H "authorization: Bearer $A" -H 'content-type: application/json' -d "{\"user_id\":\"$BID\"}" -o /dev/null -w "  post %{http_code}\n"
show ada "$A"; show bo "$B"
echo "  (ada: outgoing, bo: incoming)"

echo "=== bo accepts by adding back ==="
curl -s -X POST $API/relationships -H "authorization: Bearer $B" -H 'content-type: application/json' -d "{\"user_id\":\"$AID\"}" -o /dev/null -w "  post %{http_code}\n"
show ada "$A"; show bo "$B"
echo "  (both: friend)"

echo "=== ada favourites bo (private, one-sided) ==="
curl -s -X PUT $API/favourites/$BID -H "authorization: Bearer $A" -o /dev/null -w "  put %{http_code}\n"
show ada "$A"; show bo "$B"
echo "  (only ada's entry has favourite=true)"

echo "=== ada unfriends bo ==="
curl -s -X DELETE $API/relationships/$BID -H "authorization: Bearer $A" -o /dev/null -w "  delete %{http_code}\n"
show ada "$A"; show bo "$B"
echo "  (ada keeps the favourite with kind=none; bo has nothing)"

echo "=== bo blocks ada, then ada cannot re-add ==="
curl -s -X POST $API/relationships/block -H "authorization: Bearer $B" -H 'content-type: application/json' -d "{\"user_id\":\"$AID\"}" -o /dev/null -w "  block %{http_code}\n"
curl -s -X POST $API/relationships -H "authorization: Bearer $A" -H 'content-type: application/json' -d "{\"user_id\":\"$BID\"}" -o /dev/null -w "  ada re-add: %{http_code} (should be 403)\n"
show bo "$B"

echo "=== unfriend must not lift a block ==="
curl -s -X DELETE $API/relationships/$AID -H "authorization: Bearer $B" -o /dev/null -w "  bo unfriends ada: %{http_code}\n"
show bo "$B"
echo "  (blocked must survive)"

echo "=== self-add is refused ==="
curl -s -X POST $API/relationships -H "authorization: Bearer $A" -H 'content-type: application/json' -d "{\"user_id\":\"$AID\"}" -o /dev/null -w "  %{http_code} (should be 400)\n"
