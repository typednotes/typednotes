//! Shared fullstack server functions — the `core` service of
//! `docs/architecture.md` §2: users (signed in with GitHub or Google), their
//! orgs and projects, the third-party accounts connected to those orgs, and
//! the messaging interfaces of each project.
//!
//! The contract with the other services — `secrets`, `liaison`, `ledger` and
//! `typednotes-infra` — is `docs/connections.md`. Two rules from it shape
//! this crate:
//!
//! - **No credential ever reaches the client.** Tokens and keys go from the
//!   OAuth callback or a form straight into the vault; nothing here returns
//!   one, and the app's vault identity cannot even read them back.
//! - **Every org read is scoped to the caller's memberships.**
//!
//! The schema is applied by `typednotes-infra`, never by this server, so the
//! server's database identity has data rights only.

use dioxus::prelude::*;

#[cfg(feature = "server")]
mod server;

/// The routes the OAuth round trips need, to merge into the Dioxus router.
#[cfg(feature = "server")]
pub fn auth_routes() -> dioxus::server::axum::Router {
    server::routes::router()
}

/// Start the background work of a server process: the source scheduler's
/// tick (docs/computations.md §3.1). Call once, inside the server's runtime.
#[cfg(feature = "server")]
pub fn start_background() {
    server::scheduler::start();
}

#[cfg(feature = "server")]
use server::{channels, connections, db, errors, graphs, members, projects, session};

mod model;
pub use model::*;

mod ai;
pub use ai::*;

mod validate;
pub use validate::*;

mod computation;
pub use computation::*;

mod notebook;
pub use notebook::*;

mod permissions;
pub use permissions::*;

// ── Server functions ────────────────────────────────────────────────────

/// Reachability and configuration, for the status line.
#[get("/api/health")]
pub async fn health() -> Result<Health, ServerFnError> {
    Ok(db::health().await)
}

/// The signed-in user, if any.
#[get("/api/me")]
pub async fn current_user() -> Result<Option<User>, ServerFnError> {
    session::current_user().await
}

/// End the session and clear its cookie.
#[post("/api/logout")]
pub async fn logout() -> Result<(), ServerFnError> {
    use dioxus::fullstack::{FullstackContext, HeaderValue};
    let headers = session::request_headers().await?;
    session::destroy(&headers).await?;
    let secure = server::config::public_url(&headers).starts_with("https://");
    if let Some(ctx) = FullstackContext::current() {
        if let Ok(value) = HeaderValue::from_str(&session::clear_cookie(secure)) {
            ctx.add_response_header(dioxus::fullstack::http::header::SET_COOKIE, value);
        }
    }
    Ok(())
}

// ── Account ─────────────────────────────────────────────────────────────

#[post("/api/workspace")]
pub async fn get_workspace() -> Result<UserWorkspace, ServerFnError> {
    server::workspace::get(&session::require_user().await?).await
}

#[post("/api/workspace/defaults")]
pub async fn set_workspace_defaults(slug: String, project: Option<String>, notebook: Option<String>, finish: bool) -> Result<UserWorkspace, ServerFnError> {
    server::workspace::set(&session::require_user().await?, &slug, project.as_deref(), notebook.as_deref(), finish).await
}

/// The caller's account: profile, sign-in methods, sessions, orgs.
#[get("/api/account")]
pub async fn get_account() -> Result<Account, ServerFnError> {
    let user = session::require_user().await?;
    server::account::account(&user).await
}

/// Change the name the caller goes by (empty: their email shows instead).
#[post("/api/account/name")]
pub async fn set_display_name(name: String) -> Result<User, ServerFnError> {
    let user = session::require_user().await?;
    server::account::set_display_name(&user, &name).await
}

