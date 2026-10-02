//! Default targets are revalidated through memberships on every read. Clients
//! submit slugs, not arbitrary navigation URLs, and cannot mark setup complete
//! until the server's required workspace checks succeed.
use dioxus::prelude::ServerFnError;
use sqlx::Row;
use crate::{Connection, EffectPolicy, Graph, Project, User, UserWorkspace};
use super::{connections, db, errors, graphs, projects};

pub async fn get(user: &User) -> Result<UserWorkspace, ServerFnError> {
    let row = sqlx::query("select org_id::text as org, project_id::text as project, graph_id::text as graph, onboarded_at is not null as done from user_workspaces where user_id=$1::uuid")
        .bind(&user.id).fetch_optional(db::pool()?).await.map_err(errors::db_error)?;
    let orgs = db::list_orgs_for(&user.id).await?;
    let org = row.as_ref().and_then(|r| r.get::<Option<String>,_>("org"))
        .and_then(|id| orgs.iter().find(|o| o.id == id).cloned())
        .or_else(|| orgs.iter().min_by_key(|o| (&o.created_at, &o.id)).cloned());
    let mut project = None;
    let mut notebook = None;
    let mut requirements = Vec::new();
    if let Some(org) = &org {
        let candidates = projects::list(org).await?;
        project = row.as_ref().and_then(|r| r.get::<Option<String>,_>("project"))
            .and_then(|id| candidates.iter().find(|p| p.id == id).cloned())
            .or_else(|| candidates.iter().min_by_key(|p| (&p.created_at, &p.id)).cloned());
        if let Some(project) = &project {
            let candidates = graphs::list(org, &project.slug).await?;
            notebook = row.as_ref().and_then(|r| r.get::<Option<String>,_>("graph"))
                .and_then(|id| candidates.iter().find(|g| g.id == id).cloned())
                .or_else(|| candidates.iter().min_by_key(|g| (&g.created_at, &g.id)).cloned());
            let policy = db::org_settings(org).await?.effect_policy;
            let accounts = connections::list(org, user).await?;
            requirements = required(project, notebook.as_ref(), &accounts, &policy);
        } else { requirements.push("Create or choose a project.".into()); }
    } else { requirements.push("Create or choose an organization.".into()); }
    let saved_valid = row.as_ref().is_some_and(|r| r.get::<bool,_>("done") &&
        org.as_ref().is_some_and(|o| r.get::<Option<String>,_>("org").as_deref() == Some(o.id.as_str())) &&
        project.as_ref().is_some_and(|p| r.get::<Option<String>,_>("project").as_deref() == Some(p.id.as_str())) &&
        notebook.as_ref().is_some_and(|g| r.get::<Option<String>,_>("graph").as_deref() == Some(g.id.as_str())));
    let setup_required = !saved_valid && !requirements.is_empty();
    let default_url = if setup_required { None } else {
        org.as_ref().zip(project.as_ref()).zip(notebook.as_ref()).map(|((o,p),g)| format!("/orgs/{}/projects/{}/graphs/{}", o.slug, p.slug, g.slug))
    };
    Ok(UserWorkspace { org, project, notebook, requirements, setup_required, default_url })
}

fn subtree(connection: &Connection, policy: &EffectPolicy, operation: &str, root: &[String]) -> bool {
    let parent = connection.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(connection.provider, crate::PermissionPreset::ReadOnly));
    let effective = policy.connector_ceilings.get(connection.provider.id()).map_or(parent.clone(), |p| parent.intersect(p));
    policy.allows_connector(connection.provider) && effective.scopes.iter().any(|s| s.operation == operation && s.descendants && root.starts_with(&s.root))
}

fn required(project: &Project, graph: Option<&Graph>, connections: &[Connection], policy: &EffectPolicy) -> Vec<String> {
    let mut missing = Vec::new();
    let active = |id: &str| connections.iter().find(|c| c.id == id && c.status == "active");
    if let Some(repo) = &project.repo {
        if let Some(connection) = repo.connection_id.as_deref().and_then(active) {
            let root = repo.full_name.split('/').map(str::to_string).collect::<Vec<_>>();
            let mut write = root.clone();
            write.push("typednotes".into());
            if let Some(graph) = graph { write.push(graph.slug.clone()); }
            if !subtree(connection, policy, "repositories.read", &root) || !subtree(connection, policy, "repositories.write", &write) {
                missing.push("Allow repository read and scoped code-writing access in connection and notebook permissions.".into());
            }
        } else { missing.push("Connect an active repository account.".into()); }
    } else { missing.push("Choose the project's primary repository.".into()); }
    if let Some(graph) = graph {
        if let Some(connection) = graph.model_connection_id.as_deref().and_then(active) {
            let model = graph.model_name.as_deref().unwrap_or("");
            let parent = connection.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(connection.provider, crate::PermissionPreset::ReadOnly));
            if !connection.provider.can_generate() || model.is_empty() || policy.connector_blocker(connection.provider, &parent, "inference.generate", &crate::model_resource(connection.provider, model)).is_some() {
                missing.push("Select an allowed generative AI connection and model for the notebook.".into());
            }
        } else { missing.push("Connect a generative AI provider and select the notebook's model.".into()); }
    } else { missing.push("Create or choose a notebook.".into()); }
    if !["read", "ls", "grep", "write", "edit", "check", "publish", "lun_build"].iter().all(|tool| policy.tools.iter().any(|t| t == tool)) {
        missing.push("Enable the required code-writing tools in organization notebook permissions.".into());
    }
    missing
}

pub async fn set(user: &User, org_slug: &str, project_slug: Option<&str>, graph_slug: Option<&str>, finish: bool) -> Result<UserWorkspace, ServerFnError> {
    let org = db::org_for_member(org_slug, &user.id).await?.ok_or_else(|| errors::forbidden("default organization must be one you belong to"))?;
    let project = match project_slug { Some(slug) => Some(projects::get(&org, slug).await?.0), None => None };
    let graph = match (&project, graph_slug) {
        (Some(project), Some(slug)) => Some(graphs::list(&org, &project.slug).await?.into_iter().find(|g| g.slug == slug).ok_or_else(|| errors::not_found("no such default notebook"))?),
        (None, Some(_)) => return Err(errors::bad_request("choose a project before a notebook")),
        _ => None,
    };
    if finish {
        let Some(project) = &project else { return Err(errors::bad_request("choose a default project")); };
        let Some(graph) = &graph else { return Err(errors::bad_request("choose a default notebook")); };
        let missing = required(project, Some(graph), &connections::list(&org, user).await?, &db::org_settings(&org).await?.effect_policy);
        if !missing.is_empty() { return Err(errors::bad_request(missing.join(" "))); }
    }
    sqlx::query("insert into user_workspaces(user_id,org_id,project_id,graph_id,onboarded_at) values($1::uuid,$2::uuid,$3::uuid,$4::uuid,case when $5 then now() else null end) \
        on conflict(user_id) do update set org_id=excluded.org_id,project_id=excluded.project_id,graph_id=excluded.graph_id,onboarded_at=excluded.onboarded_at,updated_at=now()")
        .bind(&user.id).bind(&org.id).bind(project.as_ref().map(|p| &p.id)).bind(graph.as_ref().map(|g| &g.id)).bind(finish)
        .execute(db::pool()?).await.map_err(errors::db_error)?;
    get(user).await
}
