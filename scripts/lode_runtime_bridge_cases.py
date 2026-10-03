"""App -> real Lode tool -> compiled bounded Lun effects, entirely local.

The anonymous HTTP peer uses a test-only libc network interposer: the runtime
resolves a public fixture address and pins it normally, then the socket is routed
to a loopback peer. No production permission/DNS check is changed and no external
request is made. The interposer is confined to fixture Lun and its children.
"""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[1]
HTTP_CALLS = []


def broker_module(name):
    spec = importlib.util.spec_from_file_location("bridge_" + name, ROOT.parent / "liaison/LiaisonTest/integration" / (name + ".py"))
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def http_environment(scratch):
    class Peer(BaseHTTPRequestHandler):
        def log_message(self, format, *args):
            pass

        def do_GET(self):
            HTTP_CALLS.append(self.path)
            assert self.headers["Host"] == "bridge.fixture.invalid"
            assert "Authorization" not in self.headers
            self.send_response(200)
            self.send_header("Content-Length", "2")
            self.end_headers()
            self.wfile.write(b"ok")

    peer = ThreadingHTTPServer(("127.0.0.1", 0), Peer)
    threading.Thread(target=peer.serve_forever, daemon=True).start()
    source = scratch / "bridge_http.c"
    library = scratch / ("bridge_http.dylib" if sys.platform == "darwin" else "bridge_http.so")
    source.write_text(r'''
#include <sys/socket.h>
#include <netdb.h>
#include <arpa/inet.h>
#include <dlfcn.h>
#include <string.h>
static int fixture_getaddrinfo(const char *node, const char *service,
    const struct addrinfo *hints, struct addrinfo **result) {
#ifdef __APPLE__
  int (*real)(const char*,const char*,const struct addrinfo*,struct addrinfo**) = getaddrinfo;
#else
  int (*real)(const char*,const char*,const struct addrinfo*,struct addrinfo**) = dlsym(RTLD_NEXT,"getaddrinfo");
#endif
  if (node && strcmp(node,"bridge.fixture.invalid") == 0)
    return real("93.184.215.14", service, hints, result);
  if (node && strcmp(node,"private.fixture.invalid") == 0)
    return real("127.0.0.1", service, hints, result);
  return real(node,service,hints,result);
}
static int fixture_connect(int fd,const struct sockaddr *addr,socklen_t size) {
#ifdef __APPLE__
  int (*real)(int,const struct sockaddr*,socklen_t) = connect;
#else
  int (*real)(int,const struct sockaddr*,socklen_t) = dlsym(RTLD_NEXT,"connect");
#endif
  if (addr->sa_family == AF_INET && size >= sizeof(struct sockaddr_in)) {
    const struct sockaddr_in *ip = (const struct sockaddr_in*)addr;
    if (ip->sin_addr.s_addr == inet_addr("93.184.215.14") && ip->sin_port == htons(80)) {
      struct sockaddr_in target = *ip;
      target.sin_addr.s_addr = inet_addr("127.0.0.1"); target.sin_port = htons(FIXTURE_PORT);
      return real(fd,(const struct sockaddr*)&target,sizeof(target));
    }
  }
  return real(fd,addr,size);
}
#ifdef __APPLE__
#define INTERPOSE(new,old) __attribute__((used)) static struct { const void *replacement; const void *original; } interpose_##old __attribute__((section("__DATA,__interpose"))) = { (void*)&new,(void*)&old };
INTERPOSE(fixture_getaddrinfo,getaddrinfo)
INTERPOSE(fixture_connect,connect)
#else
int getaddrinfo(const char *a,const char *b,const struct addrinfo *c,struct addrinfo **d) { return fixture_getaddrinfo(a,b,c,d); }
int connect(int a,const struct sockaddr *b,socklen_t c) { return fixture_connect(a,b,c); }
#endif
''')
    subprocess.run(["cc", "-dynamiclib" if sys.platform == "darwin" else "-shared", "-fPIC",
                    f"-DFIXTURE_PORT={peer.server_port}", str(source), "-o", str(library)], check=True)
    return {"DYLD_INSERT_LIBRARIES" if sys.platform == "darwin" else "LD_PRELOAD": str(library)}


