//! Default targets are revalidated through memberships on every read. Clients
//! submit slugs, not arbitrary navigation URLs, and cannot mark setup complete
//! until the server's required workspace checks succeed.
use dioxus::prelude::ServerFnError;
use sqlx::Row;
use crate::{Connection, EffectPolicy, Graph, Project, User, UserWorkspace};
use super::{connections, db, errors, graphs, projects};

pub async fn get(user: &User) -> Result<UserWorkspace, ServerFnError> {
    let row = sqlx::query("select org_id::text as org, project_id::text as project, graph_id::text as graph, onboarded_at is not null as done,setup_skipped from user_workspaces where user_id=$1::uuid")
        .bind(&user.id).fetch_optional(db::pool()?).await.map_err(errors::db_error)?;
    let orgs = db::list_orgs_for(&user.id).await?;
    let org = row.as_ref().and_then(|r| r.get::<Option<String>,_>("org"))
        .and_then(|id| orgs.iter().find(|o| o.id == id).cloned())
        .or_else(|| orgs.iter().min_by_key(|o| (&o.created_at, &o.id)).cloned());
    let mut project = None;
    let mut notebook = None;
    let mut requirements = Vec::new();
    let mut has_ai=false;
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
            has_ai=accounts.iter().any(|c|c.status=="active"&&c.provider.can_generate());
            requirements = required(project, notebook.as_ref(), &accounts, &policy);
        } else { requirements.push("Create or choose a project.".into()); }
    } else { requirements.push("Create or choose an organization.".into()); }
    let saved_valid = row.as_ref().is_some_and(|r| r.get::<bool,_>("done") &&
        org.as_ref().is_some_and(|o| r.get::<Option<String>,_>("org").as_deref() == Some(o.id.as_str())) &&
        project.as_ref().is_some_and(|p| r.get::<Option<String>,_>("project").as_deref() == Some(p.id.as_str())) &&
        notebook.as_ref().is_some_and(|g| r.get::<Option<String>,_>("graph").as_deref() == Some(g.id.as_str())));
    let setup_required = !row.as_ref().is_some_and(|r|r.get::<bool,_>("setup_skipped"))&&!saved_valid && !requirements.is_empty();
    let default_url = if setup_required { None } else {
        org.as_ref().zip(project.as_ref()).map(|(o,p)| format!("/orgs/{}/projects/{}", o.slug, p.slug))
    };
    let (setup_url, next_step) = guidance(org.as_ref(), project.as_ref(), notebook.as_ref(), &requirements,has_ai);
    Ok(UserWorkspace { org, project, notebook, requirements, setup_required, default_url, setup_url, next_step })
}

fn guidance(org: Option<&crate::Org>, project: Option<&Project>, graph: Option<&Graph>, missing: &[String],has_ai:bool) -> (String, Option<String>) {
    let Some(org) = org else { return ("/organizations".into(), Some("Connect GitHub or GitLab and an AI account, or skip and create an organization/project yourself.".into())); };
    let Some(project) = project else { return (format!("/orgs/{}/settings/connections",org.slug), Some("Connect your repository and AI accounts, then choose a repository to create its project.".into())); };
    let project_url = format!("/orgs/{}/projects/{}",org.slug,project.slug);
    if project.repo.is_none() { return (format!("{project_url}/settings/repository"), Some("Connect a code host and choose the project's repository.".into())); }
    let Some(_graph) = graph else { return (project_url, Some("Create your first note in this project.".into())); };
    let notebook = project_url.clone();
    if missing.iter().any(|m| m.contains("generative AI")) {
        let url = if !has_ai { format!("/orgs/{}/settings/connections",org.slug) } else {format!("{project_url}/settings/model")};
        return (url, Some("Connect an AI provider, then select the project's model.".into()));
    }
    if missing.iter().any(|m| m.contains("repository")) { return (format!("{project_url}/settings/repository"),Some("Allow code writing in this repository's project folder.".into())); }
    if !missing.is_empty() { return (format!("{project_url}/settings/permissions"),missing.first().cloned()); }
    (notebook,None)
}