/// Sign out every other browser; the number of sessions ended.
#[post("/api/account/sessions/end")]
pub async fn sign_out_elsewhere() -> Result<u64, ServerFnError> {
    let user = session::require_user().await?;
    let headers = session::request_headers().await?;
    let current = session::cookie_value(&headers).map(|t| session::hash(&t));
    server::account::sign_out_elsewhere(&user, current).await
}

/// The caller's orgs, newest first.
#[get("/api/orgs")]
pub async fn list_orgs() -> Result<Vec<Org>, ServerFnError> {
    let user = session::require_user().await?;
    db::list_orgs_for(&user.id).await
}

/// Whether a slug is free for a new org, for the form to say before
/// submitting.
#[post("/api/orgs/check")]
pub async fn check_org_slug(slug: String) -> Result<SlugCheck, ServerFnError> {
    session::require_user().await?;
    db::check_org_slug(&slug.trim().to_lowercase()).await
}

/// Create an org owned by the caller, with `ledger`'s welcome grant. `409` if
/// the slug is taken, `400` if it is malformed.
#[post("/api/orgs")]
pub async fn create_org(slug: String, name: String) -> Result<Org, ServerFnError> {
    let user = session::require_user().await?;
    let slug = slug.trim().to_lowercase();
    validate_org(&slug, &name).map_err(errors::bad_request)?;
    db::create_org_for(&user.id, &slug, name.trim()).await
}

/// The caller's org and the user; `404` for non-members.
#[cfg(feature = "server")]
async fn member_org(slug: &str) -> Result<(User, Org), ServerFnError> {
    let user = session::require_user().await?;
    let org = db::org_for_member(slug.trim(), &user.id)
        .await?
        .ok_or_else(|| errors::not_found("no such organisation"))?;
    Ok((user, org))
}

/// An org's page.
#[post("/api/org")]
pub async fn get_org(slug: String) -> Result<OrgDetail, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    let credits = db::available_credits(&org.id).await;
    Ok(OrgDetail { org, credits })
}

/// Rename the org (owners and admins).
#[post("/api/org/rename")]
pub async fn rename_org(slug: String, name: String) -> Result<Org, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    server::account::rename_org(&org, &name).await
}

/// Delete the org and everything in it (owners); `confirm` is its slug.
#[post("/api/org/delete")]
pub async fn delete_org(slug: String, confirm: String) -> Result<(), ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    server::account::delete_org(&org, &confirm).await
}

/// The org's settings.
#[post("/api/org/settings")]
pub async fn get_org_settings(slug: String) -> Result<OrgSettings, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    db::org_settings(&org).await
}

/// Change the org's settings (owners and admins). `auto_repairs: None`
/// returns to the deployment's default.
#[post("/api/org/settings/save")]
pub async fn set_org_settings(
    slug: String,
    auto_repairs: Option<i32>,
) -> Result<OrgSettings, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    db::set_org_settings(&org, auto_repairs).await
}

#[post("/api/org/settings/permissions")]
pub async fn set_notebook_permissions(slug: String, policy: EffectPolicy) -> Result<OrgSettings, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    db::set_effect_policy(&org, &policy).await
}

/// The org's connections, newest first.
#[post("/api/connections")]
pub async fn list_connections(slug: String) -> Result<Vec<Connection>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::list(&org, &user).await
}

#[post("/api/connections/permissions")]
pub async fn set_connection_permissions(slug: String, connection_id: String, permissions: ConnectorPermissions) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::set_permissions(&org, &user, &connection_id, &permissions).await
}

/// Connect an S3-compatible bucket with an access key.
#[post("/api/connections/s3")]
pub async fn connect_s3(
    slug: String,
    endpoint: String,
    region: String,
    bucket: String,
    access_key_id: String,
    secret_access_key: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let form = validate_s3(
        &endpoint,
        &region,
        &bucket,
        &access_key_id,
        &secret_access_key,
    )
    .map_err(errors::bad_request)?;
    let credential = server::vault::s3(
        &form.base_url,
        &form.region,
        &form.access_key_id,
        &form.secret_access_key,
    );
    connections::store(
        &org,
        &user,
        connections::NewConnection {
            provider: Provider::S3,
            label: &form.bucket,
            base_url: &form.base_url,
            external_id: Some(&form.bucket),
        },
        credential,
    )
    .await
}

