-- Bounded retry of acknowledged/background writer startup. A retry preserves its
-- revision key; declaration changes allocate a new immutable launch identity.
alter table graph_generation_requests add column startup_attempts integer not null default 0 check(startup_attempts between 0 and 5);
alter table graph_generation_requests add column retry_at timestamptz not null default now();

-- A project owns one internal runtime graph. Historical graphs remain as origin
-- records so code/secret namespaces are never silently discarded.
alter table projects add column notes_graph_id uuid references graphs(id) on delete set null;
alter table projects add column note_connections jsonb not null default '[]';
alter table graphs add column archived_at timestamptz;
-- Lun's stateless graph steps return caller-owned state. It remains private to
-- the app and separate from the fresh execution envelope on every request.
alter table graphs add column runtime_state jsonb;
alter table graphs add column runtime_due_at timestamptz;
alter table notebook_share_sessions add column runtime_state jsonb;
alter table graph_updates drop constraint graph_updates_fed_by_check;
alter table graph_updates add constraint graph_updates_fed_by_check check(fed_by in ('ui','check','endpoint','channel','register','producer'));
-- External authority must be retired before a merged project starts a new run.
-- Retain handles before resetting graphs; a service outage leaves this durable
-- queue intact and blocks generation, rather than losing cleanup information.
create table project_note_retirements (
  graph_id uuid primary key references graphs(id) on delete restrict,
  lode_session_id text,
  lun_session_ids text[] not null default '{}'
);
insert into project_note_retirements(graph_id,lode_session_id,lun_session_ids)
select g.id,g.lode_session_id,
  array_remove(array[g.lun_session_id] || coalesce((select array_agg(s.lun_session_id)
    from notebook_share_sessions s join notebook_shares sh on sh.id=s.share_id
    where sh.graph_id=g.id and s.lun_session_id is not null),'{}'::text[]),null)
from graphs g;
create unique index project_notes_graph on projects(notes_graph_id) where notes_graph_id is not null;
create table note_origins (
  note_id uuid primary key references graph_cells(id) on delete cascade,
  graph_id uuid not null references graphs(id) on delete cascade,
  original_name text not null,
  original_config jsonb not null,
  original_impl jsonb
);
insert into note_origins select id,graph_id,name,config,impl from graph_cells;

create temporary table merged_graphs as
select p.id as project_id,(select g.id from graphs g where g.project_id=p.id order by g.created_at,g.id limit 1) as root from projects p;
insert into graphs(project_id,slug,name,created_by)
select p.id,p.slug,p.name,p.created_by from projects p join merged_graphs m on m.project_id=p.id where m.root is null;
update merged_graphs m set root=(select g.id from graphs g where g.project_id=m.project_id order by g.created_at,g.id limit 1) where root is null;
update projects p set notes_graph_id=m.root from merged_graphs m where m.project_id=p.id;

create temporary table merged_names(note_id uuid primary key,old_graph uuid,old_name text,new_name text,old_input text,new_input text);
create temporary table claimed_names(project_id uuid,name text,primary key(project_id,name));
do $$
declare r record; candidate text; input_candidate text; suffix integer;
begin
  for r in select c.*,g.project_id,m.root from graph_cells c join graphs g on g.id=c.graph_id join merged_graphs m on m.project_id=g.project_id
           order by g.project_id,(g.id=m.root) desc,g.created_at,g.id,c.position,c.id loop
    candidate:=r.name;suffix:=0;
    while exists(select 1 from claimed_names where project_id=r.project_id and name=candidate)
       or (suffix>0 and exists(select 1 from graph_cells c join graphs g on g.id=c.graph_id where g.project_id=r.project_id and c.name=candidate)) loop
      suffix:=suffix+1;
      candidate:=left(r.name,20)||'_'||left(replace(r.id::text,'-',''),12)||'_'||suffix::text;
    end loop;
    insert into claimed_names values(r.project_id,candidate);
    insert into merged_names(note_id,old_graph,old_name,new_name,old_input)
    values(r.id,r.graph_id,r.name,candidate,case when r.kind='source' and r.variant<>'secret' then coalesce(r.config->>'input',r.name) end);
  end loop;