/// New-user default grants are idempotent and occur on verified sign-in, not
/// when someone merely invites an address. Linking another IdP does not create
/// another workspace or grant credits again.
pub async fn welcome(user: &str) {
    let Ok(pool) = db::pool() else { return; };
    let Ok(Some(row)) = sqlx::query("select org_id::text as org from user_workspaces where user_id=$1::uuid and org_id is not null")
        .bind(user).fetch_optional(pool).await else { return; };
    let org: String = row.get("org");
    if let Ok(Some(_)) = sqlx::query("select 1 from orgs where id=$1::uuid and slug=$2::citext")
        .bind(&org).bind(format!("workspace-{user}")).fetch_optional(pool).await {
        db::welcome_grant(&org).await;
    }
}

fn subtree(connection: &Connection, policy: &EffectPolicy, operation: &str, root: &[String]) -> bool {
    let parent = connection.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(connection.provider, crate::PermissionPreset::ReadOnly));
    let effective = policy.connector_ceilings.get(connection.provider.id()).map_or(parent.clone(), |p| parent.intersect(p));
    policy.allows_connector(connection.provider) && effective.scopes.iter().any(|s| s.operation == operation && s.descendants && root.starts_with(&s.root))
}

pub(super) fn required(project: &Project, graph: Option<&Graph>, connections: &[Connection], policy: &EffectPolicy) -> Vec<String> {
    let mut missing = Vec::new();
    let active = |id: &str| connections.iter().find(|c| c.id == id && c.status == "active");
    if let Some(repo) = &project.repo {
        if let Some(connection) = repo.connection_id.as_deref().and_then(active) {
            let root = repo.full_name.split('/').map(str::to_string).collect::<Vec<_>>();
            let mut write = root.clone();
            write.push("typednotes".into());
            if let Some(graph) = graph { write.push(graph.slug.clone()); }
            if !subtree(connection, policy, "repositories.read", &root) || !subtree(connection, policy, "repositories.write", &write) {
                missing.push("Allow repository read and scoped code-writing access in connection permissions and organization execution policy.".into());
            }
        } else { missing.push("Connect an active repository account.".into()); }
    } else { missing.push("Choose the project's primary repository.".into()); }
    if let Some(graph) = graph {
        if let Some(connection) = graph.model_connection_id.as_deref().and_then(active) {
            let model = graph.model_name.as_deref().unwrap_or("");
            let parent = connection.permissions.clone().unwrap_or_else(|| crate::ConnectorPermissions::preset(connection.provider, crate::PermissionPreset::ReadOnly));
            if !connection.provider.can_generate() || model.is_empty() || policy.connector_blocker(connection.provider, &parent, "inference.generate", &crate::model_resource(connection.provider, model)).is_some() {
                missing.push("Select an allowed generative AI connection and model for the project.".into());
            }
        } else { missing.push("Connect a generative AI provider and select the project's model.".into()); }
    } else { missing.push("Initialize this project's notes.".into()); }
    if !["read", "ls", "grep", "write", "edit", "check", "publish", "lun_build"].iter().all(|tool| policy.tools.iter().any(|t| t == tool)) {
        missing.push("Enable the required code-writing tools in organization execution policy.".into());
    }
    missing
}

