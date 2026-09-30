-- Admin-owned permissions; a missing domain grants no anonymous network access.
alter table orgs add column effect_policy jsonb;
alter table orgs add constraint orgs_effect_policy_object
  check (effect_policy is null or jsonb_typeof(effect_policy) = 'object');

alter table connections add column permissions jsonb;
alter table connections add constraint connections_permissions_object
  check (permissions is null or jsonb_typeof(permissions) = 'object');
