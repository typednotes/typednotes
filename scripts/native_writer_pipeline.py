"""Actual app -> compiled Lode -> real broker -> local Git -> compiled Lun."""
import importlib.util
import json
from pathlib import Path
import sys
import time
import uuid
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


class Pipeline:
    def __init__(self, repository, files):
        self.repository, self.files, self.step = repository, files, 0

    def handle(self, handler, raw):
        if "/chat/completions" not in handler.path:
            return self.repository.handle(handler, raw)
        assert handler.headers["Authorization"] == "Bearer local-provider-fixture"
        request = json.loads(raw)
        assert request["model"] == "gpt-4o-mini"
        names = [tool["function"]["name"] for tool in request.get("tools", [])]
        assert {"read", "write", "bash", "check", "publish", "todo"} <= set(names), names
        assert set(names) <= {"read", "write", "bash", "check", "publish", "todo", "ls", "grep", "edit", "lun_build"}, names
        calls = []
        if self.step == 0:
            calls = [("write", {"path": path, "content": contents}) for path, contents in self.files.items()]
            calls += [("bash", {"command": "rm -- keep.sh"}), ("check", {})]
        elif self.step == 1:
            calls = [("publish", {"message": "Denied removal fixture"})]
        elif self.step == 2:
            assert "capability_denied" in raw.decode(), raw.decode()
        elif self.step == 3:
            calls = [("publish", {"message": "Explicitly authorized removal fixture"})]
        elif self.step == 4:
            assert "Published" in raw.decode(), raw.decode()
            calls=[("lun_build",{})]
        elif self.step == 5:
            assert "ready" in raw.decode(),raw.decode()
        elif self.step == 6:
            bad_manifest=json.loads(self.files["lun.json"])
            bad_manifest["functions"][0]["signature"]="String → Eff [] Nat"
            bad_manifest["functions"][0]["outputType"]="Nat"
            calls=[("write",{"path":"Sheet.lean","content":'import Linen.Control.Monad.Effect\nnamespace Sheet\nopen Control.Monad.Effect\ndef compute (value : String) : Eff [] Nat := pure value.length\nend Sheet\n'}),
                ("write",{"path":"lun.json","content":json.dumps(bad_manifest)}),
                ("publish",{"message":"Attempt to override caller String pin"})]
        elif self.step == 7:
            assert "Published" in raw.decode(),raw.decode()
            calls=[("lun_build",{})]
        elif self.step == 8:
            assert "failed" in raw.decode(),raw.decode()
        message: dict[str, Any] = {"content": "Expected removal denial" if self.step == 2 else "Fixture pipeline complete"}
        if calls:
            message["tool_calls"] = [{"id": f"pipeline-{self.step}-{i}", "type": "function",
                "function": {"name": name, "arguments": json.dumps(args)}} for i, (name, args) in enumerate(calls)]
        self.step += 1
        handler.reply(200, {"choices": [{"message": message, "finish_reason": "tool_calls" if calls else "stop"}],
                            "usage": {"prompt_tokens": 1, "completion_tokens": 1}})