pub async fn set(user: &User, org_slug: &str, project_slug: Option<&str>, graph_slug: Option<&str>, finish: bool) -> Result<UserWorkspace, ServerFnError> {
    let org = db::org_for_member(org_slug, &user.id).await?.ok_or_else(|| errors::forbidden("default organization must be one you belong to"))?;
    let project = match project_slug { Some(slug) => Some(projects::get(&org, slug).await?.0), None => None };
    let graph = match (&project, graph_slug) {
        (Some(project), Some(slug)) => Some(graphs::list(&org, &project.slug).await?.into_iter().find(|g| g.slug == slug).ok_or_else(|| errors::not_found("no such default project graph"))?),
        (None, Some(_)) => return Err(errors::bad_request("choose a project before its notes")),
        (Some(project),None)=>graphs::list(&org,&project.slug).await?.into_iter().next(),
        _ => None,
    };
    if finish {
        let Some(project) = &project else { return Err(errors::bad_request("choose a default project")); };
        let Some(graph) = &graph else { return Err(errors::bad_request("initialize the default project's notes")); };
        let missing = required(project, Some(graph), &connections::list(&org, user).await?, &db::org_settings(&org).await?.effect_policy);
        if !missing.is_empty() { return Err(errors::bad_request(missing.join(" "))); }
    }
    sqlx::query("insert into user_workspaces(user_id,org_id,project_id,graph_id,onboarded_at) values($1::uuid,$2::uuid,$3::uuid,$4::uuid,case when $5 then now() else null end) \
        on conflict(user_id) do update set org_id=excluded.org_id,project_id=excluded.project_id,graph_id=excluded.graph_id,onboarded_at=excluded.onboarded_at,updated_at=now()")
        .bind(&user.id).bind(&org.id).bind(project.as_ref().map(|p| &p.id)).bind(graph.as_ref().map(|g| &g.id)).bind(finish)
        .execute(db::pool()?).await.map_err(errors::db_error)?;
    get(user).await
}

