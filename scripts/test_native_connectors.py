#!/usr/bin/env python3
"""Real app HTTP + broker + disposable PostgreSQL, with write-only vault fixtures.

No paid provider, real credential, existing database, or infra checkout is used.
Build `cargo build -p web --features server` and the local override broker first.
"""
import argparse
import copy
import hashlib
import hmac
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import struct
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from typing import Any, cast
import uuid

ROOT = Path(__file__).resolve().parents[1]


def port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def exchange(url, value=None, headers=None, method=None) -> tuple[int, Any]:
    request = Request(url, None if value is None else json.dumps(value).encode(),
                      {"Content-Type": "application/json", **(headers or {})}, method=method)
    try:
        with urlopen(request, timeout=30) as response:
            raw = response.read()
            return response.status, json.loads(raw) if raw else None
    except HTTPError as response:
        raw = response.read().decode()
        try:
            return response.code, json.loads(raw)
        except ValueError:
            return response.code, raw


class FixtureServer(ThreadingHTTPServer):
    state: dict[str, Any]


class Fixture(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        pass

    def reply(self, status, value, extra=None):
        body = json.dumps(value).encode()
        self.reply_bytes(status, body, extra)

    def reply_bytes(self, status, body, extra=None):
        self.send_response(status)
        self.send_header("Content-Type", (extra or {}).get("Content-Type", "application/json"))
        self.send_header("Content-Length", str(len(body)))
        for key, value in (extra or {}).items():
            if key != "Content-Type": self.send_header(key, value)
        self.end_headers()
        self.wfile.write(body)

    def handle_request(self):
        state = cast(FixtureServer, self.server).state
        raw = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if state["service"] == "writer":
            assert self.headers.get("Authorization") == "Bearer fixture-writer"
            body = json.loads(raw) if raw else None
            state["calls"].append((self.command, self.path, body))
            def public_execution(execution):
                fields = ["provider", "connection", "account", "bucket", "organization", "connectionPermissions", "cell", "warrantPermissions"]
                return {"execution": {"policy": execution["policy"], "binding": execution["binding"],
                    "connectors": {name: [{key: grant[key] for key in fields if key in grant} for grant in grants]
                                   for name, grants in execution["connectors"].items()}},
                    "functions": execution["functions"], "graphs": execution["graphs"]}
            if self.path == "/v0/sessions" and self.command == "POST":
                assert isinstance(body, dict)
                state["tools"] = body["tools"]
                state["execution"] = public_execution(body["execution"])
                return self.reply(201, {"id": "writer-fixture", "state": "running", "tools": state["tools"], "execution": state["execution"]})
            if "/messages" in self.path and self.command == "POST":
                assert isinstance(body, dict)
                assert set(body.get("tools", [])) <= set(state["tools"])
                state["tools"] = body.get("tools", state["tools"])
                if "execution" in body: state["execution"] = public_execution(body["execution"])
                return self.reply(202, {})
            if "/credentials" in self.path and self.command == "PUT":
                assert isinstance(body, dict)
                assert "tools" not in body
                return self.reply(200, {})
            if "/messages" in self.path:
                return self.reply(200, {"entries": [], "next": 0, "running": True})
            return self.reply(200, {"state": "running", "entries": 0, "tools": state.get("tools", []), "execution": state.get("execution")})
        if state["service"] in ["runtime-proxy", "writer-proxy"]:
            token = "fixture-lun" if state["service"] == "runtime-proxy" else "fixture-writer"
            assert self.headers.get("Authorization") == "Bearer " + token
            value = json.loads(raw) if raw else None
            state["calls"].append((self.command, self.path, value))
            status, response = exchange(state["target"] + self.path, value, {"Authorization": "Bearer " + token}, method=self.command)
            return self.reply(status, response)
        if state["service"] == "upstream" and state.get("pipeline"):
            return state["pipeline"].handle(self, raw)
        if state["service"] == "vault":
            if self.path == "/v1/auth/userpass/login":
                return self.reply(200, {"auth": {"client_token": "app-write-only", "lease_duration": 3600}})
            token = self.headers.get("Authorization")
            if self.command == "GET":
                state["reads"].append((token, self.path))
                if token not in ["Bearer broker-read-fixture", "Bearer runtime-read-fixture"]:
                    return self.reply(403, {"error": "write-only"})
                if self.path not in state["documents"]:
                    return self.reply(404, {})
                return self.reply(200, {"data": state["documents"][self.path]})
            if token == "Bearer runtime-read-fixture" and self.command == "POST" and self.path.startswith("/v1/secret/data/graph/"):
                state["documents"][self.path] = json.loads(raw)
                return self.reply(200, {})
            if token != "Bearer app-write-only":
                return self.reply(403, {})
            if state.get("fail_prefix") and self.path.startswith(state["fail_prefix"]):
                return self.reply(503, {})
            if self.command == "DELETE":
                state["documents"].pop(self.path, None)
            else:
                document = json.loads(raw)
                # Test gateways stay entirely local. The app itself still writes
                # the validated HTTPS base and never reads a credential back.
                if "base_url" in document and document.get("kind") != "postgres":
                    document["base_url"] = state["upstream"] + "/base"
                state["documents"][self.path] = document
            return self.reply(200, {})
        if self.headers.get("Authorization", "").startswith("AWS4-HMAC-SHA256"):
            import importlib.util
            integration = ROOT.parent / "liaison/LiaisonTest/integration"
            if str(integration) not in sys.path:
                sys.path.insert(0, str(integration))
            sys.dont_write_bytecode = True
            spec = importlib.util.spec_from_file_location("broker_signature_fixture", ROOT.parent / "liaison/LiaisonTest/integration/connectors.py")
            assert spec is not None and spec.loader is not None
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            module.verify_sigv4(self.command, self.path, self.headers, raw)
            state["calls"].append((self.command, self.path, None))
            return self.reply(200, {"native": True})
        assert self.headers.get("Authorization") == "Bearer local-provider-fixture"
        state["calls"].append((self.command, self.path, json.loads(raw) if raw else None))
        if self.path.endswith("/models"):
            return self.reply(200, state.get("models", [{"id": "jev-latest"}]))
        if self.path.endswith("/systemone"):
            payload = json.loads(raw)
            assert payload["model"] == "jev-latest"
            assert "url" not in payload and "headers" not in payload
            return self.reply(200, {"ready": True})
        if self.path.endswith("/chat/completions"):
            return self.reply(200, {"choices": [{"message": {"content": "hello"}, "finish_reason": "stop"}],
                                    "usage": {"prompt_tokens": 1, "completion_tokens": 1}})
        if self.path.endswith("/messages"):
            assert self.headers.get("Accept") == "text/event-stream"
            events = [{"type": "start"}, {"type": "text_start", "contentIndex": 0},
                {"type": "text_end", "contentIndex": 0, "content": "hello"}, {"type": "done", "reason": "stop", "usage": {"input": 1, "output": 1}}]
            return self.reply_bytes(200, "".join("data: " + json.dumps(event) + "\n\n" for event in events).encode(), {"Content-Type": "text/event-stream"})
        if self.path.endswith("/user/repos"):
            return self.reply(200, [{"full_name": "fixture/repo", "html_url": "https://github.com/fixture/repo", "default_branch": "main", "private": True}])
        return self.reply(200, {"ok": True})

    do_GET = do_POST = do_DELETE = do_PUT = handle_request


def serve(service):
    server = FixtureServer(("127.0.0.1", 0), Fixture)
    server.state = {"service": service, "documents": {}, "reads": [], "calls": []}
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--temp-root", type=Path, required=True)
    parser.add_argument("--pg-bin", type=Path, default=Path("/opt/homebrew/opt/postgresql@16/bin"))
    parser.add_argument("--broker", type=Path, default=ROOT.parent / "liaison/.lake/build/bin/liaison")
    parser.add_argument("--app", type=Path, default=ROOT / "target/debug/web")
    parser.add_argument("--lean-workspace", type=Path, help="Local Lake override workspace; also exercise the real Lean SDK/writer")
    parser.add_argument("--runtime", action="store_true", help="Exercise app-provisioned local grants through a compiled Lun driver")
    parser.add_argument("--writer", action="store_true", help="Verify actual app writer launch/narrowing/PUT request shapes")
    parser.add_argument("--real-writer", action="store_true", help="Actual app/broker/Lode checkout, generation, publish and Lun adoption")
    args = parser.parse_args()
    if args.writer and args.real_writer:
        parser.error("--writer shape peer and --real-writer actual Lode are separate fixture modes")
    assert args.temp_root.is_dir() and args.broker.is_file() and args.app.is_file()
    vault, upstream = serve("vault"), serve("upstream")
    proxy = serve("runtime-proxy") if args.runtime else None
    writer = serve("writer") if args.writer else serve("writer-proxy") if args.real_writer else None
    vault.state["upstream"] = f"http://127.0.0.1:{upstream.server_port}"
    pg_port, app_port, broker_port = port(), port(), port()
    runtime_port = port()
    lode_port = port()
    user, org = str(uuid.uuid4()), str(uuid.uuid4())
    cookie = "native-app-session-fixture"
    checks = []
    processes = []
    with tempfile.TemporaryDirectory(prefix="native-app-", dir=args.temp_root) as tmp:
        scratch = Path(tmp)
        database = f"postgresql://fixture@127.0.0.1:{pg_port}/postgres?sslmode=disable"

        def command(*argv, **kwargs):
            result = subprocess.run(argv, capture_output=True, text=True, **kwargs)
            assert result.returncode == 0, (argv[0], result.stdout, result.stderr)
            return result.stdout.strip()

        def sql(statement):
            return command(str(args.pg_bin / "psql"), database, "-X", "-v", "ON_ERROR_STOP=1", "-At", "-c", statement)

        command(str(args.pg_bin / "initdb"), "-D", str(scratch / "pg"), "-A", "trust", "--no-locale", "-U", "fixture")
        if args.runtime:
            hba = scratch / "pg/pg_hba.conf"
            hba.write_text("host all fixture 127.0.0.1/32 trust\nhost all all 127.0.0.1/32 scram-sha-256\n" + hba.read_text())
        command(str(args.pg_bin / "pg_ctl"), "-D", str(scratch / "pg"), "-l", str(scratch / "pg.log"),
                "-o", f"-h 127.0.0.1 -p {pg_port} -k ''", "-w", "start")
        try:
            for migration in sorted((ROOT / "migrations").glob("*.sql")):
                sql(migration.read_text())
            sql("create table credit_holds (id uuid primary key default gen_random_uuid(), org_id uuid, run_id uuid, amount bigint, state text, expires_at timestamptz);"
                "create table credit_ledger (id uuid default gen_random_uuid(), org_id uuid, run_id uuid, delta bigint, reason text);"
                + (ROOT.parent / "liaison/sql/0001_audit_log.sql").read_text())
            sql(f"insert into users(id,email) values('{user}','native@example.invalid');"
                f"insert into orgs(id,slug,name) values('{org}','native-fixture','Native fixture');"
                f"insert into memberships(user_id,org_id,role) values('{user}','{org}','owner');"
                f"insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{cookie}','UTF8')),'{user}',now()+interval '1 hour');")
            sql(f"insert into credit_ledger(org_id,run_id,delta,reason) values('{org}','{uuid.uuid4()}',100,'fixture')")
            key = secrets.token_hex(32)
            env = {**os.environ, "DATABASE_URL": database, "LIAISON_ROOT_KEY": key,
                   "SECRETS_HOST": "127.0.0.1", "SECRETS_PORT": str(vault.server_port),
                   "SECRETS_INSECURE": "1", "SECRETS_TOKEN": "broker-read-fixture", "LIAISON_PORT": str(broker_port)}
            for name in ["SECRETS_USERNAME", "SECRETS_PASSWORD", "COMPUTE_DB_URL", "LODE_URL", "LUN_URL"]:
                env.pop(name, None)
            app = f"http://127.0.0.1:{app_port}"
            broker = f"http://127.0.0.1:{broker_port}"
            with (scratch / "broker.log").open("w") as broker_log, (scratch / "app.log").open("w") as app_log:
                processes.append(subprocess.Popen([str(args.broker)], env=env, stdout=broker_log, stderr=broker_log))
                app_env = {**env, "IP": "127.0.0.1", "PORT": str(app_port), "PUBLIC_URL": app,
                           "SECRETS_URL": f"http://127.0.0.1:{vault.server_port}", "SECRETS_PASSWORD": "fixture",
                           "LIAISON_URL": broker, "TYPEDNOTES_MODEL_CALL_COST": "1", "TYPEDNOTES_AUTO_REPAIRS": "0"}
                if writer:
                    app_env.update(LODE_URL=f"http://127.0.0.1:{writer.server_port}", LODE_TOKEN="fixture-writer")
                runtime = f"http://127.0.0.1:{runtime_port}"
                runtime_log = None
                if args.runtime or args.real_writer:
                    if proxy is None: proxy = serve("runtime-proxy")
                    runtime_log = (scratch / "runtime.log").open("w")
                    runtime_env = {**env, "LUN_PORT": str(runtime_port), "LUN_TOKEN": "fixture-lun",
                        "LUN_WORKDIR": str(scratch / "lun"), "LUN_ALLOW_LOCAL": "1", "LUN_BUILD_TIMEOUT": "240",
                        "LUN_LIAISON_URL": broker, "LUN_LIAISON_SDK_PATH": str(ROOT.parent / "liaison"),
                        "SECRETS_TOKEN": "runtime-read-fixture", "LUN_TEMP_ROOT": str(scratch / "temporary"), "LEAN_NUM_THREADS": "2"}
                    if args.real_writer and args.runtime:
                        from lode_runtime_bridge_cases import http_environment
                        runtime_env.update(http_environment(scratch))
                    processes.append(subprocess.Popen([str(ROOT.parent / "lun/.lake/build/bin/lun")], env=runtime_env, stdout=runtime_log, stderr=runtime_log))
                    proxy.state["target"] = runtime
                    app_env.update(LUN_URL=f"http://127.0.0.1:{proxy.server_port}", LUN_TOKEN="fixture-lun", COMPUTE_DB_URL=database)
                lode = f"http://127.0.0.1:{lode_port}"
                lode_env = {}
                if args.real_writer:
                    assert writer is not None
                    lode_log = (scratch / "lode.log").open("w")
                    lode_env = {**env, "LODE_PORT": str(lode_port), "LODE_TOKEN": "fixture-writer", "LODE_WORKDIR": str(scratch / "lode"),
                        "LODE_ALLOW_LOCAL": "1", "LODE_LIAISON_URL": broker, "LODE_MODEL_TIMEOUT": "30", "LODE_CHECK_TIMEOUT": "240", "LEAN_NUM_THREADS": "2",
                        "LODE_LUN_URL": runtime, "LODE_LUN_TOKEN": "fixture-lun"}
                    for name in ["LIAISON_ROOT_KEY", "DATABASE_URL", "COMPUTE_DB_URL", "SECRETS_TOKEN", "SECRETS_USERNAME", "SECRETS_PASSWORD"]:
                        lode_env.pop(name, None)
                    processes.append(subprocess.Popen([str(ROOT.parent / "lode/.lake/build/bin/lode")], env=lode_env, stdout=lode_log, stderr=lode_log))
                    writer.state["target"] = lode
                    app_env.update(LODE_URL=f"http://127.0.0.1:{writer.server_port}", LODE_TOKEN="fixture-writer")
                processes.append(subprocess.Popen([str(args.app)], cwd=ROOT, env=app_env, stdout=app_log, stderr=app_log))
                health = [(broker, "/_health"), (app, "/api/health")]
                if args.runtime or args.real_writer: health.append((runtime, "/_health"))
                if args.real_writer: health.append((lode, "/_health"))
                for origin, path in health:
                    for _ in range(200):
                        try:
                            with urlopen(origin + path, timeout=1):
                                break
                        except (OSError, URLError):
                            if any(p.poll() is not None for p in processes):
                                raise AssertionError((scratch / "broker.log").read_text() + (scratch / "app.log").read_text())
                            time.sleep(.05)
                    else:
                        raise AssertionError("fixture service failed to start")

                def api(path, fields, ok=True):
                    status, value = exchange(app + path, {"slug": "native-fixture", **fields}, {"Cookie": f"tn_session={cookie}"})
                    assert (200 <= status < 300) == ok, (path, status, value)
                    return value

                def passed(label):
                    checks.append(label)
                    print("PASS:", label, flush=True)

                connection = api("/api/connections/ai", {"provider": "TypeSafe", "base_url": "https://fixture.example.invalid/base", "api_key": "local-provider-fixture"})
                connection_id = connection["id"]
                credential_path = f"/v1/secret/data/thirdparty/typesafe/{user}/{connection_id}"
                org_path = f"/v1/secret/data/connector-policy/{org}/typesafe/{connection_id}"
                assert credential_path in vault.state["documents"]
                assert vault.state["documents"][credential_path + "/permissions"] == connection["permissions"]
                assert org_path in vault.state["documents"]
                assert "local-provider-fixture" not in json.dumps(connection)
                passed("connection becomes active only after credential and both live ceilings exist; no key in DTO")
                result = api("/api/connections/test", {"id": connection_id})
                assert result["ok"], result
                passed("actual app probe uses models.list through real broker/HMAC/Postgres")
                assert api("/api/ai/models", {"connection_id": connection_id}) == ["jev-latest"]
                baseten = api("/api/connections/ai", {"provider": "Baseten", "base_url": "https://fixture.example.invalid/base", "api_key": "local-provider-fixture"})
                upstream.state["models"] = {"data": [{"id": "z-model"}, {"id": "a-model"}, {"id": "a-model"}]}
                assert api("/api/ai/models", {"connection_id": baseten["id"]}) == ["a-model", "z-model"]
                assert api("/api/connections/test", {"id": baseten["id"]})["ok"]
                upstream.state.pop("models")
                github_id = str(uuid.uuid4())
                sql(f"insert into connections(id,org_id,user_id,provider,label,base_url,status) values('{github_id}','{org}','{user}','github','GitHub fixture','https://api.github.com','active')")
                vault.state["documents"][f"/v1/secret/data/thirdparty/github/{user}/{github_id}"] = {"kind": "bearer", "base_url": vault.state["upstream"] + "/base", "token": "local-provider-fixture"}
                assert api("/api/repos", {"connection": github_id})[0]["full_name"] == "fixture/repo"
                original_policy = api("/api/org/settings", {})["effect_policy"]
                blocked_policy = {**original_policy, "effects": ["SecretStore"]}
                api("/api/org/settings/permissions", {"policy": blocked_policy})
                before_calls, before_reads = len(upstream.state["calls"]), len(vault.state["reads"])
                for id in [connection_id, baseten["id"]]:
                    error = api("/api/ai/models", {"connection_id": id}, ok=False)
                    assert "Effects" in json.dumps(error), error
                assert "Effects" in json.dumps(api("/api/repos", {"connection": github_id}, ok=False))
                assert len(upstream.state["calls"]) == before_calls and len(vault.state["reads"]) == before_reads
                api("/api/org/settings/permissions", {"policy": original_policy})
                assert api("/api/ai/models", {"connection_id": connection_id}) == ["jev-latest"]
                assert api("/api/repos", {"connection": github_id})[0]["full_name"] == "fixture/repo"
                api("/api/connections/delete", {"id": github_id})
                api("/api/connections/delete", {"id": baseten["id"]})
                passed("provider model menus and Baseten tests use real broker; SecretStore-only policy denies before credentials, explicit admin correction restores calls")
                permissions = {"scopes": [{"operation": "models.list", "root": [], "descendants": False},
                                          {"operation": "classification.evaluate", "root": ["jev-latest"], "descendants": False}],
                               "maxRequestBytes": 4096, "maxResponseBytes": 4096}
                api("/api/connections/permissions", {"connection_id": connection_id, "permissions": permissions})
                classify = {"connection_id": connection_id, "request": {"model": "jev-latest", "state": {}, "questions": {"ready": "bool"}}}
                assert api("/api/ai/classify", classify) == {"ready": True}
                assert upstream.state["calls"][-1][1] == "/base/systemone"
                row = json.loads(sql("select row_to_json(c) from connector_authorities c order by expires_at desc limit 1"))
                run_path = f"/v1/secret/data/connector-authority/{org}/{row['run_id']}/{row['warrant_id']}"
                projection = vault.state["documents"][run_path]
                assert projection["account"] == f"{user}/{connection_id}"
                assert projection["cell"]["scopes"] == permissions["scopes"]
                assert projection["warrant"] == projection["cell"]
                passed("native classification consumes independently provisioned org/connection/cell/warrant ceilings")
                before = len(upstream.state["calls"])
                wrong = copy.deepcopy(classify)
                wrong["request"]["model"] = "another-model"
                api("/api/ai/classify", wrong, ok=False)
                wrong = copy.deepcopy(classify)
                wrong["request"]["headers"] = {"authorization": "override"}
                api("/api/ai/classify", wrong, ok=False)
                assert len(upstream.state["calls"]) == before
                passed("wrong resource and caller header override deny without provider egress")
                # Exercise the real app's graph grant path without claiming a
                # runtime simulation as effect-enforcement evidence. The final
                # runtime call deliberately fails because no runtime is configured.
                project, graph, source, cell = [str(uuid.uuid4()) for _ in range(4)]
                declared = {**permissions, "scopes": permissions["scopes"][1:]}
                cell_config = {"dependencies": ["input"], "output_type": "Bool", "connectors": [
                    {"connection": connection_id, "permissions": declared}]}
                structure = {"inputs": ["x"], "nodes": [{"id": 0, "input": "x"}, {"id": 1, "function": "compute", "args": [0]}], "sources": [0], "sinks": [1]}
                sql(f"insert into projects(id,org_id,slug,name,created_by) values('{project}','{org}','native-project','Native project','{user}');"
                    f"insert into graphs(id,project_id,slug,name,created_by,status,lun_session_id,session_build_id,structure) "
                    f"values('{graph}','{project}','native-graph','Native graph','{user}','ready','unconfigured-session','fixture-build','{json.dumps(structure)}');"
                    f"insert into graph_cells(id,graph_id,position,name,kind,variant,description,config,impl) "
                    f"values('{source}','{graph}',0,'input','source','ui','Native input','{{\"input\":\"x\"}}','{{\"input_type\":\"Nat\"}}'),"
                    f"('{cell}','{graph}',1,'compute','node',null,'Native computation','{json.dumps(cell_config)}','{{\"function\":\"compute\"}}');")
                api("/api/graph/cells/feed", {"project": "native-project", "graph": "native-graph", "cell": source, "value": 1}, ok=False)
                cell_row = json.loads(sql(f"select row_to_json(c) from connector_authorities c where cell_id='{cell}' order by expires_at desc limit 1"))
                assert cell_row["graph_id"] == graph
                cell_path = f"/v1/secret/data/connector-authority/{org}/{cell_row['run_id']}/{cell_row['warrant_id']}"
                assert vault.state["documents"][cell_path]["cell"]["scopes"] == declared["scopes"]
                narrowed = {**cell_config, "connectors": [{"connection": connection_id, "permissions": {**declared, "scopes": []}}]}
                api("/api/graph/cells/update", {"project": "native-project", "graph": "native-graph", "cell": cell,
                    "name": "compute", "description": "Native computation narrowed", "config": narrowed})
                assert cell_path not in vault.state["documents"]
                assert sql(f"select count(*) from connector_authorities where cell_id='{cell}'") == "0"
                api("/api/graph/cells/feed", {"project": "native-project", "graph": "native-graph", "cell": source, "value": 2}, ok=False)
                assert sql(f"select count(*) from connector_authorities where cell_id='{cell}'") == "0"
                assert len(upstream.state["calls"]) == before
                passed("actual app cell mint is declaration-bound; permission edits revoke old documents and cannot remint removed grants")
                sql(f"insert into credit_ledger(org_id,run_id,delta,reason) select '{org}','{uuid.uuid4()}',-sum(delta),'fixture-drain' from credit_ledger where org_id='{org}'")
                api("/api/ai/classify", classify, ok=False)
                assert len(upstream.state["calls"]) == before
                assert sql("select count(*) from credit_holds where state='held'") == "0"
                sql(f"insert into credit_ledger(org_id,run_id,delta,reason) values('{org}','{uuid.uuid4()}',100,'fixture-restore')")
                passed("exhausted real ledger denies native calls before credential egress")
                policy = {"effects": ["Connector"], "providers": ["typesafe"], "tools": [], "domains": [],
                          "configuredDomains": False, "connectorCeilings": {"typesafe": {**permissions, "scopes": []}}}
                api("/api/org/settings/permissions", {"policy": policy})
                assert run_path not in vault.state["documents"]
                assert vault.state["documents"][org_path]["scopes"] == []
                api("/api/ai/classify", classify, ok=False)
                assert len(upstream.state["calls"]) == before
                passed("org revocation deletes existing warrant-keyed run projections and blocks fresh calls")
                policy["connectorCeilings"]["typesafe"] = permissions
                api("/api/org/settings/permissions", {"policy": policy})
                assert api("/api/ai/classify", classify) == {"ready": True}
                before = len(upstream.state["calls"])
                vault.state["fail_prefix"] = f"/v1/secret/data/connector-authority/{org}/"
                api("/api/ai/classify", classify, ok=False)
                assert len(upstream.state["calls"]) == before
                vault.state["fail_prefix"] = credential_path + "/permissions"
                api("/api/connections/permissions", {"connection_id": connection_id, "permissions": permissions}, ok=False)
                assert vault.state["documents"][org_path]["scopes"] == []
                assert sql("select count(*) from credit_holds where state='held'") == "0"
                passed("run/permission provisioning outages fail closed and leave no live budget hold")
                vault.state.pop("fail_prefix")
                api("/api/connections/delete", {"id": connection_id})
                assert credential_path not in vault.state["documents"]
                assert credential_path + "/permissions" not in vault.state["documents"]
                assert not any(path.startswith(f"/v1/secret/data/connector-authority/{org}/") for path in vault.state["documents"])
                assert all(token == "Bearer broker-read-fixture" for token, _ in vault.state["reads"])
                assert sql("select count(*) from connector_authorities") == "0"
                assert sql("select count(*) from audit_log") != "0"
                passed("deletion revokes keys/projections; app never reads a secret; broker attempts are audited")
                if args.runtime:
                    assert proxy is not None
                    from native_runtime_cases import verify_runtime
                    verify_runtime(scratch, sql, api, exchange, runtime, pg_port, org, user, vault, upstream, app_env, command, proxy)
                if args.writer:
                    from native_writer_cases import verify_writer
                    verify_writer(sql, api, writer, vault, org, user)
                if args.real_writer:
                    assert writer is not None
                    from native_writer_pipeline import verify_pipeline
                    verify_pipeline(scratch, sql, api, exchange, lode, org, user, vault, upstream)
                    if args.runtime:
                        from lode_runtime_bridge_cases import verify_bridge
                        verify_bridge(scratch, sql, api, exchange, lode, runtime, pg_port, org, user, vault, upstream,
                                      writer, processes, lode_env)
                if args.lean_workspace:
                    connection, run, ident = str(uuid.uuid4()), str(uuid.uuid4()), str(uuid.uuid4())
                    lp = lambda value: struct.pack(">Q", len(value.encode())) + value.encode()
                    expires = int(time.time()) + 300
                    caveats = [({"kind": "expiresAt", "value": str(expires)}, b"\x00" + struct.pack(">Q", expires)),
                               ({"kind": "capability", "provider": "openai", "action": "inference.generate"}, b"\x01" + lp("openai") + lp("inference.generate")),
                               ({"kind": "resource", "value": connection}, b"\x02" + lp(connection)),
                               ({"kind": "budget", "value": "1"}, b"\x03" + struct.pack(">Q", 1)),
                               ({"kind": "runId", "value": run}, b"\x04" + lp(run))]
                    tag = hmac.new(bytes.fromhex(key), lp(ident) + lp(org), hashlib.sha256).digest()
                    for _, encoded in caveats:
                        tag = hmac.new(tag, encoded, hashlib.sha256).digest()
                    credentials = {"account": f"{user}/{connection}", "cost": 1,
                                   "warrant": {"id": ident, "orgId": org, "tag": tag.hex(), "caveats": [value for value, _ in reversed(caveats)]}}
                    permission = {"scopes": [{"operation": "inference.generate", "root": ["gpt-4o-mini"], "descendants": False}],
                                  "maxRequestBytes": 1048576, "maxResponseBytes": 1048576}
                    capability = {**permission, "provider": "openai", "connection": connection}
                    vault.state["documents"].update({
                        f"/v1/secret/data/thirdparty/openai/{user}/{connection}": {"kind": "bearer", "base_url": vault.state["upstream"] + "/base", "token": "local-provider-fixture"},
                        f"/v1/secret/data/thirdparty/openai/{user}/{connection}/permissions": permission,
                        f"/v1/secret/data/connector-policy/{org}/openai/{connection}": permission,
                        f"/v1/secret/data/connector-authority/{org}/{run}/{ident}": {"account": credentials["account"], "cell": capability, "warrant": capability,
                            "conversation": {"sessionId": "fixture-session", "allowedTools": ["todo"]}}})
                    native_before = len(upstream.state["calls"])
                    smoke = subprocess.run([str(ROOT.parent / "lode/.lake/build/bin/lode-native-smoke")],
                        input=json.dumps({"broker": broker, "credentials": credentials}) + "\n", cwd=args.lean_workspace,
                        capture_output=True, text=True, timeout=60)
                    assert smoke.returncode == 0, (smoke.stdout, smoke.stderr)
                    assert "PASS: real native SDK" in smoke.stdout
                    assert "PASS: real native writer" in smoke.stdout
                    print(smoke.stdout.strip(), flush=True)
                    # The same signed identity now selects Radius. Re-sign the
                    # provider caveat and independently provision its ceilings.
                    radius = json.loads(json.dumps(credentials))
                    radius["warrant"]["id"] = str(uuid.uuid4())
                    radius["warrant"]["caveats"][3]["provider"] = "radius"
                    caves = list(reversed(radius["warrant"]["caveats"]))
                    tag = hmac.new(bytes.fromhex(key), lp(radius["warrant"]["id"]) + lp(org), hashlib.sha256).digest()
                    for caveat in caves:
                        kind = caveat["kind"]
                        encoded = b"\x00" + struct.pack(">Q", int(caveat["value"])) if kind == "expiresAt" else b"\x01" + lp("radius") + lp("inference.generate") if kind == "capability" else b"\x02" + lp(connection) if kind == "resource" else b"\x03" + struct.pack(">Q", 1) if kind == "budget" else b"\x04" + lp(run)
                        tag = hmac.new(tag, encoded, hashlib.sha256).digest()
                    radius["warrant"]["tag"] = tag.hex()
                    radius_permissions = {**permission, "scopes": [{"operation": "inference.generate", "root": ["radius-fixture"], "descendants": False}]}
                    radius_cap = {**radius_permissions, "provider": "radius", "connection": connection}
                    vault.state["documents"].update({f"/v1/secret/data/thirdparty/radius/{user}/{connection}": {"kind": "bearer", "base_url": vault.state["upstream"] + "/base", "token": "local-provider-fixture"},
                        f"/v1/secret/data/thirdparty/radius/{user}/{connection}/permissions": radius_permissions,
                        f"/v1/secret/data/connector-policy/{org}/radius/{connection}": radius_permissions,
                        f"/v1/secret/data/connector-authority/{org}/{run}/{radius['warrant']['id']}": {"account": radius["account"], "cell": radius_cap, "warrant": radius_cap, "conversation": {"sessionId": "fixture-session", "allowedTools": ["todo"]}}})
                    response = subprocess.run([str(ROOT.parent / "lode/.lake/build/bin/lode-native-smoke")],
                        input=json.dumps({"broker": broker, "provider": "radius", "credentials": radius}) + "\n", capture_output=True, text=True, timeout=60)
                    assert response.returncode == 0, (response.stdout, response.stderr)
                    print(response.stdout.strip(), flush=True)
                print(f"PASS: {len(checks)} real app/broker/SQL integration groups")
        finally:
            for process in reversed(processes):
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            command(str(args.pg_bin / "pg_ctl"), "-D", str(scratch / "pg"), "-m", "immediate", "-w", "stop")
            for server in [vault, upstream]:
                server.shutdown()
                server.server_close()
            if proxy:
                proxy.shutdown()
                proxy.server_close()
            if writer:
                writer.shutdown()
                writer.server_close()


if __name__ == "__main__":
    main()
