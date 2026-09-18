#!/bin/bash
# Check that upgrading an existing database changes nothing it shouldn't.
#
# Every other test here starts from a fresh database, which is the one path a
# self-hoster never takes. The v0.4 migration adds `channels.sync_category`
# and `categories.is_private`, both defaulting to 0 precisely so an upgrade
# cannot rewrite a channel someone configured by hand — and that default is
# only worth anything if it is actually checked.
#
# Usage, from a checkout of the OLD version with its server built:
#
#   # 1. old binary, new data dir
#   MINICHAT_DATA_DIR=/tmp/up ./target/debug/minichat-server &
#   #    ...seed it: a private channel with hand-set overwrites...
#   # 2. kill it, then run the NEW binary on the same MINICHAT_DATA_DIR
#   # 3. run this script against the upgraded instance
#
# Expects the upgraded server on 127.0.0.1:8097 and the database path as $1.
set -e
API=http://127.0.0.1:8097/api
DB="${1:?pass the path to minichat.db}"

echo "=== migrations applied ==="
python3 - "$DB" <<'PY'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
for version, description, success in c.execute(
    'select version, description, success from _sqlx_migrations order by version'
):
    print(f'  {version} {description} success={bool(success)}')
PY

echo "=== nothing was silently made private or synced ==="
python3 - "$DB" <<'PY'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
synced = [r[0] for r in c.execute('select name from channels where sync_category = 1')]
private = [r[0] for r in c.execute('select name from categories where is_private = 1')]
cat_ow = next(c.execute('select count(*) from category_overwrites'))[0]
ch_ow = next(c.execute('select count(*) from channel_overwrites'))[0]
frames = list(c.execute('select avatar_x, avatar_y, avatar_zoom from users'))
print(f'  channels synced to a category: {synced}   (must be [])')
print(f'  private categories: {private}             (must be [])')
print(f'  category_overwrites rows: {cat_ow}        (must be 0)')
print(f'  channel_overwrites rows kept: {ch_ow}     (must match what you set)')
print(f'  image frames default to centred: {all(f == (50.0, 50.0, 1.0) for f in frames)}')
ok = not synced and not private and cat_ow == 0 and all(f == (50.0, 50.0, 1.0) for f in frames)
print('  PASS' if ok else '  FAIL')
PY