/// Connect an Azure Blob Storage container with a shared access signature.
#[post("/api/connections/azure")]
pub async fn connect_azure(
    slug: String,
    account: String,
    container: String,
    sas: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let form = validate_azure(&account, &container, &sas).map_err(errors::bad_request)?;
    let label = format!("{}/{}", form.account, form.container);
    connections::store(
        &org,
        &user,
        connections::NewConnection {
            provider: Provider::Azure,
            label: &label,
            base_url: &form.base_url,
            external_id: Some(&form.container),
        },
        server::vault::azure_sas(&form.base_url, &form.sas),
    )
    .await
}

/// Connect an AI account. An empty URL uses the provider's documented default;
/// an explicit URL supports regional/project endpoints and private deployments.
#[post("/api/connections/ai")]
pub async fn connect_ai(
    slug: String,
    provider: Provider,
    api_key: String,
    base_url: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    if !provider.is_ai() {
        return Err(errors::bad_request("not an AI provider"));
    }
    let key = if api_key.trim().is_empty() {
        server::ai::environment_key(&org, provider).ok_or_else(|| {
            errors::bad_request("no org-scoped environment key is available; paste an API key")
        })?
    } else {
        api_key
    };
    let key = validate_api_key(&key).map_err(errors::bad_request)?;
    let base_url = if base_url.trim().is_empty() {
        provider
            .fixed_base_url()
            .ok_or_else(|| errors::bad_request("enter an API base URL"))?
            .to_string()
    } else {
        validate_base_url(&base_url, true).map_err(errors::bad_request)?
    };
    let label = provider.name().to_string();
    let credential = server::vault::ai(provider, &base_url, &key);
    connections::store(
        &org,
        &user,
        connections::NewConnection {
            provider,
            label: &label,
            base_url: &base_url,
            external_id: None,
        },
        credential,
    )
    .await
}

/// Only reports availability, never values. Deployment keys are importable by
/// admins of the explicitly configured TYPEDNOTES_AI_ENV_ORG only.
#[post("/api/connections/ai/environment")]
pub async fn ai_environment(slug: String) -> Result<Vec<Provider>, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    Ok(Provider::AI
        .iter()
        .copied()
        .filter(|p| server::ai::environment_key(&org, *p).is_some())
        .collect())
}

#[post("/api/ai/pricing")]
pub async fn ai_token_pricing(
    provider: Provider,
    model: String,
) -> Result<TokenPricing, ServerFnError> {
    session::require_user().await?;
    if !provider.is_ai() || model.len() > 256 {
        return Err(errors::bad_request("invalid AI provider or model"));
    }
    Ok(server::ai::pricing(provider, model.trim()).await)
}

/// Live provider model inventory through the scoped models.list broker operation.
#[post("/api/ai/models")]
pub async fn list_ai_models(slug: String, connection_id: String) -> Result<Vec<String>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    server::ai::models(&org, &user, &connection_id).await
}

/// TypeSafe's typed classifier API, through the org's connection and broker.
#[post("/api/ai/classify")]
pub async fn classify_with_ai(
    slug: String,
    connection_id: String,
    request: serde_json::Value,
) -> Result<serde_json::Value, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    server::ai::classify(&org, &user, &connection_id, request).await
}

/// Connect a Notion internal integration or personal access token. Access is
/// limited to pages shared with the integration and its configured capabilities.
#[post("/api/connections/notion")]
pub async fn connect_notion(
    slug: String,
    label: String,
    token: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    validate_name(&label).map_err(errors::bad_request)?;
    let token = validate_api_key(&token).map_err(errors::bad_request)?;
    connections::store(
        &org,
        &user,
        connections::NewConnection {
            provider: Provider::Notion,
            label: label.trim(),
            base_url: Provider::Notion.fixed_base_url().unwrap(),
            external_id: None,
        },
        server::vault::notion(&token),
    )
    .await
}

