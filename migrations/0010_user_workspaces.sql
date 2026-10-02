create table user_workspaces (
    user_id uuid primary key references users(id) on delete cascade,
    org_id uuid references orgs(id) on delete set null,
    project_id uuid references projects(id) on delete set null,
    graph_id uuid references graphs(id) on delete set null,
    onboarded_at timestamptz,
    updated_at timestamptz not null default now()
);