def verify_pipeline(scratch, sql, api, exchange, lode, org, user, vault, upstream):
    integration = ROOT.parent / "liaison/LiaisonTest/integration"
    if str(integration) not in sys.path: sys.path.insert(0, str(integration))
    import native_writer as broker_fixture
    repo_dir = scratch / "full-writer-repository"
    repo_dir.mkdir()
    repository = broker_fixture.Repository(repo_dir, "github")
    manifest = {"open": ["Sheet"], "functions": [{"name": "compute", "module": "Sheet", "function": "Sheet.compute", "signature": "String → Eff [] String"}],
                "graphs": [{"name": "main", "program": 'do\n let value ← input "value" String\n compute value'}]}
    files = {
        "Sheet.lean": 'import Linen.Control.Monad.Effect\nnamespace Sheet\nopen Control.Monad.Effect\ndef compute (value : String) : Eff [] String := pure ("done:" ++ value)\nend Sheet\n',
        "lakefile.toml": f'name="writer_generated_fixture"\ndefaultTargets=["Sheet"]\n[[require]]\nname="linen"\npath="{ROOT.parent / "linen"}"\n[[lean_lib]]\nname="Sheet"\n',
        "lean-toolchain": (ROOT.parent / "lode/lean-toolchain").read_text(),
        "lun.json": json.dumps(manifest),
    }
    upstream.state["pipeline"] = Pipeline(repository, files)
    repo, project, graph, source, node = [str(uuid.uuid4()) for _ in range(5)]
    permissions = {"scopes": [{"operation": op, "root": ["owner", "repo"], "descendants": True} for op in ["repositories.read", "repositories.write"]],
                   "maxRequestBytes": 1048576, "maxResponseBytes": 16777216}
    sql(f"insert into connections(id,org_id,user_id,provider,label,base_url,status,permissions) values"
        f"('{repo}','{org}','{user}','github','Actual writer repository','https://api.github.com','active','{json.dumps(permissions)}');"
        f"insert into projects(id,org_id,slug,name,created_by,repo_connection_id,repo_provider,repo_full_name,repo_web_url,repo_default_branch) values"
        f"('{project}','{org}','pipeline-project','Pipeline project','{user}','{repo}','github','owner/repo','https://github.com/owner/repo','main');"
        f"insert into graphs(id,project_id,slug,name,created_by,auto_repairs) values('{graph}','{project}','graph','Pipeline graph','{user}',0);"
        f"insert into graph_cells(id,graph_id,position,name,kind,variant,description,config) values"
        f"('{source}','{graph}',0,'input','source','ui','Editable string source','{{\"input\":\"value\",\"output_type\":\"String\"}}');")
    vault.state["documents"][f"/v1/secret/data/thirdparty/github/{user}/{repo}"] = {
        "kind": "bearer", "base_url": f"http://127.0.0.1:{upstream.server_port}/base", "token": "local-provider-fixture"}
    policy = {"effects": ["Trace", "Error", "Connector"], "providers": ["github", "openai"],
              "tools": ["read", "write", "bash", "check", "publish", "todo", "ls", "grep", "edit", "lun_build"], "domains": [], "configuredDomains": True}
    api("/api/org/settings/permissions", {"policy": policy})
    model = api("/api/connections/ai", {"provider": "Openai", "api_key": "local-provider-fixture", "base_url": "https://fixture.example.invalid/v1"})
    args = {"project": "pipeline-project", "graph": "graph"}
    api("/api/graph/model", {**args, "connection": model["id"], "model": "gpt-4o-mini"})
    saved=api("/api/graph/cells/add",{**args,"cell_type":"Node","name":"compute","description":"Prefix the source", "config":{"dependencies":["input"],"output_type":"String"}})
    assert saved["generation"]["status"]=="implementing" and saved["generation_notice"] is None,saved
    for _ in range(200):
        api("/api/graph/progress",{**args,"after":0,"wait":0})
        if sql(f"select lode_session_id is not null and not exists(select 1 from graph_generation_requests where graph_id='{graph}') from graphs where id='{graph}'")=="t":break
        time.sleep(.1)
    else:raise AssertionError("saved cell never started the real writer")
    sid = sql(f"select lode_session_id from graphs where id='{graph}'")
    def idle():
        for _ in range(400):
            status, result = exchange(lode + f"/v0/sessions/{sid}", headers={"Authorization": "Bearer fixture-writer"})
            assert status == 200, result
            if result["state"] != "running": return result
            time.sleep(.2)
        raise AssertionError("actual writer did not become idle")
    initial = idle()
    assert initial["state"] == "idle", initial
    assert repository.published == 0
    assert repository.head() == repository.initial
    status, log = exchange(lode + f"/v0/sessions/{sid}/messages?wait=0", headers={"Authorization": "Bearer fixture-writer"})
    results = [result for entry in log["entries"] if entry.get("type") == "tool_results" for result in entry["results"]]
    assert any(result["name"] == "publish" and result["isError"] and "capability_denied" in result["content"] for result in results), results
    assert not any(result["name"] in ["write", "bash", "check"] and result["isError"] for result in results), results
    print("PASS full writer pipeline: saved cell starts queued actual Lode checkout/generation/check; ungranted removal commit denied by real broker", flush=True)
    permissions["scopes"].append({"operation": "repositories.delete", "root": ["owner", "repo", "typednotes", "graph", "keep.sh"], "descendants": False})
    api("/api/connections/permissions", {"connection_id": repo, "permissions": permissions})
    api("/api/graph/implement", {**args, "note": "Publish with the explicitly granted removal", "steer": True})
    final = idle()
    assert final["state"] == "idle", final
    assert repository.published == 1
    status, final_log=exchange(lode+f"/v0/sessions/{sid}/messages?wait=0",headers={"Authorization":"Bearer fixture-writer"})
    assert status==200
    assert any(r["name"]=="lun_build" and not r["isError"] for e in final_log["entries"] if e.get("type")=="tool_results" for r in e["results"]),final_log
    assert broker_fixture.git(repository.repo, "show", "main:Outside.txt").stdout == b"published outside\n"
    assert broker_fixture.git(repository.repo, "cat-file", "-e", "main:typednotes/graph/keep.sh", check=False).returncode != 0
    broker_fixture.git(repository.repo, "fsck", "--strict")
    progress = {}
    for _ in range(400):
        progress = api("/api/graph/progress", {**args, "after": 0, "wait": 0})
        if progress["graph"]["status"] in ["ready", "failed"]: break
        time.sleep(.2)
    assert progress["graph"]["status"] == "ready", progress
    fed = api("/api/graph/cells/feed", {**args, "cell": source, "value": "edited"})
    result = next(n for n in fed["nodes"] if n.get("function") == "compute")["outcome"]
    assert result["output"] == "done:edited", fed
    api("/api/graph/cells/feed", {**args, "cell": source, "value": 17}, ok=False)
    assert "local-provider-fixture" not in json.dumps([initial, final, progress, fed])
    print("PASS full writer pipeline: independent scoped delete, atomic publication, actual Lun adoption/source contracts and editable typed input", flush=True)
    api("/api/graph/implement",{**args,"note":"Try a wrong output type while the caller's String pin remains fixed","steer":True})
    sid=sql(f"select lode_session_id from graphs where id='{graph}'")
    refused=idle()
    assert refused["state"]=="idle",refused
    status, denied_log=exchange(lode+f"/v0/sessions/{sid}/messages?wait=0",headers={"Authorization":"Bearer fixture-writer"})
    builds=[r for e in denied_log["entries"] if e.get("type")=="tool_results" for r in e["results"] if r["name"]=="lun_build"]
    assert status==200 and builds[-1]["isError"] and "failed" in builds[-1]["content"],builds
    print("PASS full writer pipeline: model-written Nat override cannot remove caller String pin; actual writer lun_build reaches kernel-checked output refusal",flush=True)
    upstream.state.pop("pipeline")
