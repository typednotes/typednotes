"""Actual Dioxus/browser/API/SQL project-note workflows with local service peers."""
from playwright.sync_api import expect
import json
import time
import re

def verify_project_notes(page,context,app,output,sql,state,user,token,org,project,graph,github):
    context.add_cookies([{"name":"tn_session","value":token,"url":app}])
    errors=[];page.on("pageerror",lambda e:errors.append(str(e)))
    url=app+"/orgs/notebook-fixture/projects/sheets"
    def passed(text):print("PASS project notes:",text,flush=True)
    def api(path,body):
        response=page.request.post(app+path,data={"slug":"notebook-fixture","project":"sheets",**body})
        assert response.ok,(response.status,response.text());return response.json()
    def wait_ready():
        deadline=time.monotonic()+45
        while time.monotonic()<deadline:
            api("/api/graph/progress",{"graph":"lifecycle","after":0,"wait":0})
            if sql(f"select status from graphs where id='{graph}'")=="ready":break
            page.wait_for_timeout(100)
        else:raise AssertionError("automatic generation did not become ready")
        page.reload(wait_until="networkidle")
    page.goto(url,wait_until="networkidle")
    expect(page.get_by_text("Project notes",exact=True)).to_be_visible()
    expect(page.get_by_role("button",name="Add note",exact=True)).to_be_visible()
    expect(page.get_by_role("heading",name="New notebook",exact=True)).to_have_count(0)
    passed("notes are defined directly in the project; no notebook creation layer")
    for name,kind,deps,ty in [("tax","Source note · input","","Nat"),("amount","Source note · input","","Nat"),("total","Note · calculation","amount, tax","Nat"),("view","Sink note · project output","total","String")]:
        if page.locator("details.nb-add-cell").get_attribute("open") is None:page.locator("details.nb-add-cell > summary").click()
        page.get_by_role("button",name="Kind of note",exact=True).click()
        page.get_by_role("option",name=kind,exact=True).click()
        page.get_by_label("Name",exact=True).fill(name)
        page.get_by_label("Description",exact=True).fill("Project note "+name)
        if deps:page.get_by_label("Input notes (in argument order)",exact=True).fill(deps)
        page.get_by_label("Output type (optional Lean 4 constraint)",exact=True).fill(ty)
        page.get_by_role("button",name="Add note",exact=True).click()
        wait_ready()
    assert any(e[0]=="writer" and e[1]=="POST" and e[2]=="/v0/sessions" for e in state["log"])
    expect(page.get_by_text("Code activity",exact=True)).to_be_visible()
    expect(page.locator(".cell-connections")).to_have_count(0)
    passed("note saves start background code generation; actual progress is visible; note-level permission forms are gone")
    before=sql(f"select note_connections::text from projects where id='{project}'")
    refused=page.request.post(app+"/api/graph/cells/update",data={"slug":"notebook-fixture","project":"sheets","graph":"lifecycle","cell":sql("select id::text from graph_cells where name='total'"),"name":"total","description":"attempt unauthorized note permission","config":{"dependencies":["amount","tax"],"connectors":[{"connection":github}]}})
    assert not refused.ok and sql(f"select note_connections::text from projects where id='{project}'")==before
    passed("individual-note permission changes are refused rather than widening the project's authority")
    page.goto(url+"/settings/permissions",wait_until="networkidle")
    expect(page.get_by_text("Project note permissions",exact=True)).to_be_visible()
    page.get_by_text("Project connections (0)",exact=True).click()
    page.get_by_role("button",name=re.compile("Reports fixture")).click()
    page.get_by_role("button",name="Save project permissions",exact=True).click()
    expect(page.get_by_text("Project permissions saved.",exact=True)).to_be_visible()
    assert json.loads(sql(f"select note_connections::text from projects where id='{project}'"))
    passed("project operation/resource grants persist through the real API and close old writer authority")
    page.goto(url+"/settings/repository",wait_until="networkidle")
    page.get_by_role("button",name="Change",exact=True).click()
    for width in [390,641,760,900,1280]:
        page.set_viewport_size({"width":width,"height":900})
        page.wait_for_timeout(50)
        boxes=page.locator(".repo-picker .conn-grid > .orgs-field").evaluate_all("els=>els.map(e=>{let r=e.getBoundingClientRect();return {x:r.x,y:r.y,right:r.right,bottom:r.bottom}})")
        for i,a in enumerate(boxes):
            for b in boxes[i+1:]:assert not (a["x"]<b["right"] and b["x"]<a["right"] and a["y"]<b["bottom"] and b["y"]<a["bottom"]),boxes
        assert page.evaluate("document.documentElement.scrollWidth<=window.innerWidth"),width
    page.set_viewport_size({"width":1280,"height":900})
    passed("repository controls do not overlap or overflow at mobile, breakpoint and desktop widths")
    page.goto(url+"/settings/model",wait_until="networkidle")
    expect(page.get_by_role("button",name="AI connection",exact=True)).to_be_visible()
    expect(page.get_by_role("button",name="Provider model",exact=True)).to_be_visible()
    expect(page.get_by_role("button",name="Refresh models",exact=True)).to_have_count(0)
    expect(page.locator(".project-model-picker")).to_be_visible()
    passed("project model controls share an aligned layout and load their catalog automatically")
    # Persist caller-owned state through actual private/public APIs. Repeated
    # updates reuse state but send fresh independent execution ceilings.
    page.goto(url,wait_until="networkidle")
    amount=sql("select id::text from graph_cells where name='amount'")
    tax=sql("select id::text from graph_cells where name='tax'")
    api("/api/graph/cells/feed",{"graph":"lifecycle","cell":amount,"value":10})
    fed=api("/api/graph/cells/feed",{"graph":"lifecycle","cell":tax,"value":2})
    assert next(n for n in fed["nodes"] if n.get("function")=="total")["outcome"]["output"]==12,fed
    private_state=json.loads(sql(f"select runtime_state::text from graphs where id='{graph}'"))
    assert private_state["inputs"]=={"amount":10,"tax":2}
    requests=[e for e in state["log"] if e[0]=="runtime" and e[1]=="POST" and "/graphs/" in e[2]]
    assert requests[-1][3]["state"] and requests[-1][3]["binding"]["user_id"]==user
    assert all(not e[2].endswith("/sessions") for e in requests)
    detail=api("/api/graph",{"graph":"lifecycle"})
    assert "runtime_state" not in json.dumps(detail)
    passed("private feeds persist caller-owned state and use stateless graph steps with fresh actor-bound authority")
    share=api("/api/notebook/share",{"graph":"lifecycle"})
    token=share["url"].rsplit("/",1)[1]
    public=context.browser.new_context();other=context.browser.new_context()
    try:
        for client in [public,other]:
            assert client.request.post(app+"/api/shared-notebook",data={"token":token}).ok
        response=public.request.post(app+"/api/shared-notebook/input",data={"token":token,"cell":amount,"value":20})
        assert response.ok,response.text()
        assert next(n for n in response.json()["nodes"] if n.get("function")=="total")["outcome"]["output"]==22
        viewed=other.request.post(app+"/api/shared-notebook",data={"token":token}).json()
        assert viewed["inputs"]["amount"]==10
        assert json.loads(sql(f"select runtime_state::text from graphs where id='{graph}'"))==private_state
        assert "state" not in response.json() and "connectors" not in response.text()
        before=json.loads(sql("select runtime_state::text from notebook_share_sessions where runtime_state is not null limit 1"))
        state["reject_share_runtime"]=True
        refused=public.request.post(app+"/api/shared-notebook/input",data={"token":token,"cell":amount,"value":"invalid"})
        state["reject_share_runtime"]=False
        assert refused.status==502 and json.loads(sql("select runtime_state::text from notebook_share_sessions where runtime_state is not null limit 1"))==before
        api("/api/notebook/share/revoke",{"graph":"lifecycle","share":share["id"]})
        assert public.request.post(app+"/api/shared-notebook",data={"token":token}).status==404
    finally:public.close();other.close()
    passed("public stateless inputs isolate each visitor and owner, hide runtime state, retain state on refusal, and revoke links")
    state["running"]=True
    api("/api/graph/implement",{"graph":"lifecycle","note":"Ask before rounding","steer":False})
    state["question"]={"id":"rounding-question","question":{"text":"How should prices be rounded?","options":["up","down"],"freeText":False}}
    page.reload(wait_until="networkidle")
    expect(page.get_by_text("How should prices be rounded?",exact=True)).to_be_visible(timeout=20000)
    assert sql(f"select status from graphs where id='{graph}'")=="implementing"
    stranger="00000000-0000-4000-8000-000000000078";stranger_token="question-other-user"
    sql(f"insert into users(id,email) values('{stranger}','question-other@example.invalid');insert into memberships(org_id,user_id,role) values('{org}','{stranger}','member');insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{stranger_token}','UTF8')),'{stranger}',now()+interval '1 hour')")
    foreign=context.browser.new_context();foreign.add_cookies([{"name":"tn_session","value":stranger_token,"url":app}])
    try:
        denied=foreign.request.post(app+"/api/project/answer",data={"slug":"notebook-fixture","project":"sheets","question":"rounding-question","answer":"up"})
        assert denied.status==403,denied.text()
    finally:foreign.close()
    page.get_by_role("button",name="up",exact=True).click()
    page.get_by_role("button",name="Answer and continue",exact=True).click()
    wait_ready()
    assert state["question"] is None
    passed("writer questions pause adoption; the initiating actor answers through the real UI/API before bounded generation continues")
    from outbox_cases import verify_outbox
    verify_outbox(page,app,sql,state,user,org,project,graph)
    from scheduling_cases import verify_scheduling
    verify_scheduling(page,app,sql,state,user,graph)
    new="00000000-0000-4000-8000-000000000077";fresh_token="project-notes-first-user"
    sql(f"insert into users(id,email) values('{new}','fresh-project@example.invalid'); insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{fresh_token}','UTF8')),'{new}',now()+interval '1 hour')")
    assert sql(f"select count(*) from memberships where user_id='{new}'")=="0"
    fresh=context.browser.new_context(viewport={"width":900,"height":800});fresh.add_cookies([{"name":"tn_session","value":fresh_token,"url":app}]);p=fresh.new_page()
    p.goto(app+"/",wait_until="networkidle")
    expect(p).to_have_url(app+"/organizations")
    expect(p.get_by_text("Connect a repository and AI account",exact=True)).to_be_visible()
    p.get_by_role("button",name="Skip and create organization/project manually",exact=True).click()
    expect(p.get_by_text("Connect a repository and AI account",exact=True)).to_have_count(0)
    p.reload(wait_until="networkidle")
    expect(p.get_by_text("Connect a repository and AI account",exact=True)).to_have_count(0)
    assert sql(f"select count(*) from memberships where user_id='{new}'")=="0"
    fresh.close()
    passed("first sign-in asks for repository/AI setup; skip persists and creates no organization or project")
    repo_user="00000000-0000-4000-8000-000000000079";repo_token="repository-first-user"
    sql(f"insert into users(id,email) values('{repo_user}','repo-first@example.invalid');insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{repo_token}','UTF8')),'{repo_user}',now()+interval '1 hour')")
    first=context.browser.new_context();first.add_cookies([{"name":"tn_session","value":repo_token,"url":app}]);p=first.new_page()
    try:
        p.goto(app+"/",wait_until="networkidle")
        p.get_by_role("button",name="Connect GitHub/GitLab and AI",exact=True).click()
        expect(p).to_have_url(re.compile(r"/orgs/setup-.*/settings/connections"))
        provision=first.request.post(app+"/api/setup/begin",data={})
        assert provision.ok,provision.text()
        setup_org=provision.json();setup_id=setup_org["id"]
        assert sql(f"select count(*) from memberships where user_id='{repo_user}'")=="1"
        code="00000000-0000-4000-8000-000000000080"
        sql(f"insert into connections(id,org_id,user_id,provider,label,base_url,status) values('{code}','{setup_id}','{repo_user}','github','First repo','https://api.github.com','active')")
        ai=first.request.post(app+"/api/connections/ai",data={"slug":setup_org["slug"],"provider":"Anthropic","api_key":"test-fixture-key","base_url":""})
        assert ai.ok,ai.text()
        p.reload(wait_until="networkidle")
        picker=p.locator(".repo-picker").filter(has=p.get_by_role("button",name="Create project from repository",exact=True))
        picker.get_by_role("button",name="Code connection",exact=True).click()
        p.get_by_role("option",name="GitHub First repo",exact=True).click()
        picker.get_by_role("button",name="Repository",exact=True).click()
        p.get_by_role("option",name="other-org/repo-000",exact=True).click()
        picker.get_by_role("button",name="Create project from repository",exact=True).click()
        expect(p).to_have_url(app+"/orgs/other-org/projects/repo-000/settings/model")
        assert sql(f"select name from orgs where id='{setup_id}'")=="other-org"
        assert sql(f"select name from projects where org_id='{setup_id}'")=="repo-000"
        retried=first.request.post(app+"/api/setup/repository",data={"slug":"other-org","connection":code,"repository":"other-org/repo-000"})
        assert retried.ok,retried.text()
        assert sql(f"select count(*) from projects where org_id='{setup_id}'")=="1"
    finally:first.close()
    passed("repository/AI-first setup derives org/project names, preserves connections across the rename, and retries without duplicate projects")
    assert not errors,errors
    passed("no browser JavaScript errors")
    page.screenshot(path=str(output/"project-notes.png"),full_page=True)