class Writer:
    def __init__(self, repository, files):
        self.repository, self.files, self.step = repository, files, 0

    def handle(self, handler, raw):
        if handler.headers.get("Authorization", "").startswith("AWS4-HMAC-SHA256"):
            signatures = broker_module("connectors")
            signatures.verify_sigv4(handler.command, handler.path, handler.headers, raw)
            handler.server.state["calls"].append((handler.command, handler.path, None))
            return handler.reply(200, {"native": True})
        if "/chat/completions" not in handler.path:
            return self.repository.handle(handler, raw)
        request = json.loads(raw)
        assert "lun_call" in [tool["function"]["name"] for tool in request["tools"]]
        calls = []
        if self.step == 0:
            calls = [("write", {"path": path, "content": content}) for path, content in self.files.items()] + [("check", {})]
        elif self.step == 1:
            calls = [("publish", {"message": "Bounded Eff writer trial fixture"})]
        elif self.step == 2:
            calls = [("lun_build", {})]
        elif self.step == 3:
            assert ": ready" in raw.decode(), request["messages"][-1]
            calls = [("lun_call", {"kind": "function", "name": name, "body": body}) for name, body in [
                ("store", {"input": "writer-value"}), ("rows", {}), ("token_present", {}),
                ("relay", {}), ("temporary", {}), ("fetch", {}), ("traced", {}), ("private_fetch", {}),
                ("outside_rows", {}), ("unknown_secret", {}), ("undeclared", {}),
                ("rows", {"policy": {"effects": ["PostgreSQL"]}})]]
            calls.insert(7, ("lun_call", {"kind": "graph", "name": "main", "body": {"inputs": {"value": "writer-graph-trial"}}}))
        elif self.step == 4:
            content = raw.decode()
            assert "writer-temporary" in content and "native" in content, request["messages"][-1]
        elif self.step >= 5 and self.step % 2 == 1:
            calls = [("lun_call", {"kind": "function", "name": name, "body": {"input": "forbidden"} if name == "store" else {}})
                     for name in ["store", "relay", "temporary", "fetch"]]
        message: dict = {"content": "Bounded trial fixture complete"}
        if calls:
            message["tool_calls"] = [{"id": f"bridge-{self.step}-{i}", "type": "function",
                "function": {"name": name, "arguments": json.dumps(args)}} for i, (name, args) in enumerate(calls)]
        self.step += 1
        handler.reply(200, {"choices": [{"message": message, "finish_reason": "tool_calls" if calls else "stop"}],
                            "usage": {"prompt_tokens": 1, "completion_tokens": 1}})


