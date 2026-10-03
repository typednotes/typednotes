"""App-created authority/credentials executed by a real compiled Lun driver."""
import copy
import json
import os
from pathlib import Path
import time
import uuid
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]


def verify_runtime(scratch, sql, api, exchange, runtime, pg_port, org, user, vault, upstream, env, command, proxy):
    checks = []
    def passed(label):
        checks.append(label)
        print("PASS compiled app/runtime:", label, flush=True)
    def runtime_api(path, value):
        return exchange(runtime + path, value, {"Authorization": "Bearer fixture-lun"})
    policy = {"effects": ["Trace", "Error", "HTTP", "FileSystem", "PostgreSQL", "SecretStore", "ObjectStore", "Connector"],
              "providers": ["s3", "openai", "github"], "tools": ["read", "todo"], "domains": [], "configuredDomains": True}
    api("/api/org/settings/permissions", {"policy": policy})
    project, graph, source, secret_cell, shared_owner, conn = [str(uuid.uuid4()) for _ in range(6)]
    ids = {name: str(uuid.uuid4()) for name in ["store", "rows", "token_present", "put_token", "relay"]}
    schema = "native_fixture_" + user.replace("-", "_")
    def permission(operation, root, descendants=False):
        return {"scopes": [{"operation": operation, "root": root, "descendants": descendants}],
                "maxRequestBytes": 1048576, "maxResponseBytes": 16777216}
    configs = {
        "store": {"table": "notes", "dependencies": ["input"], "output_type": "Nat"},
        "rows": {"connectors": [{"connection": "compute", "permissions": permission("rows.select", [schema], True)}]},
        "token_present": {},
        "put_token": {"connectors": [{"connection": "graph", "permissions": permission("secrets.write", ["token"])}]},
        "relay": {"connectors": [{"connection": conn, "permissions": permission("objects.read", ["reports"], True)}]},
    }
    nodes = [{"id": 0, "input": "value"}] + [{"id": i + 1, "function": name, "args": [0] if name == "store" else []} for i, name in enumerate(ids)]
    structure = {"inputs": ["value"], "nodes": nodes, "sources": [0, 2, 3, 4, 5], "sinks": [1, 2, 3, 4, 5]}
    sql(f"insert into users(id,email) values('{shared_owner}','shared-owner@example.invalid');"
        f"insert into memberships(org_id,user_id,role) values('{org}','{shared_owner}','member');"
        f"insert into connections(id,org_id,user_id,provider,label,base_url,external_id,status) values "
        f"('{conn}','{org}','{shared_owner}','s3','Shared bucket','https://s3.example.invalid/bucket','bucket','active');"
        f"insert into projects(id,org_id,slug,name,created_by) values('{project}','{org}','runtime-project','Runtime project','{user}');"
        f"insert into graphs(id,project_id,slug,name,created_by,status,lun_session_id,session_build_id,structure) values"
        f"('{graph}','{project}','runtime-graph','Runtime graph','{user}','ready','bootstrap-session','bootstrap-build','{json.dumps(structure)}');")
    impl = {"function": None, "constant": None, "module": None, "signature": None, "effects": [], "input": "value", "input_type": "String"}
    sql(f"insert into graph_cells(id,graph_id,position,name,kind,variant,description,config,impl) values "
        f"('{source}','{graph}',0,'input','source','ui','Runtime input','{{\"input\":\"value\",\"output_type\":\"String\"}}','{json.dumps(impl)}'),"
        f"('{secret_cell}','{graph}',1,'secret_source','source','secret','Fixture graph secret','{{\"name\":\"token\"}}',null);")
    for i, name in enumerate(ids):
        local_impl = {**impl, "function": name, "input": None, "input_type": None}
        sql(f"insert into graph_cells(id,graph_id,position,name,kind,variant,description,config,impl) values "
            f"('{ids[name]}','{graph}',{i + 2},'{name}','{'sink' if name == 'store' else 'node'}',"
            f"{'NULL' if name != 'store' else chr(39) + 'db' + chr(39)},'Runtime fixture cell','{json.dumps(configs[name])}','{json.dumps(local_impl)}')")
    vault.state["documents"][f"/v1/secret/data/thirdparty/s3/{shared_owner}/{conn}"] = {
        "kind": "s3", "base_url": f"http://127.0.0.1:{upstream.server_port}/bucket", "region": "us-east-1",
        "access_key_id": "fixture-key", "secret_access_key": "fixture-secret"}
    args = {"project": "runtime-project", "graph": "runtime-graph"}
    api("/api/graph/cells/secret", {**args, "cell": secret_cell, "value": "graph-private-fixture"})
    api("/api/graph/cells/feed", {**args, "cell": source, "value": "bootstrap"}, ok=False)
    assert sql(f"select name from compute_schemas where org_id='{org}' and user_id='{user}'") == schema
    assert sql(f"select rolpassword like 'SCRAM-SHA-256%' from pg_authid where rolname='{schema}'") == "t"
    assert sql(f"select tableowner from pg_tables where schemaname='{schema}' and tablename='notes'") == schema
    credential = vault.state["documents"][f"/v1/secret/data/compute/{org}/{user}"]
    assert credential["schema"] == schema and credential["kind"] == "postgres"
    assert any(row.get("provider") == "postgres" and row.get("connection_ref") == "compute" and row.get("connection_id") is None
               for row in json.loads(sql(f"select json_agg(a) from connector_authorities a where graph_id='{graph}'")))
    passed("real app provisions SCRAM actor role/table and synthetic local authority records")
    repo = scratch / "app-runtime-repo"
    repo.mkdir()
    (repo / "lean-toolchain").write_text((ROOT.parent / "lun/lean-toolchain").read_text())
    (repo / "lakefile.toml").write_text(f'name="app_runtime_fixture"\n[[require]]\nname="linen"\npath="{ROOT.parent / "linen"}"\n[[lean_lib]]\nname="AppRuntime"\n')
    fixture = f'''import Linen.Control.Monad.Effect.PostgreSQL
import Linen.Control.Monad.Effect.SecretStore
import Linen.Control.Monad.Effect.Connector
namespace AppRuntime
open Control.Monad.Effect
abbrev compute : PostgreSQL.Capability :=
  {{ host := "127.0.0.1", port := {pg_port}, database := "postgres", user := "{schema}", canSelect := true, canInsert := true }}
abbrev secrets : SecretStore.Capability :=
  {{ canGetValue := true, canDescribe := true, canPut := true, canList := true, scopes := [{{namePrefix := []}}] }}
abbrev remote : Connector.Capability :=
  {{provider := "s3", connection := "{conn}", scopes := [{{operation := "objects.read", root := ["reports"]}}]}}
def store (value : String) : Eff [PostgreSQL.PostgreSQL compute] Nat :=
  PostgreSQL.insertInto {{schema := "{schema}", name := "notes"}} ["value"] [.text (Lean.toJson value).compress] (hs := by rfl)
def rows : Eff [PostgreSQL.PostgreSQL compute] Nat := do
  return (← PostgreSQL.select {{schema := "{schema}", name := "notes"}}).rows.size
def token_present : Eff [SecretStore.SecretStore secrets] Bool := do
  return (← SecretStore.getString ["token"]).isOk
def put_token : Eff [SecretStore.SecretStore secrets] Bool := do
  return (← SecretStore.putString ["token"] "runtime-placeholder").isOk
def relay : Eff [Connector.Connector remote] Lean.Json :=
  Connector.call "objects.read" ["reports", "file"]
end AppRuntime
'''
    (repo / "AppRuntime.lean").write_text(fixture)
    command("lake", "update", cwd=repo, env=env)
    command("git", "init", "-q", "-b", "main", cwd=repo)
    command("git", "add", "-A", cwd=repo)
    command("git", "-c", "user.name=fixture", "-c", "user.email=fixture@localhost", "-c", "commit.gpgsign=false", "commit", "-qm", "fixture", cwd=repo)
    commit = command("git", "rev-parse", "HEAD", cwd=repo)
    functions = []
    for name in ids:
        effect = "PostgreSQL.PostgreSQL AppRuntime.compute" if name in ["store", "rows"] else "SecretStore.SecretStore AppRuntime.secrets" if name in ["token_present", "put_token"] else "Connector.Connector AppRuntime.remote"
        result_type = "Nat" if name in ["store", "rows"] else "Bool" if name in ["token_present", "put_token"] else "Lean.Json"
        signature = ("String → " if name == "store" else "") + f"Eff [Control.Monad.Effect.{effect}] {result_type}"
        functions.append({"name": name, "module": "AppRuntime", "function": "AppRuntime." + name, "signature": signature, "outputType": result_type})
    request = {"source": {"url": repo.as_uri(), "branch": "main", "commit": commit}, "functions": functions,
               "graphs": [{"name": "main", "inputTypes": {"value": "String"}, "dependencies": {"store": ["value"]},
                           "program": 'do\n let value ← input "value" String\n let s ← store value\n let _ ← rows\n let _ ← token_present\n let _ ← put_token\n let _ ← relay\n pure s'}]}
    status, build = runtime_api("/v0/builds", request)
    assert status == 202, build
    description = {}
    for _ in range(1500):
        with urlopen(Request(runtime + "/v0/builds/" + build["id"], headers={"Authorization": "Bearer fixture-lun"}), timeout=10) as response:
            description = json.load(response)
        if description["state"] in ["ready", "failed"]:
            break
        time.sleep(.2)
    assert description["state"] == "ready", description
    assert description["runtimeContract"] == "bounded-eff-worker-v2"
    actual_structure = next(g for g in description["graphs"] if g["name"] == "main")
    sql(f"update graphs set session_build_id='{build['id']}', lun_build_id='{build['id']}', lun_session_id=null, "
        f"structure='{json.dumps(actual_structure)}', status='ready' where id='{graph}';")
    # Historic type mismatch is intentionally retained; app registration must
    # select runtime recovery rather than failing or passing Nat as a String.
    sql(f"insert into graph_inputs(graph_id,input,value,fed_by) values('{graph}','value','17','ui');")
    restarted = api("/api/graph/restart", args)
    def result(reply, name):
        return next(node for node in reply["nodes"] if node.get("function") == name)["outcome"]
    assert result(restarted, "store")["skipped"] is not None, restarted
    assert sql(f'select count(*) from "{schema}".notes') == "0"
    passed("app recoverInputs registration turns incompatible recorded input into a source error and blocks the insert")
    fed = api("/api/graph/cells/feed", {**args, "cell": source, "value": "native-write"})
    assert result(fed, "store")["output"] == 1, fed
    assert result(fed, "rows")["output"] >= 0, fed
    assert result(fed, "token_present")["output"] is True, fed
    assert result(fed, "put_token")["output"] is True, fed
    assert result(fed, "relay")["output"]["body"] == {"native": True}, fed
    assert sql(f'select value::text from "{schema}".notes') == '"native-write"'
    passed("compiled driver consumes app DB/vault grants and HMAC broker grant for a shared credential owner")
    before_rows = sql(f'select count(*) from "{schema}".notes')
    before_reads = len(vault.state["reads"])
    safe = {"safeShare":True,"binding":{"org_id":org,"user_id":user,"graph_id":graph},"policy":{"effects":["Trace","Error"],"domains":[]},"connectors":{},"inputs":{"value":"public-must-not-write"}}
    status, shared = runtime_api(f"/v0/builds/{build['id']}/graphs/main/sessions", safe)
    assert status == 201, shared
    assert any(node.get("error") for node in shared["nodes"]), shared
    assert sql(f'select count(*) from "{schema}".notes') == before_rows
    assert len(vault.state["reads"]) == before_reads
    status, refused = runtime_api(f"/v0/builds/{build['id']}/graphs/main/sessions", {**safe,"policy":{"effects":["PostgreSQL"],"domains":[]}})
    assert status == 403, refused
    status, refused = runtime_api("/v0/sessions/"+shared["session"], {"inputs":{"value":"no-widen"},"policy":{"effects":["Trace","Error","HTTP"],"domains":[]},"connectors":{}})
    assert status == 403, refused
    passed("compiled public-share sessions refuse external effects before credentials/DB writes and cannot widen execution on update")
    captured = next(body for method, path, body in reversed(proxy.state["calls"]) if method == "POST" and path.startswith("/v0/sessions/") and body and "connectors" in body)
    def direct(name, body):
        status, result = runtime_api(f"/v0/builds/{build['id']}/functions/{name}", body)
        assert status == 200, result
        return result
    context = {key: copy.deepcopy(captured[key]) for key in ["binding", "policy", "connectors"]}
    db_grant = next(grant for grant in context["connectors"]["rows"] if grant["provider"] == "postgres")
    select = next(token for token in db_grant["warrants"] if token["operation"] == "rows.select")
    run = next(c["value"] for c in select["warrant"]["caveats"] if c["kind"] == "runId")
    projection = f"/v1/secret/data/connector-authority/{org}/{run}/{select['warrant']['id']}"
    for ceiling in ["organization", "connection", "cell", "warrant"]:
        path = f"/v1/secret/data/connector-policy/{org}/postgres/compute" if ceiling == "organization" else \
               f"/v1/secret/data/thirdparty/postgres/{user}/compute/permissions" if ceiling == "connection" else projection
        saved = copy.deepcopy(vault.state["documents"][path])
        document = vault.state["documents"][path] if ceiling in ["organization", "connection"] else vault.state["documents"][path][ceiling]
        document["scopes"] = []
        before_reads = len(vault.state["reads"])
        refused = direct("rows", context)
        assert refused.get("error"), (ceiling, refused)
        assert all(not path.startswith("/v1/secret/data/compute/") for _, path in vault.state["reads"][before_reads:])
        vault.state["documents"][path] = saved
    passed("actual app-minted DB warrants are denied independently by all four live ceilings before credential reads")
    wrong = copy.deepcopy(context); wrong["binding"]["schema"] = "another_schema"
    assert direct("rows", wrong).get("error")
    wrong = copy.deepcopy(context); wrong["binding"]["user_id"] = shared_owner
    assert direct("rows", wrong).get("error")
    wrong = copy.deepcopy(context); wrong["binding"]["graph_id"] = str(uuid.uuid4())
    assert direct("token_present", wrong).get("error")
    passed("compiled app grants reject cross-schema, cross-user and cross-graph substitutions")
    session = sql(f"select lun_session_id from graphs where id='{graph}'")
    assert vault.state["documents"][f"/v1/secret/data/graph/{org}/{graph}/token"]["value"] == "runtime-placeholder"
    revoked = copy.deepcopy(configs["store"])
    revoked["connectors"] = [{"connection": "compute", "permissions": {**permission("rows.insert", [schema, "notes"]), "scopes": []}}]
    api("/api/graph/cells/update", {**args, "cell": ids["store"], "name": "store", "description": "Denied insert", "config": revoked})
    before = sql(f'select count(*) from "{schema}".notes')
    denied = api("/api/graph/cells/feed", {**args, "cell": source, "value": "must-not-insert"})
    assert result(denied, "store")["error"], denied
    assert sql(f'select count(*) from "{schema}".notes') == before
    passed("cell permission narrowing revokes local run documents and denies actual driver writes")
    status, wrong_actor = runtime_api("/v0/sessions/" + session, {"inputs": {"value": "foreign"},
        "binding": {"org_id": org, "user_id": shared_owner, "graph_id": graph}, "policy": policy, "connectors": {}})
    assert status == 403, wrong_actor
    assert sql(f'select count(*) from "{schema}".notes') == before
    passed("shared credential owner cannot substitute the execution actor on a live runtime session")
    native_paths = [path for token, path in vault.state["reads"] if token == "Bearer runtime-read-fixture"]
    assert f"/v1/secret/data/compute/{org}/{user}" in native_paths
    assert f"/v1/secret/data/thirdparty/s3/{shared_owner}/{conn}/permissions" in [path for _, path in vault.state["reads"]]
    assert all(token != "Bearer app-write-only" for token, _ in vault.state["reads"])
    assert credential["token"] not in json.dumps([restarted, fed, denied, description])
    passed("runtime reads actor-bound compute and owner-bound external policy; API/model outputs contain no credential key")
    print(f"PASS: {len(checks)} compiled app/runtime integration groups", flush=True)
