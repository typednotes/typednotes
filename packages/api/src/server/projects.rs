//! Projects: work inside an org, each with an optional primary repository
//! read through a GitHub or GitLab connection (docs/connections.md §10).
//!
//! Every query is scoped by the org the caller was checked to be a member
//! of (`member_org`), so a project of another org is `404`, like the org.

use dioxus::prelude::ServerFnError;
use serde::Deserialize;
use sqlx::postgres::PgRow;
use sqlx::Row;

use super::connections::{self, ProviderCall};
use super::db::{pool, slug_check};
use super::errors::{bad_gateway, bad_request, conflict, db_error, forbidden, not_found};
use crate::{
    validate_org, validate_repo_name, Org, Project, Provider, Repo, RepoRef, SlugCheck, User,
};

macro_rules! project_columns {
    () => {
        "p.id::text as id, p.slug::text as slug, p.name, \
         to_char(p.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as created_at, \
         p.repo_connection_id::text as repo_connection_id, p.repo_provider, p.repo_full_name, \
         p.repo_web_url, p.repo_default_branch, p.created_by::text as created_by"
    };
}

fn project_of(row: &PgRow) -> Project {
    let repo_provider: Option<String> = row.get("repo_provider");
    let repo = repo_provider
        .as_deref()
        .and_then(Provider::from_id)
        .map(|provider| RepoRef {
            provider,
            full_name: row.get("repo_full_name"),
            web_url: row.get("repo_web_url"),
            default_branch: row.get("repo_default_branch"),
            connection_id: row.get("repo_connection_id"),
        });
    Project {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        created_at: row.get("created_at"),
        repo,
    }
}

pub async fn list(org: &Org) -> Result<Vec<Project>, ServerFnError> {
    let rows = sqlx::query(concat!(
        "select ",
        project_columns!(),
        " from projects p where p.org_id = $1::uuid order by p.created_at desc"
    ))
    .bind(&org.id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().map(project_of).collect())
}

/// The project called `slug` in `org`, and who created it.
pub async fn get(org: &Org, slug: &str) -> Result<(Project, Option<String>), ServerFnError> {
    let row = sqlx::query(concat!(
        "select ",
        project_columns!(),
        " from projects p where p.org_id = $1::uuid and p.slug = $2::citext"
    ))
    .bind(&org.id)
    .bind(slug.trim())
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("no such project"))?;
    Ok((project_of(&row), row.get("created_by")))
}

/// Whether `slug` is free for a new project of `org`.
pub async fn check_slug(org: &Org, slug: &str) -> Result<SlugCheck, ServerFnError> {
    if let Err(message) = crate::validate_slug(slug) {
        return Ok(SlugCheck {
            available: false,
            message,
        });
    }
    let taken = sqlx::query("select 1 from projects where org_id = $1::uuid and slug = $2::citext")
        .bind(&org.id)
        .bind(slug)
        .fetch_optional(pool()?)
        .await
        .map_err(db_error)?
        .is_some();
    Ok(slug_check(slug, taken))
}

pub async fn create(
    org: &Org,
    user: &User,
    slug: &str,
    name: &str,
) -> Result<Project, ServerFnError> {
    validate_org(slug, name).map_err(bad_request)?;
    let inserted = sqlx::query(concat!(
        "insert into projects as p (org_id, slug, name, created_by) \
         values ($1::uuid, $2, $3, $4::uuid) returning ",
        project_columns!()
    ))
    .bind(&org.id)
    .bind(slug)
    .bind(name.trim())
    .bind(&user.id)
    .fetch_one(pool()?)
    .await;
    match inserted {
        Ok(row) => Ok(project_of(&row)),
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            Err(conflict(format!("this org already has a project '{slug}'")))
        }
        Err(e) => Err(db_error(e)),
    }
}