pub async fn begin_setup(user:&User)->Result<crate::Org,ServerFnError>{
    let mut tx=db::pool()?.begin().await.map_err(errors::db_error)?;
    sqlx::query("select id from users where id=$1::uuid and deleted_at is null for update")
        .bind(&user.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?;
    let existing=sqlx::query("select o.id::text as id from orgs o join memberships m on m.org_id=o.id where m.user_id=$1::uuid order by (o.setup_owner=$1::uuid) desc nulls last,o.created_at,o.id limit 1")
        .bind(&user.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?;
    let id=if let Some(row)=existing {row.get::<String,_>("id")}else{
        let id=sqlx::query("insert into orgs(slug,name,setup_owner) values($1,'Connect your accounts',$2::uuid) returning id::text as id")
            .bind(format!("setup-{}",user.id)).bind(&user.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?.get::<String,_>("id");
        sqlx::query("insert into memberships(org_id,user_id,role) values($1::uuid,$2::uuid,'owner')")
            .bind(&id).bind(&user.id).execute(&mut *tx).await.map_err(errors::db_error)?;
        id
    };
    sqlx::query("insert into user_workspaces(user_id,org_id,setup_skipped) values($1::uuid,$2::uuid,false) on conflict(user_id) do update set org_id=excluded.org_id,setup_skipped=false,updated_at=now()")
        .bind(&user.id).bind(&id).execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)?;
    let org=db::org_by_id_for_member(&id,&user.id).await?.ok_or_else(||errors::forbidden("setup organization is no longer available"))?;
    db::welcome_grant(&org.id).await;
    Ok(org)
}

pub async fn skip_setup(user:&User)->Result<(),ServerFnError>{
    sqlx::query("insert into user_workspaces(user_id,setup_skipped) values($1::uuid,true) on conflict(user_id) do update set setup_skipped=true,updated_at=now()")
        .bind(&user.id).execute(db::pool()?).await.map_err(errors::db_error)?;
    Ok(())
}

fn address(value:&str)->String{
    let value=value.to_ascii_lowercase().chars().map(|c|if c.is_ascii_alphanumeric(){c}else{'-'}).collect::<String>();
    let value=value.trim_matches('-').chars().take(48).collect::<String>();
    if value.chars().next().is_some_and(|c|c.is_ascii_alphabetic()){value}else{format!("repo-{value}")}
}

pub async fn project_from_repo(user:&User,org_slug:&str,connection:&str,full_name:&str)->Result<(crate::Org,Project),ServerFnError>{
    let mut org=db::org_for_member(org_slug,&user.id).await?.ok_or_else(||errors::forbidden("choose an organization you belong to"))?;
    let repo=projects::verify_selection(&org,user,connection,full_name).await?;
    let (owner,name)=repo.full_name.split_once('/').ok_or_else(||errors::bad_request("repository must be owner/name"))?;
    let mut tx=super::connector::lock(&org.id).await?;super::connector::require_actor(&mut tx,&org.id,&user.id).await?;
    let provisional=sqlx::query("select setup_owner::text as owner from orgs where id=$1::uuid for update").bind(&org.id).fetch_one(&mut *tx).await.map_err(errors::db_error)?.get::<Option<String>,_>("owner");
    let provider=sqlx::query("select provider from connections where id=$1::uuid and org_id=$2::uuid and status='active'")
        .bind(connection).bind(&org.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?.ok_or_else(||errors::forbidden("the repository connection changed; reconnect and retry"))?.get::<String,_>("provider");
    if provisional.as_deref()==Some(&user.id){
        let mut slug=address(owner);
        if sqlx::query("select 1 from orgs where slug=$1::citext and id<>$2::uuid").bind(&slug).bind(&org.id).fetch_optional(&mut *tx).await.map_err(errors::db_error)?.is_some(){slug=format!("{}-{}",slug,&org.id.replace('-',"")[..8]);}
        sqlx::query("update orgs set slug=$2,name=$3,setup_owner=null where id=$1::uuid").bind(&org.id).bind(&slug).bind(owner).execute(&mut *tx).await.map_err(errors::db_error)?;
        org.slug=slug;org.name=owner.into();
    }
    let existing=sqlx::query("select id::text as id,slug::text as slug from projects where org_id=$1::uuid and created_by=$2::uuid and repo_connection_id=$3::uuid and lower(repo_full_name)=lower($4) order by created_at,id limit 1")
        .bind(&org.id).bind(&user.id).bind(connection).bind(&repo.full_name).fetch_optional(&mut *tx).await.map_err(errors::db_error)?;
    let (id,slug)=if let Some(row)=existing {(row.get::<String,_>("id"),row.get::<String,_>("slug"))}else{
        let mut slug=address(name);
        if sqlx::query("select 1 from projects where org_id=$1::uuid and slug=$2::citext").bind(&org.id).bind(&slug).fetch_optional(&mut *tx).await.map_err(errors::db_error)?.is_some(){slug=format!("{slug}-{}",&uuid::Uuid::new_v4().simple().to_string()[..8]);}
        let id=sqlx::query("insert into projects(org_id,slug,name,created_by,repo_connection_id,repo_provider,repo_full_name,repo_web_url,repo_default_branch) values($1::uuid,$2,$3,$4::uuid,$5::uuid,$6,$7,$8,$9) returning id::text as id")
            .bind(&org.id).bind(&slug).bind(name).bind(&user.id).bind(connection).bind(provider).bind(&repo.full_name).bind(&repo.web_url).bind(&repo.default_branch)
            .fetch_one(&mut *tx).await.map_err(errors::db_error)?.get::<String,_>("id");
        (id,slug)
    };
    sqlx::query("insert into user_workspaces(user_id,org_id,project_id,graph_id,setup_skipped) select $1::uuid,$2::uuid,p.id,p.notes_graph_id,false from projects p where p.id=$3::uuid on conflict(user_id) do update set org_id=excluded.org_id,project_id=excluded.project_id,graph_id=excluded.graph_id,setup_skipped=false,onboarded_at=null,updated_at=now()")
        .bind(&user.id).bind(&org.id).bind(&id).execute(&mut *tx).await.map_err(errors::db_error)?;
    tx.commit().await.map_err(errors::db_error)?;
    let project=projects::get(&org,&slug).await?.0;
    Ok((org,project))
}