/// Connect a CalDAV calendar collection/home with an app password.
#[post("/api/connections/caldav")]
pub async fn connect_caldav(
    slug: String,
    endpoint: String,
    username: String,
    password: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let (base_url, username) =
        validate_caldav(&endpoint, &username, &password).map_err(errors::bad_request)?;
    connections::store(
        &org,
        &user,
        connections::NewConnection {
            provider: Provider::Caldav,
            label: &username,
            base_url: &base_url,
            external_id: None,
        },
        server::vault::basic(&base_url, &username, &password),
    )
    .await
}

/// Connect Fastmail or another JMAP mail server. The credential is scoped to
/// the session URL's origin, so its API endpoint is reachable on that host too.
#[post("/api/connections/jmap")]
pub async fn connect_jmap(
    slug: String,
    endpoint: String,
    token: String,
) -> Result<Connection, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let (endpoint, token) = validate_jmap(&endpoint, &token).map_err(errors::bad_request)?;
    let url =
        url::Url::parse(&endpoint).map_err(|_| errors::bad_request("invalid JMAP session URL"))?;
    let base_url = url.origin().ascii_serialization();
    // Use the same canonical authority for both confinement and the probe.
    let endpoint = url.to_string();
    connections::store(
        &org,
        &user,
        connections::NewConnection {
            provider: Provider::Jmap,
            label: url.host_str().unwrap_or("JMAP"),
            base_url: &base_url,
            external_id: Some(&endpoint),
        },
        server::vault::bearer(&base_url, &token),
    )
    .await
}

/// Remove a connection and its credential.
#[post("/api/connections/delete")]
pub async fn delete_connection(slug: String, id: String) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::remove(&org, &user, id.trim()).await
}

/// Test a connection end to end through liaison.
#[post("/api/connections/test")]
pub async fn test_connection(slug: String, id: String) -> Result<TestResult, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::test(&org, &user, id.trim()).await
}

// ── Projects ────────────────────────────────────────────────────────────

#[post("/api/notebook/share")]
pub async fn create_notebook_share(slug: String, project: String, graph: String) -> Result<ShareCreated, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    server::shares::create(&org, &user, &project, &graph).await
}

#[post("/api/notebook/share/revoke")]
pub async fn revoke_notebook_share(slug: String, project: String, graph: String, share: String) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    server::shares::revoke(&org, &user, &project, &graph, &share).await
}

#[post("/api/notebook/shares")]
pub async fn list_notebook_shares(slug: String, project: String, graph: String) -> Result<Vec<ShareInfo>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    server::shares::list(&org, &user, &project, &graph).await
}

#[post("/api/shared-notebook")]
pub async fn get_shared_notebook(token: String) -> Result<SharedNotebook, ServerFnError> { server::shares::get(&token).await }

#[post("/api/shared-notebook/input")]
pub async fn feed_shared_notebook(token: String, cell: String, value: serde_json::Value) -> Result<SharedNotebook, ServerFnError> { server::shares::feed(&token, &cell, value).await }

/// The org's projects, newest first.
#[post("/api/projects")]
pub async fn list_projects(slug: String) -> Result<Vec<Project>, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    projects::list(&org).await
}

/// Whether a slug is free for a new project of the org.
#[post("/api/projects/check")]
pub async fn check_project_slug(slug: String, project: String) -> Result<SlugCheck, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    projects::check_slug(&org, &project.trim().to_lowercase()).await
}

