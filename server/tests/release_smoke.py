"""Exercise a real server on a disposable database; no production credentials."""
import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def run(binary):
    with tempfile.TemporaryDirectory(prefix="minichat-release-") as data:
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env = dict(os.environ, MINICHAT_DATA_DIR=data,
                   DATABASE_URL=f"sqlite://{data}/minichat.db?mode=rwc",
                   BIND_ADDR=f"127.0.0.1:{port}", JWT_SECRET=secrets.token_hex(32),
                   PUBLIC_URL=f"http://127.0.0.1:{port}", SETUP_TOKEN="",
                   LIVEKIT_URL="ws://127.0.0.1:7880", LIVEKIT_API_KEY="test",
                   LIVEKIT_API_SECRET=secrets.token_hex(32))
        with open(os.path.join(data, "server.log"), "w+") as log:
            process = subprocess.Popen([os.path.abspath(binary)], env=env, stdout=log, stderr=log)
            try:
                def api(method, path, body=None, token=None, status=200):
                    headers = {"Content-Type": "application/json"}
                    if token:
                        headers["Authorization"] = "Bearer " + token
                    request = urllib.request.Request(f"http://127.0.0.1:{port}/api{path}",
                        data=json.dumps(body).encode() if body is not None else None,
                        headers=headers, method=method)
                    try:
                        response = urllib.request.urlopen(request, timeout=10)
                    except urllib.error.HTTPError as error:
                        response = error
                    with response:
                        result = json.loads(response.read())
                        assert response.status == status, (method, path, response.status, result)
                        return result

                for _ in range(100):
                    try:
                        api("GET", "/meta")
                        break
                    except urllib.error.URLError:
                        if process.poll() is not None:
                            raise RuntimeError("Server exited before ready")
                        time.sleep(.1)
                else:
                    raise RuntimeError("Server did not become ready")

                operator = api("POST", "/setup", {"username": "operator", "password": "a long test password",
                    "instance_name": "Release test", "channels": [{"name": "General Chat", "kind": "text"}]})["token"]
                def member(name):
                    invite = api("POST", "/invites", {}, operator)["code"]
                    token = api("POST", "/auth/register", {"username": name, "password": "another long test password", "invite": invite, "accept_rules": True})["token"]
                    return token, api("GET", "/auth/me", token=token)["id"]
                a, aid = member("alice")
                b, bid = member("bobby")
                api("GET", "/direct", status=401)
                conversation = api("POST", f"/direct/open/{bid}", token=a)["id"]
                assert api("POST", f"/direct/open/{aid}", token=b)["id"] == conversation
                message = api("POST", f"/direct/{conversation}/messages", {"content": "hello privately"}, a)
                assert len(api("GET", f"/direct/{conversation}/messages", token=b)) == 1
                api("GET", f"/direct/{conversation}/messages", token=operator, status=404)
                assert api("GET", "/direct", token=operator) == []
                assert api("GET", "/direct", token=b)[0]["unread"] == 1
                api("POST", f"/direct/{conversation}/ack", {"message_id": message["id"]}, b)
                assert api("GET", "/direct", token=b)[0]["unread"] == 0
                call = api("POST", f"/direct/{conversation}/call", token=a)["id"]
                api("POST", f"/direct/calls/{call}/token", token=a, status=404)
                api("POST", f"/direct/calls/{call}", {"action": "accept"}, a, status=400)
                api("POST", f"/direct/calls/{call}", {"action": "accept"}, b)
                assert api("POST", f"/direct/calls/{call}/token", token=a)["room"] == "direct-" + call
                api("POST", f"/direct/calls/{call}/token", token=operator, status=404)
                api("POST", "/relationships/block", {"user_id": aid}, b)
                assert api("GET", "/direct/calls", token=a) == []
                api("POST", f"/direct/{conversation}/messages", {"content": "blocked"}, a, status=403)
                api("POST", f"/direct/{conversation}/call", token=a, status=403)
                api("POST", "/relationships/block", {"user_id": bid}, a)
                api("DELETE", f"/relationships/block/{bid}", token=a)
                # A cannot undo B's block by blocking and then unblocking B.
                api("POST", f"/direct/open/{bid}", token=a, status=403)
                api("DELETE", f"/relationships/block/{aid}", token=b)
                api("POST", f"/direct/{conversation}/messages", {"content": "unblocked"}, a)

                role = api("POST", "/admin/roles", {"name": "Staff", "permissions": 0}, operator)["id"]
                category = api("POST", "/categories", {"name": "Private category", "is_private": True, "allowed_role_ids": [role]}, operator)["id"]
                channel = api("POST", "/channels", {"name": "Staff Room", "category_id": category}, operator)["id"]
                assert category not in [c["id"] for c in api("GET", "/categories", token=a)]
                assert channel not in [c["id"] for c in api("GET", "/channels", token=a)]
                api("PATCH", f"/categories/{category}", {"name": "Renamed private category"}, operator)
                assert channel not in [c["id"] for c in api("GET", "/channels", token=a)]
                api("PUT", f"/admin/members/{aid}/roles/{role}", token=operator)
                assert category in [c["id"] for c in api("GET", "/categories", token=a)]
                assert channel in [c["id"] for c in api("GET", "/channels", token=a)]
                api("DELETE", f"/admin/members/{aid}/roles/{role}", token=operator)
                assert channel not in [c["id"] for c in api("GET", "/channels", token=a)]
                public = api("POST", "/categories", {"name": "Public category"}, operator)["id"]
                order = {"channels": [{"id": channel, "position": 0, "category_id": public}]}
                api("POST", "/channels/reorder", order, a, status=403)
                api("POST", "/channels/reorder", order, operator)
                assert channel in [c["id"] for c in api("GET", "/channels", token=a)]
                order["channels"][0]["category_id"] = category
                api("POST", "/channels/reorder", order, operator)
                assert channel not in [c["id"] for c in api("GET", "/channels", token=a)]
                # A later invalid entry rolls back the earlier move too.
                api("POST", "/channels/reorder", {"channels": [
                    {"id": channel, "position": 0, "category_id": public},
                    {"id": "missing-channel", "position": 1, "category_id": public},
                ]}, operator, status=404)
                assert channel not in [c["id"] for c in api("GET", "/channels", token=a)]
                api("POST", "/channels/reorder", {"channels": [
                    {"id": channel, "position": 0, "category_id": "missing-category"},
                ]}, operator, status=400)
                print("PASS: authenticated DM privacy, unread state, call consent, blocking, category inheritance, and atomic channel reordering")
            except Exception:
                log.flush()
                log.seek(0)
                print(log.read(), file=sys.stderr)
                raise
            finally:
                process.terminate()
                process.wait(timeout=15)


if __name__ == "__main__":
    run(sys.argv[1])
