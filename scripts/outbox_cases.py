"""Actual app/SQL ordered sink delivery, persistence, and ambiguous send handling."""
import json,uuid

def verify_outbox(page,app,sql,state,user,org,project,graph):
    args={"slug":"notebook-fixture","project":"sheets","graph":"lifecycle"}
    def api(path,extra):
        response=page.request.post(app+path,data={**args,**extra})
        assert response.ok,response.text()
        return response.json()
    slack=sql("select id::text from connections where provider='slack' and org_id='"+org+"'")
    channel=str(uuid.uuid4());note=str(uuid.uuid4())
    permission={"scopes":[{"operation":"messages.send","root":["C-fixture"],"descendants":False}],"maxRequestBytes":1048576,"maxResponseBytes":16777216}
    api("/api/connections/permissions",{"connection_id":slack,"permissions":permission})
    grants=json.loads(sql(f"select note_connections from projects where id='{project}'"))
    grants.append({"connection":slack,"permissions":permission})
    api("/api/project/permissions",{"grants":grants})
    config={"channel_id":channel,"dependencies":["total"],"output_type":"String"}
    impl={"function":"announce","signature":"Nat → Eff [] String","effects":[]}
    sql(f"insert into channels(id,project_id,connection_id,provider,external_id,external_channel,label) values('{channel}','{project}','{slack}','slack','T-fixture','C-fixture','Outbox fixture');insert into graph_cells(id,graph_id,position,name,kind,variant,description,config,impl) values('{note}','{graph}',10,'announce','sink','channel','Publish every emission','{json.dumps(config)}','{json.dumps(impl)}')")
    amount=sql("select id::text from graph_cells where name='amount'")
    state["channel_burst"]=[10,11,12]
    api("/api/graph/cells/feed",{"cell":amount,"value":13})
    assert state["sent_texts"][-3:]==["10","11","12"],state.get("sent_texts")
    assert sql(f"select count(*) from graph_delivery_outbox where cell_id='{note}' and state='sent'")=="3"
    assert json.loads(sql(f"select json_agg(body order by runtime_revision,emission_index) from graph_delivery_outbox where cell_id='{note}'"))==["10","11","12"]
    print("PASS outbox: every burst emission is persisted with graph state and delivered in order, including repeated node IDs",flush=True)
    state["channel_burst"]=[20,21,22];state["drop_delivery_response"]=True
    try:api("/api/graph/cells/feed",{"cell":amount,"value":14})
    finally:state["drop_delivery_response"]=False
    assert state["sent_texts"][-1]=="20"
    assert sql(f"select count(*) from graph_delivery_outbox where cell_id='{note}' and state='uncertain'")=="1"
    assert sql(f"select count(*) from graph_delivery_outbox where cell_id='{note}' and state='pending'")=="2"
    count=len(state["sent_texts"]);page.wait_for_timeout(32000)
    assert len(state["sent_texts"])==count
    print("PASS outbox: a lost send response is uncertain, blocks later emissions, and is never automatically resent",flush=True)
    api("/api/graph/restart",{})
    assert sql(f"select count(*) from graph_delivery_outbox where cell_id='{note}' and state in ('pending','uncertain','sending')")=="0"
    sql(f"delete from graph_cells where id='{note}';delete from channels where id='{channel}'")
    api("/api/project/permissions",{"grants":[g for g in grants if g['connection']!=slack]})
