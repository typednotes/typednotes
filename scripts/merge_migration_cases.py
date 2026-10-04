"""Real append-only SQL merge: duplicate names, references, inputs and secrets."""
import json
def verify_merge(sql,migration):
    user="11000000-0000-4000-8000-000000000001";org="11000000-0000-4000-8000-000000000002";project="11000000-0000-4000-8000-000000000003"
    a="11000000-0000-4000-8000-000000000004";b="11000000-0000-4000-8000-000000000005"
    sql(f"""insert into users(id,email) values('{user}','merge@example.invalid');
      insert into orgs(id,slug,name) values('{org}','merge','Merge');
      insert into memberships(org_id,user_id,role) values('{org}','{user}','owner');
      insert into projects(id,org_id,slug,name,created_by) values('{project}','{org}','merged','Merged','{user}');
      insert into graphs(id,project_id,slug,name,created_by,created_at) values('{a}','{project}','a','A','{user}','2026-01-01'),('{b}','{project}','b','B','{user}','2026-01-02');
      insert into graph_cells(graph_id,position,name,kind,variant,description,config) values
      ('{a}',0,'amount','source','ui','Amount A','{{"input":"value","output_type":"Nat"}}'),
      ('{a}',1,'total','node',null,'Double @amount, preserving email person@amount.example','{{"dependencies":["amount"],"output_type":"Nat"}}'),
      ('{b}',0,'amount','source','ui','Amount B','{{"input":"value","output_type":"String"}}'),
      ('{b}',1,'total','node',null,'Count @amount','{{"dependencies":["amount"],"output_type":"Nat"}}'),
      ('{b}',2,'credential','source','secret','Secret remains in old vault namespace','{{"name":"token","set_at":"2026-01-01"}}');
      insert into graph_inputs(graph_id,input,value,fed_by) values('{a}','value','7','ui'),('{b}','value','"eleven"','ui');""")
    old=json.loads(sql(f"select json_agg(json_build_object('id',id,'graph',graph_id,'name',name,'config',config)) from graph_cells where graph_id in ('{a}','{b}')"))
    sql(f"update graphs set lode_session_id='legacy-writer' where id='{b}';update graphs set lun_session_id='legacy-runtime' where id='{a}';update graph_cells set impl='{{\"function\":\"total\",\"signature\":\"Nat\"}}' where graph_id='{b}' and name='total'")
    sql(migration)
    assert sql(f"select notes_graph_id::text from projects where id='{project}'")==a
    assert sql(f"select count(*) from graph_cells where graph_id='{a}'")=="5"
    assert sql(f"select count(distinct name) from graph_cells where graph_id='{a}'")=="5"
    assert sql(f"select count(*) from graph_cells c join note_origins o on o.note_id=c.id where c.graph_id='{a}'")=="5"
    rows=json.loads(sql(f"select json_agg(json_build_object('id',c.id,'name',c.name,'config',c.config,'description',c.description,'old_graph',o.graph_id,'old_name',o.original_name)) from graph_cells c join note_origins o on o.note_id=c.id where c.graph_id='{a}'"))
    for total in [r for r in rows if r["old_name"]=="total"]:
        amount=next(r for r in rows if r["old_graph"]==total["old_graph"] and r["old_name"]=="amount")
        assert total["config"]["dependencies"]==[amount["name"]]
        assert "@"+amount["name"] in total["description"]
        assert total["config"]["output_type"]=="Nat"
    values=json.loads(sql(f"select json_agg(json_build_object('input',input,'value',value)) from graph_inputs where graph_id='{a}'"))
    assert len({v["input"] for v in values})==2 and {str(v["value"]) for v in values}=={"7","eleven"}
    assert sql(f"select graph_id::text||':'||(original_config->>'name') from note_origins where original_name='credential'")==b+":token"
    assert sql(f"select count(*) from graph_cells where graph_id='{a}' and impl is not null")=="0"
    assert sql(f"select status from graphs where id='{a}'")=="editing"
    assert {r["id"] for r in rows}=={r["id"] for r in old}
    assert sql(f"select lode_session_id from project_note_retirements where graph_id='{b}'")=="legacy-writer"
    assert sql(f"select lun_session_ids[1] from project_note_retirements where graph_id='{a}'")=="legacy-runtime"
    assert sql(f"select original_impl->>'function' from note_origins where graph_id='{b}' and original_name='total'")=="total"
    print("PASS migration: merged projects preserve every note ID, duplicate-name references, input history, pins and original secret namespaces; execution requires regeneration",flush=True)