/// Create a project in the org. `409` if the org already has that slug.
#[post("/api/projects/create")]
pub async fn create_project(
    slug: String,
    project: String,
    name: String,
) -> Result<Project, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    projects::create(&org, &user, &project.trim().to_lowercase(), &name).await
}

/// A project's page; `404` outside the caller's orgs.
#[post("/api/project")]
pub async fn get_project(slug: String, project: String) -> Result<ProjectDetail, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    let (project, _) = projects::get(&org, &project).await?;
    Ok(ProjectDetail { org, project })
}

/// Rename a project (its creator or an org admin).
#[post("/api/project/rename")]
pub async fn rename_project(
    slug: String,
    project: String,
    name: String,
) -> Result<Project, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    projects::rename(&org, &user, &project, &name).await
}

/// Delete a project with its interfaces and messages.
#[post("/api/project/delete")]
pub async fn delete_project(slug: String, project: String) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    projects::delete(&org, &user, &project).await
}

/// The repositories a GitHub or GitLab connection of the org can see.
#[post("/api/repos")]
pub async fn list_repos(slug: String, connection: String) -> Result<Vec<Repo>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    projects::list_repos(&org, &user, &connection).await
}

#[post("/api/repos/page")]
pub async fn list_repo_page(slug: String, connection: String, page: u32) -> Result<RepoPage, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    projects::repo_page(&org, &user, &connection, page).await
}

/// Make a repository the project's primary one.
#[post("/api/project/repo")]
pub async fn set_project_repo(
    slug: String,
    project: String,
    connection: String,
    full_name: String,
) -> Result<Project, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    projects::set_repo(&org, &user, &project, &connection, &full_name).await
}

/// Forget the project's primary repository.
#[post("/api/project/repo/clear")]
pub async fn clear_project_repo(slug: String, project: String) -> Result<Project, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    projects::clear_repo(&org, &project).await
}

// ── Interfaces and inbox ────────────────────────────────────────────────

/// The project's messaging interfaces.
#[post("/api/channels")]
pub async fn list_channels(slug: String, project: String) -> Result<Vec<Channel>, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    channels::list(&org, &project).await
}

/// The channels a Slack connection's bot can post to.
#[post("/api/slack/channels")]
pub async fn list_slack_channels(
    slug: String,
    connection: String,
) -> Result<Vec<SlackChannel>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    channels::slack_channels(&org, &user, &connection).await
}

/// Make a Slack channel an interface of the project.
#[post("/api/channels/slack")]
pub async fn add_slack_channel(
    slug: String,
    project: String,
    connection: String,
    channel: String,
) -> Result<Channel, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    channels::add_slack(&org, &user, &project, &connection, &channel).await
}

/// Reuse an existing WhatsApp or Signal sender as a project interface.
#[post("/api/channels/existing")]
pub async fn add_existing_interface(slug: String, project: String, connection: String) -> Result<Channel, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    channels::add_existing(&org, &user, &project, &connection).await
}

/// Connect a WhatsApp Cloud API number as an interface of the project.
#[post("/api/channels/whatsapp")]
pub async fn connect_whatsapp(
    slug: String,
    project: String,
    phone_number_id: String,
    access_token: String,
) -> Result<Channel, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    channels::connect_whatsapp(&org, &user, &project, &phone_number_id, &access_token).await
}

/// Connect a Signal number, through a signal-cli-rest-api bridge, as an
/// interface of the project.
#[post("/api/channels/signal")]
pub async fn connect_signal(
    slug: String,
    project: String,
    base_url: String,
    number: String,
    token: String,
) -> Result<Channel, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    channels::connect_signal(&org, &user, &project, &base_url, &number, &token).await
}

/// Remove an interface (its connection stays in the org).
#[post("/api/channels/delete")]
pub async fn remove_channel(
    slug: String,
    project: String,
    channel: String,
) -> Result<(), ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    channels::remove(&org, &project, &channel).await
}

