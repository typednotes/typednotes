-- Owned projects must not survive their owner's deletion. Existing unowned
-- rows are left untouched; this changes the transition, not historical data.
alter table projects drop constraint projects_created_by_fkey;
alter table projects add constraint projects_created_by_fkey
    foreign key (created_by) references users(id) on delete cascade;

-- FK-driven default resets must invalidate completion and the whole descendant
-- selection, not leave a graph/project pointer beneath a removed default org.
create function reset_removed_workspace() returns trigger language plpgsql as $$
begin
    if old.org_id is not null and new.org_id is null then
        new.project_id := null;
        new.graph_id := null;
        new.onboarded_at := null;
    elsif old.project_id is not null and new.project_id is null then
        new.graph_id := null;
        new.onboarded_at := null;
    elsif old.graph_id is not null and new.graph_id is null then
        new.onboarded_at := null;
    end if;
    new.updated_at := now();
    return new;
end;
$$;
create trigger user_workspaces_reset_removed
before update on user_workspaces for each row execute function reset_removed_workspace();

-- All membership removals (including account cascades) obey the same owner
-- rule. Parent-row locking serializes simultaneous departures/ownership edits.
create function guard_membership_owner() returns trigger language plpgsql as $$
begin
    if tg_op = 'UPDATE' then
        if new.org_id = old.org_id and new.user_id = old.user_id and new.role = 'owner' then
            return new;
        end if;
    end if;
    perform 1 from orgs where id = old.org_id for update;
    if found and old.role = 'owner'
        and (tg_op = 'UPDATE' or exists (select 1 from memberships where org_id = old.org_id and user_id <> old.user_id))
        and not exists (select 1 from memberships where org_id = old.org_id and user_id <> old.user_id and role = 'owner') then
        raise exception 'transfer organization ownership before removing its last owner'
            using errcode = '23514', constraint = 'memberships_keep_owner';
    end if;
    if tg_op = 'UPDATE' then return new; else return old; end if;
end;
$$;
create trigger memberships_keep_owner
before delete or update of role, org_id, user_id on memberships
for each row execute function guard_membership_owner();

create function cleanup_removed_membership() returns trigger language plpgsql as $$
begin
    update user_workspaces set org_id = null, project_id = null, graph_id = null, onboarded_at = null
        where user_id = old.user_id and org_id = old.org_id;
    delete from orgs where id = old.org_id
        and not exists (select 1 from memberships where org_id = old.org_id);
    return old;
end;
$$;
create trigger memberships_cleanup_removed
after delete on memberships for each row execute function cleanup_removed_membership();

-- The app refuses destructive cleanup until the independently owned ledger
-- history has changed every restrictive tenant FK. An absent ledger is valid
-- for local/app-only installations; an older or partially applied one is not.
create function tenant_deletion_ready() returns boolean language sql stable as $$
    select not exists (
        select 1 from (values
            ('usage_events', 'usage_events_org_id_fkey', 'c'),
            ('credit_ledger', 'credit_ledger_org_id_fkey', 'c'),
            ('credit_holds', 'credit_holds_org_id_fkey', 'c'),
            ('usage_events', 'usage_events_user_id_fkey', 'n'),
            ('credit_ledger', 'credit_ledger_usage_event_fkey', 'n')
        ) expected(table_name, constraint_name, delete_action)
        where to_regclass('public.' || expected.table_name) is not null
          and not exists (select 1 from pg_constraint c
              where c.conrelid = to_regclass('public.' || expected.table_name)
                and c.conname = expected.constraint_name
                and c.confdeltype::text = expected.delete_action)
    );
$$;