def verify_bridge(scratch, sql, api, exchange, lode, runtime, pg_port, org, user, vault, upstream,
                  writer_proxy, processes, lode_env):
    sys.path.insert(0, str(ROOT.parent / "liaison/LiaisonTest/integration"))
    broker_fixture = broker_module("native_writer")
    repo_dir = scratch / "bridge-repository"
    repo_dir.mkdir()
    repository = broker_fixture.Repository(repo_dir, "github")
    repo, project, graph, source, secret, shared_owner, remote = [str(uuid.uuid4()) for _ in range(7)]
    schema = "native_fixture_" + user.replace("-", "_")
    def permission(op, root, descendants=False):
        return {"scopes": [{"operation": op, "root": root, "descendants": descendants}],
                "maxRequestBytes": 1048576, "maxResponseBytes": 16777216}
    policy = {"effects": ["Trace", "Error", "HTTP", "FileSystem", "PostgreSQL", "SecretStore", "Connector"],
        "providers": ["github", "openai", "s3"], "tools": ["read", "write", "check", "publish", "todo", "lun_build", "lun_call"],
        "domains": ["bridge.fixture.invalid", "private.fixture.invalid"], "configuredDomains": False}
    api("/api/org/settings/permissions", {"policy": policy})
    repo_permissions = permission("repositories.read", ["owner", "repo"], True)
    repo_permissions["scopes"].append({"operation": "repositories.write", "root": ["owner", "repo"], "descendants": True})
    sql(f"insert into users(id,email) values('{shared_owner}','bridge-owner@example.invalid');"
        f"insert into memberships(org_id,user_id,role) values('{org}','{shared_owner}','member');"
        f"insert into connections(id,org_id,user_id,provider,label,base_url,status,permissions) values"
        f"('{repo}','{org}','{user}','github','Bridge repository','https://api.github.com','active','{json.dumps(repo_permissions)}'),"
        f"('{remote}','{org}','{shared_owner}','s3','Bridge shared bucket','https://s3.example.invalid/bucket','active','{json.dumps(permission('objects.read', ['reports'], True))}');"
        f"insert into projects(id,org_id,slug,name,created_by,repo_connection_id,repo_provider,repo_full_name,repo_web_url,repo_default_branch) values"
        f"('{project}','{org}','bridge-project','Bridge project','{user}','{repo}','github','owner/repo','https://github.com/owner/repo','main');"
        f"insert into graphs(id,project_id,slug,name,created_by) values('{graph}','{project}','bridge','Bridge graph','{user}');"
        f"insert into graph_cells(id,graph_id,position,name,kind,variant,description,config) values"
        f"('{source}','{graph}',0,'input','source','ui','Bridge input','{{\"input\":\"value\",\"output_type\":\"String\"}}'),"
        f"('{secret}','{graph}',1,'secret_source','source','secret','Bridge secret','{{\"name\":\"token\"}}');")
    configs = {
        "store": {"table": "notes", "dependencies": ["input"], "output_type": "Nat"},
        "rows": {"connectors": [{"connection": "compute", "permissions": permission("rows.select", [schema], True)}], "output_type": "Nat"},
        "token_present": {"output_type": "Bool"},
        "relay": {"connectors": [{"connection": remote, "permissions": permission("objects.read", ["reports"], True)}], "output_type": "Lean.Json"},
        "temporary": {"output_type": "String"}, "fetch": {"output_type": "Nat"},
        "traced": {"output_type": "Nat"},
        "private_fetch": {"output_type": "Nat"},
        "outside_rows": {"connectors": [{"connection": "compute", "permissions": permission("rows.select", [schema], True)}], "output_type": "Nat"},
        "unknown_secret": {"output_type": "Bool"},
    }
    for i, (name, config) in enumerate(configs.items()):
        config.setdefault("dependencies", [])
        sql(f"insert into graph_cells(id,graph_id,position,name,kind,variant,description,config) values"
            f"('{uuid.uuid4()}','{graph}',{i+2},'{name}','{'sink' if name == 'store' else 'node'}',"
            f"{chr(39)+'db'+chr(39) if name == 'store' else 'null'},'Bounded writer fixture','{json.dumps(config)}');")
    vault.state["documents"].update({
        f"/v1/secret/data/thirdparty/github/{user}/{repo}": {"kind": "bearer", "base_url": f"http://127.0.0.1:{upstream.server_port}/base", "token": "local-provider-fixture"},
        f"/v1/secret/data/thirdparty/s3/{shared_owner}/{remote}": {"kind": "s3", "base_url": f"http://127.0.0.1:{upstream.server_port}/bucket", "region": "us-east-1", "access_key_id": "fixture-key", "secret_access_key": "fixture-secret"},
    })
    args = {"project": "bridge-project", "graph": "bridge"}
    api("/api/graph/cells/secret", {**args, "cell": secret, "value": "bridge-private-secret"})
    model = api("/api/connections/ai", {"provider": "Openai", "api_key": "local-provider-fixture", "base_url": "https://fixture.example.invalid/v1"})
    api("/api/graph/model", {**args, "connection": model["id"], "model": "gpt-4o-mini"})
    lean = f'''import Linen.Control.Monad.Effect.PostgreSQL
import Linen.Control.Monad.Effect.SecretStore
import Linen.Control.Monad.Effect.Connector
import Linen.Control.Monad.Effect.FileSystem
import Linen.Control.Monad.Effect.HTTP
import Linen.Control.Monad.Effect.Trace
namespace Bridge
open Control.Monad.Effect
abbrev compute : PostgreSQL.Capability :=
  {{host := "127.0.0.1", port := {pg_port}, database := "postgres", user := "{schema}", canSelect := true, canInsert := true}}
abbrev secrets : SecretStore.Capability :=
  {{canGetValue := true, canDescribe := true, canList := true, scopes := [{{namePrefix := []}}]}}
abbrev remote : Connector.Capability :=
  {{provider := "s3", connection := "{remote}", scopes := [{{operation := "objects.read", root := ["reports"]}}]}}
abbrev files : FileSystem.Capability :=
  {{canRead := true, canWrite := true, scopes := [FileSystem.under ["tmp"]]}}
def store (value : String) : Eff [PostgreSQL.PostgreSQL compute] Nat :=
  PostgreSQL.insertInto {{schema := "{schema}", name := "notes"}} ["value"] [.text (Lean.toJson value).compress] (hs := by rfl)
def rows : Eff [PostgreSQL.PostgreSQL compute] Nat := do
  return (← PostgreSQL.select {{schema := "{schema}", name := "notes"}}).rows.size
def outside_rows : Eff [PostgreSQL.PostgreSQL compute] Nat := do
  return (← PostgreSQL.select {{schema := "other_actor", name := "notes"}}).rows.size
def token_present : Eff [SecretStore.SecretStore secrets] Bool := do
  return (← SecretStore.getString ["token"]).isOk
def unknown_secret : Eff [SecretStore.SecretStore secrets] Bool := do
  return (← SecretStore.getString ["ungranted"]).isOk
def relay : Eff [Connector.Connector remote] Lean.Json := Connector.call "objects.read" ["reports", "file"]
def temporary : Eff [FileSystem.FileSystem files] String := do
  FileSystem.writeFileString ["tmp", "bridge.txt"] "writer-temporary"
  return (← FileSystem.readFileString? ["tmp", "bridge.txt"]).getD "invalid utf8"
def fetch : Eff [HTTP.HTTP HTTP.readOnlyWeb] Nat := do
  return (← HTTP.get u!"http://bridge.fixture.invalid/").statusCode.statusCode
def private_fetch : Eff [HTTP.HTTP HTTP.readOnlyWeb] Nat := do
  return (← HTTP.get u!"http://private.fixture.invalid/").statusCode.statusCode
def traced : Eff [Trace.Trace] Nat := do
  Trace.trace "authorized writer trace"
  pure 42
end Bridge
'''
    functions = []
    for name in configs:
        effect = "PostgreSQL.PostgreSQL Bridge.compute" if name in ["store", "rows", "outside_rows"] else "SecretStore.SecretStore Bridge.secrets" if name in ["token_present", "unknown_secret"] else "Connector.Connector Bridge.remote" if name == "relay" else "FileSystem.FileSystem Bridge.files" if name == "temporary" else "Trace.Trace" if name == "traced" else "HTTP.HTTP HTTP.readOnlyWeb"
        signature = ("String → " if name == "store" else "") + f"Eff [{effect}] {configs[name]['output_type']}"
        functions.append({"name": name, "module": "Bridge", "function": "Bridge." + name, "signature": signature, "outputType": configs[name]["output_type"]})
    manifest = {"open": ["Bridge", "Control.Monad.Effect"], "functions": functions,
        "graphs": [{"name": "main", "inputTypes": {"value": "String"}, "dependencies": {"store": ["value"]},
            "program": 'do\n let value ← input "value" String\n' +
                ''.join(f' let _ ← {name}\n' for name in configs if name != "store") + ' store value'}]}
    files = {"Bridge.lean": lean, "lun.json": json.dumps(manifest),
        "lakefile.toml": f'name="bridge_fixture"\ndefaultTargets=["Bridge"]\n[[require]]\nname="linen"\npath="{ROOT.parent / "linen"}"\n[[lean_lib]]\nname="Bridge"\n',
        "lean-toolchain": (ROOT.parent / "lode/lean-toolchain").read_text()}
    writer = Writer(repository, files)
    upstream.state["pipeline"] = writer
    api("/api/graph/implement", {**args, "note": "Compile and try the bounded functions", "steer": False})
    sid = sql(f"select lode_session_id from graphs where id='{graph}'")
    headers = {"Authorization": "Bearer fixture-writer"}
    def idle():
        for _ in range(2400):
            code, status = exchange(lode + f"/v0/sessions/{sid}", headers=headers)
            assert code == 200, status
            if status["state"] == "idle": return status
            time.sleep(.2)
        raise AssertionError("bounded writer did not become idle")
    status = idle()
    _, log = exchange(lode + f"/v0/sessions/{sid}/messages", headers=headers)
    results = [r for entry in log["entries"] if entry.get("type") == "tool_results" for r in entry["results"]]
    assert not any(r["isError"] for r in results[:len(files) + 3]), results
    calls = [r for r in results if r["name"] == "lun_call"]
    assert len(calls) == 13, results
    for r in calls[:8]: assert not r["isError"], r
    assert json.loads(calls[0]["content"])["output"] == 1
    assert json.loads(calls[2]["content"])["output"] is True
    assert json.loads(calls[5]["content"])["output"] == 200
    assert json.loads(calls[6]["content"])["output"] == 42
    assert "authorized writer trace" in calls[6]["content"]
    assert next(node["output"] for node in json.loads(calls[7]["content"])["nodes"] if node.get("function") == "store") == 1
    for r in calls[8:]: assert r["isError"], r
    assert "non-public" in calls[8]["content"], calls[8]
    # The immutable cell-layout contract requires every declared function in
    # the graph: the approved HTTP call runs once directly and once in the graph.
    assert HTTP_CALLS == ["/", "/"], HTTP_CALLS
    assert "bridge-private-secret" not in json.dumps([status, log])
    assert "fixture-key" not in json.dumps([status, log])
    metadata = json.loads((scratch / "lode/sessions" / sid / "session.json").read_text())
    assert not any(word in json.dumps(metadata) for word in ["warrants", '"tag"', "bridge-private-secret", "fixture-secret"])
    grants = status["execution"]["execution"]["connectors"]["relay"]
    assert grants[0]["account"] == f"{shared_owner}/{remote}"
    assert status["execution"]["execution"]["binding"]["user_id"] == user
    print("PASS Lode bounded bridge: actual tool calls execute SCRAM DB, graph vault, temporary files, pinned anonymous HTTP, authorized Trace, graph run and real HMAC native connector; owner differs from actor", flush=True)
    print("PASS Lode bounded bridge: schema/secret/private HTTP/undeclared service/model authority injection denied; no persisted operation warrants", flush=True)
    before_rows = sql(f'select count(*) from "{schema}".notes')
    before_http = len(HTTP_CALLS)
    execution = next(body["execution"] for method, path, body in reversed(writer_proxy.state["calls"])
        if method == "POST" and path.endswith("/messages") and body.get("execution", {}).get("binding", {}).get("graph_id") == graph)
    def send(method, suffix, body):
        return exchange(lode + f"/v0/sessions/{sid}" + suffix, body, headers, method=method)
    modified = copy.deepcopy(execution)
    modified["policy"]["effects"] = []
    assert send("PUT", "/credentials", {"execution": modified})[0] == 400
    modified = copy.deepcopy(execution)
    modified["binding"]["user_id"] = shared_owner
    assert send("POST", "/messages", {"text": "Substituted actor", "execution": modified})[0] == 400
    modified = copy.deepcopy(execution)
    modified["connectors"]["relay"][0]["account"] = f"{user}/{remote}"
    assert send("POST", "/messages", {"text": "Substituted credential owner", "execution": modified})[0] == 400
    modified = copy.deepcopy(execution)
    modified["_runtime"] = {"SECRETS_TOKEN": "injected"}
    assert send("PUT", "/credentials", {"execution": modified})[0] == 400
    assert send("PUT", "/credentials", {"execution": execution})[0] == 200
    expired = copy.deepcopy(execution)
    for grants in expired["connectors"].values():
        for grant in grants:
            for token in grant["warrants"]:
                for caveat in token["warrant"]["caveats"]:
                    if caveat["kind"] == "expiresAt": caveat["value"] = "0"
    assert send("PUT", "/credentials", {"execution": expired})[0] == 200
    assert send("POST", "/messages", {"text": "Attempt trial with stale operation warrants"})[0] == 202
    idle()
    _, stale_log = exchange(lode + f"/v0/sessions/{sid}/messages", headers=headers)
    stale = [r for entry in stale_log["entries"] if entry.get("type") == "tool_results" for r in entry["results"] if r["name"] == "lun_call"][-4:]
    assert all(r["isError"] and "expired" in r["content"] for r in stale), stale
    assert sql(f'select count(*) from "{schema}".notes') == before_rows and len(HTTP_CALLS) == before_http
    print("PASS Lode bounded bridge: PUT cannot edit policy/identity/runtime; fresh token replacement works; expired warrants deny before all trial effects", flush=True)
    assert send("PUT", "/credentials", {"execution": execution})[0] == 200
    # Reload the exact disk record into a new compiled server process. No old
    # model/repository/runtime tokens may survive; then test app refresh/resume.
    process = next(p for p in processes if str(p.args[0]).endswith("/lode"))
    process.terminate()
    process.wait(timeout=10)
    with (scratch / "bridge-restart.log").open("w") as logfile:
        restarted = subprocess.Popen(process.args, env=lode_env, stdout=logfile, stderr=logfile)
    processes.append(restarted)
    for _ in range(200):
        try:
            code, loaded = exchange(lode + f"/v0/sessions/{sid}", headers=headers)
            if code == 200: break
        except OSError:
            pass
        assert restarted.poll() is None, "compiled Lode failed to restart"
        time.sleep(.05)
    else: raise AssertionError("compiled Lode restart did not become ready")
    assert loaded["execution"] == status["execution"]
    assert not any(loaded["credentials"].values()), loaded["credentials"]
    assert send("POST", "/messages", {"text": "Attempt restart without credentials"})[0] == 202
    idle()
    assert writer.step == 7, writer.step
    api("/api/graph/implement", {**args, "note": "Refresh and resume after restart", "steer": True})
    resumed = idle()
    assert resumed["credentials"]["execution"] is True
    print("PASS Lode bounded bridge: actual compiled restart preserves ceilings, drops every token, cannot execute before app refresh, and resumes with caller-minted grants", flush=True)
    # The resumed run is authorized again. Its store/file/http/relay calls all
    # succeed; now record new counters before the conservative live revocation.
    _, resumed_log = exchange(lode + f"/v0/sessions/{sid}/messages", headers=headers)
    resumed_calls = [r for entry in resumed_log["entries"] if entry.get("type") == "tool_results" for r in entry["results"] if r["name"] == "lun_call"][-4:]
    assert all(not r["isError"] for r in resumed_calls), resumed_calls
    fresh_execution = next(body["execution"] for method, path, body in reversed(writer_proxy.state["calls"])
        if method == "POST" and path.endswith("/messages") and body.get("execution", {}).get("binding", {}).get("graph_id") == graph)
    # Pause automatic adoption while measuring isolated denial calls. Otherwise
    # the scheduler can start this fully authorized graph (including rows) during
    # a trial build and legitimately read the same actor's compute credential.
    # Take the real generation lock so an in-flight adoption has drained first.
    sql(f"begin; select pg_advisory_xact_lock(hashtextextended('{graph}',3)); "
        f"update graphs set status='editing' where id='{graph}'; commit;")
    # Each trial starts with one independently attenuated ceiling; the tokens,
    # real app-provisioned vault projections and compiled implementation are the
    # same ones as the authorized run. No fake read/write test preset is minted.
    for ceiling in ["organization", "connectionPermissions", "cell", "warrantPermissions"]:
        denied_execution = copy.deepcopy(fresh_execution)
        grant = next(grant for grant in denied_execution["connectors"]["store"] if grant["provider"] == "postgres")
        grant[ceiling]["scopes"] = []
        before = sql(f'select count(*) from "{schema}".notes')
        reads = len(vault.state["reads"])
        spec = {"source": {"url": repository.repo.as_uri(), "branch": "main", "path": "typednotes/bridge"},
            "model": {"api": "scripted", "script": [
                {"calls": [{"name": "lun_build", "arguments": {}}]},
                {"calls": [{"name": "lun_call", "arguments": {"kind": "function", "name": "store", "body": {"input": "denied"}}}]},
                {"text": "Independent ceiling refusal verified"}]},
            "tools": ["lun_build", "lun_call"], "execution": denied_execution,
            "message": "Try the compiled function under an independently narrowed ceiling"}
        code, opened = exchange(lode + "/v0/sessions", spec, headers)
        assert code == 201, opened
        denial_id = opened["id"]
        for _ in range(2400):
            _, state = exchange(lode + f"/v0/sessions/{denial_id}", headers=headers)
            if state["state"] == "idle": break
            time.sleep(.2)
        else: raise AssertionError("independent ceiling trial did not end")
        _, denied_log = exchange(lode + f"/v0/sessions/{denial_id}/messages", headers=headers)
        denied_results = [result for entry in denied_log["entries"] if entry.get("type") == "tool_results" for result in entry["results"]]
        assert len(denied_results) == 2 and not denied_results[0]["isError"], denied_results
        refusal = denied_results[1]
        assert refusal["isError"] and "native operation exceeds a capability ceiling" in refusal["content"], (ceiling, refusal)
        assert sql(f'select count(*) from "{schema}".notes') == before
        assert not any(path == f"/v1/secret/data/compute/{org}/{user}" for _, path in vault.state["reads"][reads:])
    print("PASS Lode bounded bridge: every independent org/connection/cell/warrant ceiling denies the actual tool-dispatched insert before compute credential resolution", flush=True)
    before_rows = sql(f'select count(*) from "{schema}".notes')
    before_http = len(HTTP_CALLS)
    fetch_cell = sql(f"select id::text from graph_cells where graph_id='{graph}' and name='fetch'")
    api("/api/graph/cells/update", {**args, "cell": fetch_cell, "name": "fetch",
        "description": "Changed caller declaration for the HTTP trial", "config": configs["fetch"]})
    _, declaration_revoked = exchange(lode + f"/v0/sessions/{sid}", headers=headers)
    assert declaration_revoked["execution"]["execution"]["policy"]["effects"] == []
    print("PASS Lode bounded bridge: acknowledged cell declaration edit revokes outstanding anonymous and native writer effects", flush=True)
    api("/api/org/settings/permissions", {"policy": policy})
    _, revoked = exchange(lode + f"/v0/sessions/{sid}", headers=headers)
    assert revoked["execution"]["execution"]["policy"]["effects"] == []
    assert send("POST", "/messages", {"text": "Attempt to restore removed authority", "execution": execution})[0] == 400
    # Retain this specific session as an in-flight fixture, so the real app's
    # steering path mints fresh model credentials for its actual ID while
    # intersecting execution with its empty current ceiling. Regeneration of an
    # edited declaration normally opens a fresh session and tests another bound.
    sql(f"begin; select pg_advisory_xact_lock(hashtextextended('{graph}',3)); "
        f"update graphs set status='implementing',lode_session_id='{sid}' where id='{graph}'; commit;")
    api("/api/graph/implement", {**args, "note": "Try again after policy revocation", "steer": True})
    assert sql(f"select lode_session_id from graphs where id='{graph}'") == sid
    idle()
    _, log = exchange(lode + f"/v0/sessions/{sid}/messages", headers=headers)
    calls = [r for entry in log["entries"] if entry.get("type") == "tool_results" for r in entry["results"] if r["name"] == "lun_call"]
    assert all(r["isError"] for r in calls[-4:]), calls[-4:]
    assert sql(f'select count(*) from "{schema}".notes') == before_rows
    assert len(HTTP_CALLS) == before_http
    print("PASS Lode bounded bridge: acknowledged live policy revocation also closes HTTP/files; new app warrants cannot widen the running writer", flush=True)
    upstream.state.pop("pipeline")
