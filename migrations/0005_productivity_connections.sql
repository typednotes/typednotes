-- Calendar, webmail and workspace connections. Credentials stay in the vault.
-- Provider ids must match Provider::ALL, vault paths and warrant capabilities.

alter table connections drop constraint connections_provider_check;
alter table connections add constraint connections_provider_check check (provider in
    ('github', 'gitlab', 'gdrive', 'dropbox', 's3', 'azure',
     'mistral', 'openai', 'anthropic', 'openai-compatible',
     'slack', 'whatsapp', 'signal',
     'google-calendar', 'microsoft-calendar', 'caldav', 'gmail', 'outlook', 'jmap', 'notion'));

alter table oauth_flows drop constraint oauth_flows_idp_check;
alter table oauth_flows add constraint oauth_flows_idp_check check (idp in
    ('github', 'google', 'gitlab', 'dropbox', 'slack', 'microsoft'));

alter table oauth_flows drop constraint oauth_flows_connection_provider_check;
alter table oauth_flows add constraint oauth_flows_connection_provider_check
    check (connection_provider in
        ('github', 'gitlab', 'gdrive', 'dropbox', 'slack',
         'google-calendar', 'microsoft-calendar', 'gmail', 'outlook'));