/// Delete a project and its channels and messages: its creator or an org
/// admin.
pub async fn delete(org: &Org, user: &User, slug: &str) -> Result<(), ServerFnError> {
    let (project, created_by) = get(org, slug).await?;
    let admin = org.role == "owner" || org.role == "admin";
    if !admin && created_by.as_deref() != Some(user.id.as_str()) {
        return Err(forbidden(
            "only its creator or an org admin can delete this project",
        ));
    }
    sqlx::query("delete from projects where id = $1::uuid")
        .bind(&project.id)
        .execute(pool()?)
        .await
        .map_err(db_error)?;
    Ok(())
}

// ── Repositories ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GithubRepo {
    full_name: String,
    html_url: String,
    default_branch: Option<String>,
    #[serde(default)]
    private: bool,
}

#[derive(Deserialize)]
struct GitlabProject {
    path_with_namespace: String,
    web_url: String,
    default_branch: Option<String>,
    visibility: Option<String>,
}

impl From<GithubRepo> for Repo {
    fn from(r: GithubRepo) -> Self {
        Repo {
            full_name: r.full_name,
            web_url: r.html_url,
            default_branch: r.default_branch,
            private: r.private,
        }
    }
}

impl From<GitlabProject> for Repo {
    fn from(p: GitlabProject) -> Self {
        Repo {
            full_name: p.path_with_namespace,
            web_url: p.web_url,
            default_branch: p.default_branch,
            private: p.visibility.as_deref().is_some_and(|v| v != "public"),
        }
    }
}

fn github_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("accept", "application/vnd.github+json"),
        ("user-agent", "typednotes"),
    ]
}

/// Parse a repository listing (or, with `one`, a single repository).
fn parse_repos(provider: Provider, body: &[u8]) -> Result<Vec<Repo>, String> {
    let unreadable = |e: serde_json::Error| format!("unreadable repository list: {e}");
    match provider {
        Provider::Github => serde_json::from_slice::<Vec<GithubRepo>>(body)
            .map(|rs| rs.into_iter().map(Repo::from).collect())
            .map_err(unreadable),
        Provider::Gitlab => serde_json::from_slice::<Vec<GitlabProject>>(body)
            .map(|ps| ps.into_iter().map(Repo::from).collect())
            .map_err(unreadable),
        _ => Err("not a code host".to_string()),
    }
}

fn parse_repo(provider: Provider, body: &[u8]) -> Result<Repo, String> {
    let unreadable = |e: serde_json::Error| format!("unreadable repository: {e}");
    match provider {
        Provider::Github => serde_json::from_slice::<GithubRepo>(body)
            .map(Repo::from)
            .map_err(unreadable),
        Provider::Gitlab => serde_json::from_slice::<GitlabProject>(body)
            .map(Repo::from)
            .map_err(unreadable),
        _ => Err("not a code host".to_string()),
    }
}

/// The URL of one repository on the connection's API. GitLab takes the
/// whole path as one URL-encoded segment.
fn repo_url(provider: Provider, base_url: &str, full_name: &str) -> String {
    match provider {
        Provider::Gitlab => format!("{base_url}/projects/{}", full_name.replace('/', "%2F")),
        _ => format!("{base_url}/repos/{full_name}"),
    }
}

/// A code connection of `org`.
async fn code_connection(
    org: &Org,
    user: &User,
    connection_id: &str,
) -> Result<(crate::Connection, String), ServerFnError> {
    let (connection, owner) = connections::get(org, user, connection_id.trim()).await?;
    if !connection.provider.is_code() {
        return Err(bad_request("not a GitHub or GitLab connection"));
    }
    Ok((connection, owner))
}

/// The repositories a code connection can see, most recently active first
/// (the first hundred).
pub async fn list_repos(
    org: &Org,
    user: &User,
    connection_id: &str,
) -> Result<Vec<Repo>, ServerFnError> {
    let (connection, owner) = code_connection(org, user, connection_id).await?;
    let request = match connection.provider {
        Provider::Github => ProviderCall {
            action: "read",
            method: "GET",
            url: format!(
                "{}/user/repos?per_page=100&sort=updated",
                connection.base_url
            ),
            headers: github_headers(),
            body: None,
        },
        _ => ProviderCall {
            action: "read",
            method: "GET",
            url: format!(
                "{}/projects?membership=true&per_page=100&order_by=last_activity_at",
                connection.base_url
            ),
            headers: vec![("accept", "application/json")],
            body: None,
        },
    };
    let body = connections::call_ok(org, &connection, &owner, request).await?;
    parse_repos(connection.provider, &body).map_err(bad_gateway)
}

