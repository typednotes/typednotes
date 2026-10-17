"""Hosted pairing through the real app/broker/C/Rust boundary, offline peers."""
import copy
import json
import time
import uuid


def verify_paired(api, exchange, sql, app, org, user, vault, device_root, runtime=None):
    original = api("/api/org/settings", {})["effect_policy"]
    policy = copy.deepcopy(original)
    policy["effects"] = list(set(policy.get("effects", [])) | {"Connector"})
    policy["providers"] = list(set(policy.get("providers", [])) | {"signal", "whatsapp"})
    api("/api/org/settings/permissions", {"policy": policy})
    project = api("/api/projects/create", {"project": "paired-check", "name": "Paired check"})
    other, token = str(uuid.uuid4()), "paired-other-member"
    sql(f"insert into users(id,email) values('{other}','paired-other@example.invalid');"
        f"insert into memberships(org_id,user_id,role) values('{org}','{other}','member');"
        f"insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{token}','UTF8')),'{other}',now()+interval '1 hour')")
    cases=[]
    for provider, name in (("Signal", "signal"), ("Whatsapp", "whatsapp")):
        state = api("/api/devices/pair", {"provider": provider})
        connection = state["connection"]
        credential = f"/v1/secret/data/thirdparty/{name}/{user}/{connection}"
        secret = vault.state["documents"][credential]["key"]
        assert len(secret) == 64 and secret not in json.dumps(state)
        deadline = time.monotonic() + 10
        while state["state"] != "connected":
            assert time.monotonic() < deadline, state
            state = api("/api/devices/status", {"connection": connection, "start": False})
            if state.get("qr_svg"):
                assert "<svg" in state["qr_svg"] and "offline-fixture-qr" not in state["qr_svg"]
            time.sleep(.05)
        assert state["identity"] == "sender" and state["qr_svg"] is None
        status, response = exchange(app + "/api/devices/status", {"slug":"native-fixture","connection":connection,"start":False}, {"Cookie":f"tn_session={token}"})
        assert status == 403 and secret not in json.dumps(response)
        channel = api("/api/channels/existing", {"project":"paired-check","connection":connection})
        assert channel["paired"]
        # Read one conversation only, leaving sending ungranted.
        peer = "00000000-0000-4000-8000-000000000099" if name == "signal" else "33611111111"
        permissions = {"scopes":[{"operation":"messages.read","root":["sender","alice"],"descendants":False}],
                       "maxRequestBytes":1048576,"maxResponseBytes":16777216}
        api("/api/connections/permissions", {"connection_id":connection,"permissions":permissions})
        inbox = api("/api/inbox", {"project":"paired-check"})
        messages = [m for m in inbox if m["channel_id"] == channel["id"]]
        assert [m["peer"] for m in messages] == ["alice"], messages
        api("/api/inbox/send", {"project":"paired-check","channel":channel["id"],"recipient":peer,"text":"hello"}, ok=False)
        permissions["scopes"].append({"operation":"messages.send","root":["sender",peer],"descendants":False})
        api("/api/connections/permissions", {"connection_id":connection,"permissions":permissions})
        sent = api("/api/inbox/send", {"project":"paired-check","channel":channel["id"],"recipient":peer,"text":"hello"})
        assert sent["peer"] == peer and sent["direction"] == "out"
        api("/api/inbox", {"project":"paired-check"})
        assert sql(f"select count(*) from channel_messages where channel_id='{channel['id']}' and direction='in'") == "1"
        assert sql(f"select count(*) from paired_inbox_cursors where channel_id='{channel['id']}'") == "1"
        cases.append((name,connection))
        print(f"PASS paired {name}: owner-only QR/status, exact conversation reads, explicit send grant and replay deduplication", flush=True)
    if runtime:
        from paired_runtime_cases import verify_paired_runtime
        verify_paired_runtime(cases,exchange=exchange,vault=vault,org=org,user=user,**runtime)
    for name,connection in cases:
        channel_id=sql(f"select id from channels where connection_id='{connection}'")
        credential=f"/v1/secret/data/thirdparty/{name}/{user}/{connection}"
        api("/api/devices/unlink", {"connection":connection})
        assert credential not in vault.state["documents"]
        assert not (device_root/name/org/user/connection).exists()
        assert sql(f"select count(*) from paired_inbox_cursors where channel_id='{channel_id}'") == "0"
        print(f"PASS paired {name}: remote/local unlink and cursor cleanup",flush=True)
    api("/api/org/settings/permissions", {"policy": original})