end $$;
-- Source inputs have their own namespace; qualify collisions without conflating
-- an input identifier with its source note's display name.
truncate claimed_names;
do $$
declare r record; candidate text; suffix integer;
begin
  for r in select n.*,g.project_id from merged_names n join graphs g on g.id=n.old_graph where n.old_input is not null order by g.project_id,n.note_id loop
    candidate:=r.old_input;suffix:=0;
    while exists(select 1 from claimed_names where project_id=r.project_id and name=candidate) loop
      suffix:=suffix+1;candidate:='input_'||left(replace(r.note_id::text,'-',''),24)||'_'||suffix::text;
    end loop;
    insert into claimed_names values(r.project_id,candidate);
    update merged_names set new_input=candidate where note_id=r.note_id;
  end loop;
end $$;
-- Rename dependencies and explicit @references within their original graph.
do $$
declare r record;
begin
  for r in select * from merged_names where old_name<>new_name loop
    update graph_cells set description=regexp_replace(description,'(^|[^a-zA-Z0-9_])@'||r.old_name||'(?=[^a-zA-Z0-9_]|$)','\1@'||r.new_name,'g') where graph_id=r.old_graph;
  end loop;
end $$;
update graph_cells c set config=jsonb_set(c.config,'{dependencies}',
  coalesce((select jsonb_agg(coalesce(n.new_name,dep.value) order by dep.ordinality) from jsonb_array_elements_text(c.config->'dependencies') with ordinality dep(value,ordinality)
    left join merged_names n on n.old_graph=c.graph_id and n.old_name=dep.value),'[]'::jsonb)) where jsonb_typeof(c.config->'dependencies')='array';
update graph_cells c set config=jsonb_set(c.config,'{input}',to_jsonb(n.new_input)) from merged_names n where n.note_id=c.id and n.new_input is not null;
update graph_inputs i set input=n.new_input from merged_names n where n.old_graph=i.graph_id and n.old_input=i.input and n.new_input is not null;
-- Secret logical names are unique project-wide. Physical namespace/name remains
-- in note_origins and is routed through authenticated local authority records.
update graph_cells c set config=jsonb_set(c.config,'{name}',to_jsonb(n.new_name)) from merged_names n where n.note_id=c.id and c.variant='secret';
-- Use temporary collision-free identifiers before installing the final names.
update graph_cells c set name='m_'||replace(c.id::text,'-','') from merged_names n where n.note_id=c.id and c.name<>n.new_name;
update graph_cells c set name=n.new_name from merged_names n where n.note_id=c.id;
update graph_cells c set graph_id=m.root,position=s.position from
  (select c.id,(row_number() over(partition by g.project_id order by g.created_at,g.id,c.position,c.id)-1)::integer as position from graph_cells c join graphs g on g.id=c.graph_id) s,
  merged_graphs m,graphs old where s.id=c.id and old.id=c.graph_id and m.project_id=old.project_id;
update graph_inputs i set graph_id=m.root from graphs g join merged_graphs m on m.project_id=g.project_id where i.graph_id=g.id;
update graph_updates u set graph_id=m.root from graphs g join merged_graphs m on m.project_id=g.project_id where u.graph_id=g.id;
update user_workspaces w set graph_id=m.root from graphs g join merged_graphs m on m.project_id=g.project_id where w.graph_id=g.id;
update graphs g set archived_at=now(),status='editing',lode_session_id=null,lun_session_id=null,session_build_id=null where exists(select 1 from merged_graphs m where m.project_id=g.project_id and m.root<>g.id);
update graphs g set name=p.name,status='editing',status_detail='Project notes merged. Regenerate code before execution.',lode_session_id=null,lun_session_id=null,lun_build_id=null,session_build_id=null,last_nodes=null,structure=null from projects p where p.notes_graph_id=g.id;
update graph_cells set impl=null,code_at=null,writing=false,edited_at=now();
delete from graph_generation_requests;
-- Prior published snapshots stay stored for audit but links are closed after the
-- structural change; migration never runs code or broadens runtime authority.
alter table notebook_shares add column revoked_at timestamptz;
update notebook_shares set revoked_at=now();

create function create_project_notes() returns trigger language plpgsql as $$
declare root uuid;
begin
  insert into graphs(project_id,slug,name,created_by) values(new.id,new.slug,new.name,new.created_by) returning id into root;
  update projects set notes_graph_id=root where id=new.id;
  return new;
end;
$$;
create trigger projects_notes after insert on projects for each row execute function create_project_notes();

-- New users choose repository-derived or manual organizations/projects. Existing
-- user-owned defaults are preserved; account creation no longer invents them.
drop trigger users_default_workspace on users;
alter table user_workspaces add column setup_skipped boolean not null default false;
alter table orgs add column setup_owner uuid references users(id) on delete set null;
create unique index one_setup_org_per_user on orgs(setup_owner) where setup_owner is not null;