/// The project's latest messages, newest first.
#[post("/api/inbox")]
pub async fn list_messages(slug: String, project: String) -> Result<Vec<Message>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    channels::inbox(&org, &user, &project).await
}

/// Send a message through an interface: to its channel (Slack) or to
/// `recipient` (WhatsApp, Signal).
#[post("/api/inbox/send")]
pub async fn send_message(
    slug: String,
    project: String,
    channel: String,
    recipient: String,
    text: String,
) -> Result<Message, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    channels::send(&org, &user, &project, &channel, &recipient, &text).await
}

// ── Members ─────────────────────────────────────────────────────────────

/// The org's members, owners first.
#[post("/api/members")]
pub async fn list_members(slug: String) -> Result<Vec<Member>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    members::list(&org, &user).await
}

/// Add someone to the org by email (owners and admins). An address nobody
/// signed in with yet is linked on its first sign-in.
#[post("/api/members/add")]
pub async fn add_member(
    slug: String,
    email: String,
    role: String,
) -> Result<Member, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    members::add(&org, &email, role.trim()).await
}

/// Remove a member (owners and admins), or leave (anyone).
#[post("/api/members/remove")]
pub async fn remove_member(slug: String, user_id: String) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    members::remove(&org, &user, &user_id).await
}

// ── Notebooks (docs/computations.md) ────────────────────────────────────

/// The project's notebooks, newest first.
#[post("/api/graphs")]
pub async fn list_graphs(slug: String, project: String) -> Result<Vec<Graph>, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    graphs::list(&org, &project).await
}

/// Whether a slug is free for a new notebook of the project.
#[post("/api/graphs/check")]
pub async fn check_graph_slug(
    slug: String,
    project: String,
    graph: String,
) -> Result<SlugCheck, ServerFnError> {
    let (_, org) = member_org(&slug).await?;
    graphs::check_slug(&org, &project, &graph.trim().to_lowercase()).await
}

#[post("/api/graphs/create")]
pub async fn create_graph(
    slug: String,
    project: String,
    graph: String,
    name: String,
) -> Result<Graph, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::create(&org, &user, &project, &graph.trim().to_lowercase(), &name).await
}

/// A notebook's page: its cells, their implementations and last outcomes.
#[post("/api/graph")]
pub async fn get_graph(
    slug: String,
    project: String,
    graph: String,
) -> Result<GraphDetail, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::detail(&org, &user, &project, &graph).await
}

#[post("/api/graph/delete")]
pub async fn delete_graph(
    slug: String,
    project: String,
    graph: String,
) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::delete(&org, &user, &project, &graph).await
}

/// The AI connection and model lode implements the notebook with.
#[post("/api/graph/model")]
pub async fn set_graph_model(
    slug: String,
    project: String,
    graph: String,
    connection: String,
    model: String,
) -> Result<Graph, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::set_model(&org, &user, &project, &graph, &connection, &model).await
}

/// The app's public origin, for the URLs it shows (an endpoint's).
#[cfg(feature = "server")]
async fn public_url() -> Result<String, ServerFnError> {
    Ok(server::config::public_url(
        &session::request_headers().await?,
    ))
}

/// Add a cell at the end of the notebook. An `endpoint` cell's URL is in
/// the answer, and only there.
#[post("/api/graph/cells/add")]
pub async fn add_cell(
    slug: String,
    project: String,
    graph: String,
    cell_type: CellType,
    name: String,
    description: String,
    config: CellConfig,
) -> Result<CellSaved, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let origin = public_url().await?;
    let form = graphs::CellForm {
        name: &name,
        description: &description,
        config: &config,
    };
    graphs::add_cell(&org, &user, &project, &graph, cell_type, form, &origin).await
}

#[post("/api/graph/cells/update")]
pub async fn update_cell(
    slug: String,
    project: String,
    graph: String,
    cell: String,
    name: String,
    description: String,
    config: CellConfig,
) -> Result<Cell, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let form = graphs::CellForm {
        name: &name,
        description: &description,
        config: &config,
    };
    graphs::update_cell(&org, &user, &project, &graph, &cell, form).await
}

