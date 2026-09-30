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
        assert set(names) == {"read", "write", "bash", "check", "publish", "todo"}, names
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
        f"('{source}','{graph}',0,'input','source','ui','Editable string source','{{\"input\":\"value\",\"output_type\":\"String\"}}'),"
        f"('{node}','{graph}',1,'compute','node',null,'Prefix the source','{{\"dependencies\":[\"input\"],\"output_type\":\"String\"}}');")
    vault.state["documents"][f"/v1/secret/data/thirdparty/github/{user}/{repo}"] = {
        "kind": "bearer", "base_url": f"http://127.0.0.1:{upstream.server_port}/base", "token": "local-provider-fixture"}
    policy = {"effects": ["Trace", "Error", "Connector"], "providers": ["github", "openai"],
              "tools": ["read", "write", "bash", "check", "publish", "todo"], "domains": [], "configuredDomains": True}
    api("/api/org/settings/permissions", {"policy": policy})
    model = api("/api/connections/ai", {"provider": "Openai", "api_key": "local-provider-fixture", "base_url": "https://fixture.example.invalid/v1"})
    args = {"project": "pipeline-project", "graph": "graph"}
    api("/api/graph/model", {**args, "connection": model["id"], "model": "gpt-4o-mini"})
    started = api("/api/graph/implement", {**args, "note": "Implement the fixture project", "steer": False})
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
    print("PASS full writer pipeline: actual Lode checkout/generation/check; ungranted removal commit denied by real broker", flush=True)
    permissions["scopes"].append({"operation": "repositories.delete", "root": ["owner", "repo", "typednotes", "graph", "keep.sh"], "descendants": False})
    api("/api/connections/permissions", {"connection_id": repo, "permissions": permissions})
    api("/api/graph/implement", {**args, "note": "Publish with the explicitly granted removal", "steer": True})
    final = idle()
    assert final["state"] == "idle", final
    assert repository.published == 1
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
    upstream.state.pop("pipeline")
