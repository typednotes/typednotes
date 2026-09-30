"""Verify app writer caller shapes; the peer is not a model/broker proof."""
import json
import uuid


def verify_writer(sql, api, writer, vault, org, user):
    policy = {"effects": ["Connector"], "providers": ["github", "openai"], "tools": ["read", "todo"],
              "domains": [], "configuredDomains": True}
    api("/api/org/settings/permissions", {"policy": policy})
    repo, project, graph, cell = [str(uuid.uuid4()) for _ in range(4)]
    permissions = {"scopes": [{"operation": op, "root": [], "descendants": True} for op in ["repositories.read", "repositories.write"]],
                   "maxRequestBytes": 1048576, "maxResponseBytes": 16777216}
    sql(f"insert into connections(id,org_id,user_id,provider,label,base_url,status,permissions) values"
        f"('{repo}','{org}','{user}','github','Writer repository','https://api.github.com','active','{json.dumps(permissions)}');"
        f"insert into projects(id,org_id,slug,name,created_by,repo_connection_id,repo_provider,repo_full_name,repo_web_url,repo_default_branch) values"
        f"('{project}','{org}','writer-project','Writer project','{user}','{repo}','github','fixture/repo','https://github.com/fixture/repo','main');"
        f"insert into graphs(id,project_id,slug,name,created_by) values('{graph}','{project}','writer-graph','Writer graph','{user}');"
        f"insert into graph_cells(id,graph_id,position,name,kind,description,config) values"
        f"('{cell}','{graph}',0,'compute','node','Return a fixture value','{{\"output_type\":\"String\"}}');")
    model = api("/api/connections/ai", {"provider": "Openai", "api_key": "local-provider-fixture", "base_url": "https://fixture.example.invalid/v1"})
    args = {"project": "writer-project", "graph": "writer-graph"}
    api("/api/graph/model", {**args, "connection": model["id"], "model": "gpt-4o-mini"})
    api("/api/graph/implement", {**args, "note": "Implement fixture", "steer": False})
    opened = next(body for method, path, body in writer.state["calls"] if method == "POST" and path == "/v0/sessions")
    assert opened["tools"] == ["read", "todo"]
    assert "message" not in opened
    assert "local-provider-fixture" not in json.dumps(opened)
    assert opened["model"]["credentials"]["warrant"]["caveats"][3]["action"] == "inference.generate"
    model_id = opened["model"]["credentials"]["warrant"]["id"]
    model_projection = next(value for path, value in vault.state["documents"].items() if path.endswith("/" + model_id))
    assert model_projection["conversation"] == {"sessionId": "writer-fixture", "allowedTools": ["read", "todo"]}
    print("PASS app writer caller: organization tools forwarded at launch; request contains no API key", flush=True)
    policy["tools"] = ["read"]
    api("/api/org/settings/permissions", {"policy": policy})
    assert writer.state["tools"] == ["read"]
    api("/api/graph/progress", {**args, "after": 0, "wait": 0})
    puts = [body for method, path, body in writer.state["calls"] if method == "PUT" and path.endswith("/credentials")]
    assert puts and all("tools" not in body for body in puts)
    policy["tools"] = ["read", "todo", "write"]
    api("/api/org/settings/permissions", {"policy": policy})
    api("/api/graph/implement", {**args, "note": "Continue within bounds", "steer": True})
    messages = [body for method, path, body in writer.state["calls"] if method == "POST" and path.endswith("/messages")]
    assert messages[-1]["tools"] == ["read"]
    assert writer.state["tools"] == ["read"]
    print("PASS app writer caller: acknowledged narrowing, credentials-only PUT, and widening cannot restore removed tools", flush=True)
