-- migrate:up

-- The very simplest RBAC implementation, the roles get added to the group_users table
-- as users are added to an org.

CREATE TYPE role AS ENUM (
    'Administrator', 
    'Collaborator'
);
COMMENT ON TYPE role IS 'Users have roles, they can be managers or administrators etc.';

CREATE TYPE visibility AS ENUM (
    'Organization', 
    'Group', 
    'Private'
);
COMMENT ON TYPE visibility IS 'For some data the user can select the visibility';

CREATE TYPE permission AS ENUM (
    -- The ManageGroup permission gives the user the ability to invite group members, 
    -- delete group members and change the group name
    'ManageGroup'
);
COMMENT ON TYPE permission IS 'A permission gives the user the ability to do something. i.e. Manage users.';

CREATE TABLE roles_permissions (
    role role NOT NULL,
    permission permission NOT NULL,

    PRIMARY KEY (role, permission)
);
COMMENT ON TABLE roles_permissions IS 'Maps roles to permissions. i.e. a role can have multiple permissions.';

INSERT INTO roles_permissions VALUES('Administrator', 'ManageGroup');


-- Give access to the application user.
GRANT SELECT ON roles_permissions TO typednotes_application;

-- Give access to the readonly user
GRANT SELECT ON roles_permissions TO typednotes_readonly;

-- migrate:down

DROP TABLE roles_permissions;
DROP TYPE role;
DROP TYPE permission;
DROP TYPE visibility;
