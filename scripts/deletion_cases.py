"""Deletion regressions used by the real app/browser/PostgreSQL fixture.

All state is disposable. This module never accesses an existing database or
credential and exercises HTTP, SQL cascades and actual compute-role teardown.
"""
from typing import Any
import json
import subprocess
import uuid
from concurrent.futures import ThreadPoolExecutor


def verify_deletion(page, context, app, sql, state, passed):
    from playwright.sync_api import expect
    uid = lambda: str(uuid.uuid4())
    primary = sql("select id from users where email='notebook@example.invalid'")
    email = "delete-account@example.invalid"
    target, token = uid(), "delete-account-fixture"
    sql(f"insert into users(id,email) values('{target}','{email}');"
        f"insert into identities(user_id,issuer,subject) values('{target}','https://github.com','delete-account-subject');"
        f"insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{token}','UTF8')),'{target}',now()+interval '1 day')")
    browser = context.browser.new_context()
    browser.add_cookies([{"name":"tn_session","value":token,"url":app}])
    actor_page = browser.new_page()
    actor_errors = []
    actor_page.on("pageerror", lambda error: actor_errors.append(str(error)))

    def api(client, path, fields, status=200) -> Any:
        reply = client.request.post(app + path, data=fields)
        assert reply.status == status, (path, reply.status, reply.text())
        return reply.json() if reply.text() else None

    def org(client, slug):
        return api(client, "/api/orgs", {"slug":slug,"name":slug})

    def project(client, slug, name):
        return api(client, "/api/projects/create", {"slug":slug,"project":name,"name":name})

    def graph(project_id, owner, name):
        result = uid()
        sql(f"insert into graphs(id,project_id,slug,name,created_by,status) values('{result}','{project_id}','{name}','{name}','{owner}','ready');"
            f"insert into graph_cells(graph_id,position,name,kind,variant,description,config) values('{result}',0,'key','source','secret','private fixture','{{\"name\":\"key\",\"set_at\":\"fixture\"}}')")
        return result

    def connection(org_id, owner, provider="s3"):
        result = uid()
        sql(f"insert into connections(id,org_id,user_id,provider,label,base_url,status) values('{result}','{org_id}','{owner}','{provider}','Delete fixture','https://fixture.example.invalid','active')")
        path = f"/v1/secret/data/thirdparty/{provider}/{owner}/{result}"
        state["vault"][path] = {"fixture":"private-key"}
        state["vault"][path+"/permissions"] = {"scopes":[]}
        return result, path

    def billing(org_id, owner):
        event = uid()
        sql(f"insert into usage_events(id,org_id,user_id,event_type,provider,idempotency_key) values('{event}','{org_id}','{owner}','fixture','fixture','{event}');"
            f"insert into credit_ledger(org_id,delta,reason,usage_event) values('{org_id}',-1,'fixture','{event}');"
            f"insert into credit_holds(org_id,run_id,amount,state,expires_at) values('{org_id}','{uid()}',1,'held',now()+interval '1 hour')")
        return event

    def workspace(owner, org_id, project_id, graph_id):
        sql(f"insert into user_workspaces(user_id,org_id,project_id,graph_id,onboarded_at) values('{owner}','{org_id}','{project_id}','{graph_id}',now()) "
            "on conflict(user_id) do update set org_id=excluded.org_id,project_id=excluded.project_id,graph_id=excluded.graph_id,onboarded_at=excluded.onboarded_at")

    removed = org(page, "delete-org-fixture")
    api(page, "/api/members/add", {"slug":removed["slug"],"email":email,"role":"member"})
    p = project(page, removed["slug"], "owned-project")
    g = graph(p["id"], primary, "deleted-graph")
    conn, path = connection(removed["id"], primary, "signal")
    channel = uid()
    sql(f"insert into channels(id,project_id,connection_id,provider,external_id,label) values('{channel}','{p['id']}','{conn}','signal','fixture-sender','fixture');"
        f"insert into channel_messages(channel_id,direction,peer,body) values('{channel}','in','fixture','private fixture');"
        f"insert into graph_inputs(graph_id,input,value,fed_by) values('{g}','x','1','ui')")
    billing(removed["id"], primary)
    share, runtime = uid(), "deleted-public-session"
    secret_path = f"/v1/secret/data/graph/{removed['id']}/{g}/key"
    state["vault"][secret_path] = {"fixture":"private-value"}
    state["sessions"][runtime] = {"inputs":{},"start":{}}
    sql(f"insert into notebook_shares(id,graph_id,owner_id,token_hash,build_id,snapshot,initial_inputs) values('{share}','{g}','{primary}',sha256(convert_to('deleted-share','UTF8')),'B1','{{}}','{{}}');"
        f"insert into notebook_share_sessions(id_hash,share_id,lun_session_id,inputs,nodes,expires_at) values(sha256(convert_to('deleted-viewer','UTF8')),'{share}','{runtime}','{{}}','[]',now()+interval '1 hour')")
    workspace(target, removed["id"], p["id"], g)
    workspace(primary, removed["id"], p["id"], g)
    before = len(state["log"])
    api(browser, "/api/org/delete", {"slug":removed["slug"],"confirm":removed["slug"]}, 403)
    api(page, "/api/org/delete", {"slug":removed["slug"],"confirm":"wrong"}, 400)
    assert len(state["log"]) == before
    passed("organization deletion requires its current owner and exact confirmation before external cleanup")

    # Reproduce the reported old-ledger FK, but refuse before deleting any key.
    sql("alter table credit_ledger drop constraint credit_ledger_org_id_fkey;"
        "alter table credit_ledger add constraint credit_ledger_org_id_fkey foreign key(org_id) references orgs(id)")
    before = len(state["log"])
    api(page, "/api/org/delete", {"slug":removed["slug"],"confirm":removed["slug"]}, 503)
    assert len(state["log"]) == before and path in state["vault"]
    sql("alter table credit_ledger drop constraint credit_ledger_org_id_fkey;"
        "alter table credit_ledger add constraint credit_ledger_org_id_fkey foreign key(org_id) references orgs(id) on delete cascade")
    passed("old ledger FK migration is detected before teardown rather than leaving deleted credentials behind a database error")

    api(page, "/api/org/delete", {"slug":removed["slug"],"confirm":removed["slug"]})
    for table, column, key in [("orgs","id",removed["id"]), ("projects","id",p["id"]),
                               ("graphs","id",g), ("connections","id",conn), ("graph_cells","graph_id",g),
                               ("graph_inputs","graph_id",g), ("channels","id",channel),
                               ("channel_messages","channel_id",channel), ("usage_events","org_id",removed["id"]),
                               ("credit_ledger","org_id",removed["id"]), ("credit_holds","org_id",removed["id"]),
                               ("notebook_shares","id",share), ("notebook_share_sessions","share_id",share)]:
        assert sql(f"select count(*) from {table} where {column}='{key}'") == "0", table
    assert path not in state["vault"] and path+"/permissions" not in state["vault"] and secret_path not in state["vault"]
    assert runtime not in state["sessions"]
    w = api(browser, "/api/workspace", {})
    assert w["org"] is None and w["project"] is None and w["notebook"] is None and w["setup_required"]
    assert sql(f"select org_id is null and project_id is null and graph_id is null and onboarded_at is null from user_workspaces where user_id='{target}'") == "t"
    actor_page.goto(app + "/", wait_until="networkidle")
    expect(actor_page).to_have_url(app + "/onboarding")
    expect(actor_page.get_by_role("heading", name="Your default workspace")).to_be_visible()
    passed("organization deletion cascades real ledger/project/connection/share rows, removes private resources and sends org-less members to empty onboarding")

    shared = org(browser, "surviving-org-fixture")
    solo = org(browser, "empty-org-fixture")
    api(browser, "/api/members/add", {"slug":shared["slug"],"email":"notebook@example.invalid","role":"member"})
    own = project(browser, shared["slug"], "removed-owner-project")
    own_graph = graph(own["id"], target, "owned-graph")
    other = project(page, shared["slug"], "surviving-owner-project")
    other_graph = graph(other["id"], primary, "other-graph")
    solo_project = project(browser, solo["slug"], "solo-project")
    solo_graph = graph(solo_project["id"], target, "solo-graph")
    owned_conn, owned_path = connection(shared["id"], target)
    solo_conn, solo_path = connection(solo["id"], target)
    for o, notebook in [(shared, own_graph), (solo, solo_graph), (shared, other_graph)]:
        state["vault"][f"/v1/secret/data/graph/{o['id']}/{notebook}/key"] = {"fixture":"private-value"}
    surviving_event = billing(shared["id"], target)
    billing(solo["id"], target)
    workspace(primary, shared["id"], own["id"], own_graph)
    workspace(target, shared["id"], own["id"], own_graph)
    before = len(state["log"])
    api(browser, "/api/account/delete", {"confirm":"someone-else@example.invalid"}, 400)
    api(browser, "/api/account/delete", {"confirm":email}, 409)
    assert len(state["log"]) == before and owned_path in state["vault"]
    api(page, "/api/members/role", {"slug":shared["slug"],"user_id":primary,"role":"owner"}, 403)
    api(browser, "/api/members/role", {"slug":shared["slug"],"user_id":target,"role":"member"}, 409)
    try:
        sql(f"update memberships set role='member' where org_id='{shared['id']}' and user_id='{target}'")
    except subprocess.CalledProcessError as refused:
        assert "transfer organization ownership" in refused.stderr
    else:
        raise AssertionError("database accepted an ownerless membership transition")
    assert sql(f"select role from memberships where org_id='{shared['id']}' and user_id='{target}'") == "owner"
    passed("account deletion rejects wrong confirmation and last-owner orphaning without cleanup; members cannot self-promote")

    actor_page.goto(app + f"/orgs/{shared['slug']}/settings/members", wait_until="networkidle")
    actor_page.get_by_role("button", name="Make owner", exact=True).click()
    expect(actor_page.get_by_role("button", name="Make owner", exact=True)).to_have_count(0)
    assert sql(f"select role from memberships where org_id='{shared['id']}' and user_id='{primary}'") == "owner"
    passed("existing members can become owners through the UI so ownership transfer unblocks account deletion")

    # Compute resources disappear in a separate database transaction. Force the
    # later org's key cleanup to fail, then verify retry works with an absent role.
    first, last = sorted([shared, solo], key=lambda o: o["id"])
    role = "deletion_compute_fixture"
    sql(f'create role "{role}";create schema "{role}" authorization "{role}";'
        f'create table "{role}".private_data(value text);'
        f"insert into compute_schemas(org_id,user_id,name) values('{first['id']}','{target}','{role}')")
    state["vault"][f"/v1/secret/data/compute/{first['id']}/{target}"] = {"fixture":"private-db-key"}
    state["vault_fail_delete"] = owned_path if last["id"] == shared["id"] else solo_path
    api(browser, "/api/account/delete", {"confirm":email}, 502)
    assert sql(f"select count(*) from users where id='{target}'") == "1"
    assert sql(f"select count(*) from projects where id='{own['id']}'") == "1"
    assert sql(f"select count(*) from compute_schemas where user_id='{target}'") == "1"
    assert sql(f"select count(*) from pg_roles where rolname='{role}'") == "0"
    state.pop("vault_fail_delete")
    passed("distributed cleanup failure retains SQL/account state; already-removed compute roles are safely retryable")

    actor_page.goto(app + "/settings", wait_until="networkidle")
    button = actor_page.get_by_role("button", name="Delete my account permanently", exact=True)
    expect(button).to_be_disabled()
    actor_page.locator("#delete-account-confirm").fill(email)
    button.click()
    expect(actor_page.get_by_text("Sign in to Typednotes", exact=True)).to_be_visible()
    for table, column, key in [("users","id",target), ("identities","user_id",target),
                               ("sessions","user_id",target), ("memberships","user_id",target),
                               ("user_workspaces","user_id",target), ("projects","created_by",target),
                               ("graphs","id",own_graph), ("graphs","id",solo_graph), ("connections","user_id",target),
                               ("orgs","id",solo["id"]), ("credit_ledger","org_id",solo["id"]),
                               ("credit_holds","org_id",solo["id"]), ("usage_events","org_id",solo["id"]),
                               ("compute_schemas","user_id",target)]:
        assert sql(f"select count(*) from {table} where {column}='{key}'") == "0", table
    assert sql(f"select count(*) from orgs where id='{shared['id']}'") == "1"
    assert sql(f"select count(*) from graphs where id='{other_graph}'") == "1"
    assert sql(f"select user_id is null from usage_events where id='{surviving_event}'") == "t"
    assert sql(f"select count(*) from graphs where slug='lifecycle'") == "1"
    assert sql(f"select count(*) from orgs where slug='notebook-fixture'") == "1"
    assert owned_path not in state["vault"] and solo_path not in state["vault"]
    assert f"/v1/secret/data/graph/{shared['id']}/{other_graph}/key" in state["vault"]
    assert sql(f"select org_id::text='{shared['id']}' and project_id is null and graph_id is null and onboarded_at is null from user_workspaces where user_id='{primary}'") == "t"
    old = context.browser.new_context()
    old.add_cookies([{"name":"tn_session","value":token,"url":app}])
    assert old.request.get(app + "/api/me").json() is None
    api(old, "/api/account/delete", {"confirm":email}, 401)
    old.close()
    assert not actor_errors, actor_errors
    browser.close()
    passed("account UI deletes ownerless projects and empty orgs, preserves other owners/tenants and shared billing, clears dependent defaults and invalidates every session")

    # Both owners can delete their accounts concurrently. The first leaves a
    # valid owner; the second removes the now-empty organization in its transaction.
    from urllib.request import Request, urlopen
    concurrent = []
    for i in range(2):
        user, cookie, address = uid(), f"concurrent-delete-{i}", f"concurrent-delete-{i}@example.invalid"
        sql(f"insert into users(id,email) values('{user}','{address}');"
            f"insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{cookie}','UTF8')),'{user}',now()+interval '1 hour')")
        client = context.browser.new_context()
        client.add_cookies([{"name":"tn_session","value":cookie,"url":app}])
        concurrent.append((user,cookie,address,client))
    coowned = org(concurrent[0][3], "concurrent-delete-org")
    api(concurrent[0][3], "/api/members/add", {"slug":coowned["slug"],"email":concurrent[1][2],"role":"owner"})
    def delete(item):
        _, cookie, address, _ = item
        request = Request(app + "/api/account/delete", data=json.dumps({"confirm":address}).encode(),
            headers={"Cookie":f"tn_session={cookie}","Content-Type":"application/json"})
        with urlopen(request, timeout=30) as reply:
            return reply.status
    with ThreadPoolExecutor(max_workers=2) as executor:
        assert list(executor.map(delete, concurrent)) == [200,200]
    assert sql(f"select count(*) from orgs where id='{coowned['id']}'") == "0"
    for user, _, _, client in concurrent:
        assert sql(f"select count(*) from users where id='{user}'") == "0"
        client.close()
    passed("concurrent co-owner account deletions serialize ownership checks and remove the final empty organization")
