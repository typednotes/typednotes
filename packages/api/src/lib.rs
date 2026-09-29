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

mod validate;
pub use validate::*;

mod computation;
pub use computation::*;

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

/// The org's connections, newest first.
#[post("/api/connections")]
pub async fn list_connections(slug: String) -> Result<Vec<Connection>, ServerFnError> {
    let (user, org) = member_org(&slug).await?;
    connections::list(&org, &user).await
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
            external_id: None,
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
            external_id: None,
        },
        server::vault::azure_sas(&form.base_url, &form.sas),
    )
    .await
}

/// Connect an AI account with an API token. `base_url` is used only for
/// `OpenaiCompatible`.
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
    let key = validate_api_key(&api_key).map_err(errors::bad_request)?;
    let base_url = match provider.fixed_base_url() {
        Some(fixed) => fixed.to_string(),
        None => validate_base_url(&base_url, true).map_err(errors::bad_request)?,
    };
    let (label, credential) = match provider {
        Provider::Anthropic => (
            provider.name().to_string(),
            server::vault::header(
                &base_url,
                "x-api-key",
                &key,
                &[("anthropic-version", "2023-06-01")],
            ),
        ),
        Provider::OpenaiCompatible => {
            let host = base_url.split("://").nth(1).unwrap_or(&base_url);
            (host.to_string(), server::vault::bearer(&base_url, &key))
        }
        _ => (
            provider.name().to_string(),
            server::vault::bearer(&base_url, &key),
        ),
    };
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