#[post("/api/graph/cells/delete")]
pub async fn delete_cell(
    slug: String,
    project: String,
    graph: String,
    cell: String,
) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::delete_cell(&org, &user, &project, &graph, &cell).await
}

/// Move a cell up (`-1`) or down (`1`).
#[post("/api/graph/cells/move")]
pub async fn move_cell(
    slug: String,
    project: String,
    graph: String,
    cell: String,
    delta: i32,
) -> Result<(), ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::move_cell(&org, &user, &project, &graph, &cell, delta).await
}

/// Set a secret cell's value. Write-only: nothing ever reads it back.
#[post("/api/graph/cells/secret")]
pub async fn set_cell_secret(
    slug: String,
    project: String,
    graph: String,
    cell: String,
    value: String,
) -> Result<Cell, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::set_secret(&org, &user, &project, &graph, &cell, &value).await
}

/// A new URL for an endpoint cell; the old one stops working.
#[post("/api/graph/cells/rotate")]
pub async fn rotate_endpoint(
    slug: String,
    project: String,
    graph: String,
    cell: String,
) -> Result<CellSaved, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    let origin = public_url().await?;
    graphs::rotate_endpoint(&org, &user, &project, &graph, &cell, &origin).await
}

/// Feed a `ui` input cell's value to the running graph.
#[post("/api/graph/cells/feed")]
pub async fn feed_cell(
    slug: String,
    project: String,
    graph: String,
    cell: String,
    value: serde_json::Value,
) -> Result<FeedResult, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::feed_ui(&org, &user, &project, &graph, &cell, value).await
}

/// Ask lode to implement the notebook's cells (with an optional note), or
/// — `steer` while it runs — to change course.
#[post("/api/graph/implement")]
pub async fn implement_graph(
    slug: String,
    project: String,
    graph: String,
    note: String,
    steer: bool,
) -> Result<Graph, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::implement(&org, &user, &project, &graph, &note, steer).await
}

/// Tell lode a cell does not do the right thing: it rewrites the cell's
/// code while the current code keeps running.
#[post("/api/graph/cells/report")]
pub async fn report_cell(
    slug: String,
    project: String,
    graph: String,
    cell: String,
    report: String,
) -> Result<Graph, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::report_cell(&org, &user, &project, &graph, &cell, &report).await
}

/// A cell's code: published, and lode's unpublished changes to it.
#[post("/api/graph/cells/code")]
pub async fn cell_code(
    slug: String,
    project: String,
    graph: String,
    cell: String,
) -> Result<CellCode, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::cell_code(&org, &user, &project, &graph, &cell).await
}

#[post("/api/graph/abort")]
pub async fn abort_graph(
    slug: String,
    project: String,
    graph: String,
) -> Result<Graph, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::abort(&org, &user, &project, &graph).await
}

/// lode's log from `after`, waiting up to `wait` seconds for news; moves
/// the notebook on (build, then session) once lode's run is over.
#[post("/api/graph/progress")]
pub async fn graph_progress(
    slug: String,
    project: String,
    graph: String,
    after: u64,
    wait: u64,
) -> Result<LodeProgress, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::progress(&org, &user, &project, &graph, after, wait).await
}

/// Build the notebook again from the repository's branch head.
#[post("/api/graph/rebuild")]
pub async fn rebuild_graph(
    slug: String,
    project: String,
    graph: String,
) -> Result<Graph, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::rebuild(&org, &user, &project, &graph).await
}

/// Register the session again from the recorded inputs.
#[post("/api/graph/restart")]
pub async fn restart_graph(
    slug: String,
    project: String,
    graph: String,
) -> Result<FeedResult, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    graphs::restart_session(&org, &user, &project, &graph).await
}