/// Make `full_name` the project's primary repository, after reading it
/// through the connection — so it exists, the connection can see it, and
/// its URL and default branch are the host's, not the client's.
pub async fn set_repo(
    org: &Org,
    user: &User,
    project_slug: &str,
    connection_id: &str,
    full_name: &str,
) -> Result<Project, ServerFnError> {
    let (project, _) = get(org, project_slug).await?;
    let (connection, owner) = code_connection(org, user, connection_id).await?;
    let full_name = validate_repo_name(connection.provider, full_name).map_err(bad_request)?;
    let headers = match connection.provider {
        Provider::Github => github_headers(),
        _ => vec![("accept", "application/json")],
    };
    let body = connections::call_ok(
        org,
        &connection,
        &owner,
        ProviderCall {
            action: "read",
            method: "GET",
            url: repo_url(connection.provider, &connection.base_url, &full_name),
            headers,
            body: None,
        },
    )
    .await?;
    let repo = parse_repo(connection.provider, &body).map_err(bad_gateway)?;
    sqlx::query(
        "update projects set repo_connection_id = $2::uuid, repo_provider = $3, \
         repo_full_name = $4, repo_web_url = $5, repo_default_branch = $6 where id = $1::uuid",
    )
    .bind(&project.id)
    .bind(&connection.id)
    .bind(connection.provider.id())
    .bind(&repo.full_name)
    .bind(&repo.web_url)
    .bind(&repo.default_branch)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(get(org, project_slug).await?.0)
}

pub async fn clear_repo(org: &Org, project_slug: &str) -> Result<Project, ServerFnError> {
    let (project, _) = get(org, project_slug).await?;
    sqlx::query(
        "update projects set repo_connection_id = null, repo_provider = null, \
         repo_full_name = null, repo_web_url = null, repo_default_branch = null \
         where id = $1::uuid",
    )
    .bind(&project.id)
    .execute(pool()?)
    .await
    .map_err(db_error)?;
    Ok(get(org, project_slug).await?.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_and_gitlab_listings() {
        let gh = br#"[{"full_name":"octo/hello","html_url":"https://github.com/octo/hello",
                      "default_branch":"main","private":true,"id":1}]"#;
        assert_eq!(
            parse_repos(Provider::Github, gh).unwrap(),
            vec![Repo {
                full_name: "octo/hello".into(),
                web_url: "https://github.com/octo/hello".into(),
                default_branch: Some("main".into()),
                private: true,
            }]
        );
        let gl =
            br#"[{"path_with_namespace":"grp/sub/proj","web_url":"https://gitlab.com/grp/sub/proj",
                      "default_branch":null,"visibility":"internal"}]"#;
        let repos = parse_repos(Provider::Gitlab, gl).unwrap();
        assert_eq!(repos[0].full_name, "grp/sub/proj");
        assert!(repos[0].private && repos[0].default_branch.is_none());
        assert!(parse_repos(Provider::Github, b"{}").is_err());
        assert!(parse_repo(
            Provider::Github,
            br#"{"full_name":"a/b","html_url":"https://github.com/a/b"}"#
        )
        .is_ok());
    }

    #[test]
    fn repository_urls() {
        assert_eq!(
            repo_url(Provider::Github, "https://api.github.com", "octo/hello"),
            "https://api.github.com/repos/octo/hello"
        );
        assert_eq!(
            repo_url(
                Provider::Gitlab,
                "https://gitlab.com/api/v4",
                "grp/sub/proj"
            ),
            "https://gitlab.com/api/v4/projects/grp%2Fsub%2Fproj"
        );
    }
}
