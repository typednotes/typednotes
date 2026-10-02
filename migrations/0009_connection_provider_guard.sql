-- Preserve historical rows/credential references; prevent every new duplicate.
-- The same transaction lock also serializes app policy publication/removal.
create function enforce_connection_provider_once() returns trigger
language plpgsql as $$
begin
    perform pg_advisory_xact_lock(hashtextextended(new.org_id::text, 0));
    if exists (select 1 from connections where org_id = new.org_id and provider = new.provider) then
        raise exception 'this organization already has a connection for %', new.provider
            using errcode = '23505', constraint = 'connections_org_provider_once';
    end if;
    return new;
end;
$$;

create trigger connections_provider_once
before insert on connections for each row
execute function enforce_connection_provider_once();
