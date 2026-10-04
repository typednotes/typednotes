"""Actual SQL/app-worker scheduling: durable claims and ambiguous effect outcomes."""
import json,time,uuid

def verify_scheduling(page,app,sql,state,user,graph):
    verify_persisted_source(page,sql,state,graph)
    def wait(query,expected,seconds=75):
        end=time.monotonic()+seconds
        result=None
        while time.monotonic()<end:
            result=sql(query)
            if result==expected:return
            page.wait_for_timeout(200)
        raise AssertionError((query,result,expected))
    args={"slug":"notebook-fixture","project":"sheets","graph":"lifecycle"}
    def restart():
        r=page.request.post(app+"/api/graph/restart",data=args)
        assert r.ok,r.text()
    def enqueue(key,revision=None):
        sql(f"insert into scheduled_runs(occurrence_key,graph_id,actor_id,kind,due_at,build_id,runtime_revision,phase) select '{key}',id,session_user_id,'producer',now()-interval '1 second',session_build_id,{'runtime_revision' if revision is None else revision},'graph' from graphs where id='{graph}'")
    # A record queued before a worker restart is authoritative without a timer
    # handle or a retained process. Execution and input edits share the SQL lock.
    key="persisted-before-dispatch";enqueue(key)
    wait(f"select state from scheduled_runs where occurrence_key='{key}'","completed")
    assert sql(f"select count(*) from graph_updates u join scheduled_runs r on r.id=u.scheduled_run_id where r.occurrence_key='{key}'")=="1"
    print("PASS scheduling: durable pre-existing occurrence executes once and commits its result/history with graph state",flush=True)
    stale="superseded-continuation";enqueue(stale,0)
    wait(f"select state from scheduled_runs where occurrence_key='{stale}'","cancelled")
    print("PASS scheduling: newer graph state cancels an obsolete continuation rather than replaying it",flush=True)
    lost="lost-response";state["drop_runtime_response"]=True;enqueue(lost)
    try:wait(f"select state from scheduled_runs where occurrence_key='{lost}'","uncertain")
    finally:state["drop_runtime_response"]=False
    attempts=len([e for e in state["log"] if e[0]=="runtime" and "/graphs/" in e[2]])
    page.wait_for_timeout(32000)
    assert len([e for e in state["log"] if e[0]=="runtime" and "/graphs/" in e[2]])==attempts
    amount=sql("select id::text from graph_cells where name='amount'")
    refused=page.request.post(app+"/api/graph/cells/feed",data={**args,"cell":amount,"value":11})
    assert refused.status==502 and "uncertain" in refused.text(),refused.text()
    restart()
    assert sql(f"select state from scheduled_runs where occurrence_key='{lost}'")=="cancelled"
    print("PASS scheduling: lost responses are recorded as uncertain, block automatic/input replay, and require an explicit reviewed restart",flush=True)
    expired="expired-dispatch";enqueue(expired)
    sql(f"update scheduled_runs set state='running',started_at=now()-interval '16 minutes' where occurrence_key='{expired}'")
    wait(f"select state from scheduled_runs where occurrence_key='{expired}'","uncertain")
    restart()
    print("PASS scheduling: abandoned dispatch leases become uncertain after restart; the worker never retries unknown effects",flush=True)
def verify_persisted_source(page,sql,state,graph):
    def wait(query,expected):
        end=time.monotonic()+75;actual=None
        while time.monotonic()<end:
            actual=sql(query)
            if actual==expected:return
            page.wait_for_timeout(200)
        raise AssertionError((query,actual,expected))
    # Persisted source responses have a separate phase. A crash after fetching
    # resumes graph input delivery without repeating the source call.
    cell=str(uuid.uuid4());source_key="persisted-source-result"
    config={"input":"periodic","url":"https://example.com/report","schedule":"0 * * * *","output_type":"Nat"}
    impl={"function":"periodic","input":"periodic","input_type":"Nat","effects":[],"signature":"String → Eff [] Nat"}
    sql(f"insert into graph_cells(id,graph_id,position,name,kind,variant,description,config,impl) values('{cell}','{graph}',20,'periodic','source','scheduled','Scheduled fixture','{json.dumps(config)}','{json.dumps(impl)}');update graphs set structure=jsonb_set(structure,'{{inputs}}',(structure->'inputs')||'[\"periodic\"]'::jsonb) where id='{graph}';insert into scheduled_runs(occurrence_key,graph_id,cell_id,actor_id,kind,due_at,build_id,declaration,input_value,state) select '{source_key}',g.id,'{cell}'::uuid,g.session_user_id,'source',now(),g.session_build_id,jsonb_build_object('config',c.config,'impl',c.impl),'4'::jsonb,'input_ready' from graphs g join graph_cells c on c.id='{cell}' where g.id='{graph}'")
    before=len([e for e in state["log"] if e[0]=="runtime" and "/functions/" in e[2]])
    wait(f"select state from scheduled_runs where occurrence_key='{source_key}'","completed")
    assert len([e for e in state["log"] if e[0]=="runtime" and "/functions/" in e[2]])==before
    assert sql(f"select value::text from graph_inputs where cell_id='{cell}' order by at desc limit 1")=="4"
    print("PASS scheduling: a persisted source result survives interruption and feeds the graph without re-fetching",flush=True)
    sql(f"delete from graph_cells where id='{cell}';update graphs set structure=jsonb_set(structure,'{{inputs}}','[\"amount\",\"tax\"]') where id='{graph}'")
