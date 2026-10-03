-- Provision new users atomically, including invite placeholders. UUID-backed
-- slugs cannot collide with another user's workspace or expose their email.
-- Do not recreate workspaces for users who intentionally deleted/left them.
create function create_default_workspace() returns trigger language plpgsql as $$
declare org uuid; project uuid;
begin
    insert into orgs(slug,name) values('workspace-' || new.id::text,'Personal workspace') returning id into org;
    insert into memberships(org_id,user_id,role) values(org,new.id,'owner');
    insert into projects(org_id,slug,name,created_by) values(org,'my-project','My project',new.id) returning id into project;
    insert into user_workspaces(user_id,org_id,project_id) values(new.id,org,project);
    return new;
end;
$$;
create trigger users_default_workspace after insert on users
for each row execute function create_default_workspace();

-- Reauthorization is tied to an existing connection, not an extra provider slot.
alter table oauth_flows add column connection_id uuid references connections(id) on delete cascade;
alter table connections add column credential_revision uuid;

-- Durable, actor-bound requests let the saved cell render while checkout/model
-- calls run. A changed revision stays queued for the next serialized launch.
create table graph_generation_requests (
    graph_id uuid primary key references graphs(id) on delete cascade,
    user_id uuid not null references users(id) on delete cascade,
    revision uuid not null default gen_random_uuid(),
    requested_at timestamptz not null default now()
);
