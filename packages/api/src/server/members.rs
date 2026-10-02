//! An org's members (docs/services/core.md §4, `memberships`): owners and
//! admins add people by email and remove them; anyone may leave.
//!
//! Adding an address nobody has signed in with yet creates the `users` row
//! for it, with no identity: the first sign-in with that verified address
//! (GitHub or Google) links to it (`session::user_for_identity`), so an
//! invitation is just a membership waiting for its user.

use dioxus::prelude::ServerFnError;
use sqlx::postgres::PgRow;
use sqlx::Row;

use super::db::pool;
use super::errors::{bad_request, conflict, db_error, forbidden, not_found};
use crate::{may_manage, Member, Org, User, ROLES};

fn member_of(row: &PgRow, org: &Org, user: &User) -> Member {
    let user_id: String = row.get("user_id");
    let role: String = row.get("role");
    let is_you = user_id == user.id;
    Member {
        can_remove: is_you || may_manage(&org.role, &role),
        is_you,
        user_id,
        email: row.get("email"),
        display_name: row.get("display_name"),
        role,
        added_at: row.get("added_at"),
        signed_in: row.get("signed_in"),
    }
}

pub async fn list(org: &Org, user: &User) -> Result<Vec<Member>, ServerFnError> {
    let rows = sqlx::query(
        "select m.user_id::text as user_id, u.email::text as email, u.display_name, m.role, \
         to_char(m.added_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI \"UTC\"') as added_at, \
         exists (select 1 from identities i where i.user_id = u.id) as signed_in \
         from memberships m join users u on u.id = m.user_id \
         where m.org_id = $1::uuid and u.deleted_at is null \
         order by case m.role when 'owner' then 0 when 'admin' then 1 else 2 end, m.added_at",
    )
    .bind(&org.id)
    .fetch_all(pool()?)
    .await
    .map_err(db_error)?;
    Ok(rows.iter().map(|r| member_of(r, org, user)).collect())
}

/// A plausible email address: one `@`, a dot in the domain, no spaces.
pub fn validate_email(email: &str) -> Result<String, String> {
    let email = email.trim();
    let ok = (3..=254).contains(&email.len())
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains('@')
        });
    if ok {
        Ok(email.to_string())
    } else {
        Err("email: an address like ada@example.com".to_string())
    }
}

/// Add `email` to `org` as `role`. `409` if already a member.
pub async fn add(org: &Org, email: &str, role: &str) -> Result<Member, ServerFnError> {
    let email = validate_email(email).map_err(bad_request)?;
    if !ROLES.contains(&role) {
        return Err(bad_request("role: owner, admin or member"));
    }
    if !may_manage(&org.role, role) {
        return Err(forbidden(format!(
            "a{} {} cannot add a{} {role}",
            if org.role == "admin" { "n" } else { "" },
            org.role,
            if role == "admin" || role == "owner" {
                "n"
            } else {
                ""
            }
        )));
    }
    let mut tx = super::connector::lock(&org.id).await?;
    sqlx::query("select id from orgs where id=$1::uuid for update").bind(&org.id)
        .fetch_one(&mut *tx).await.map_err(db_error)?;
    // The user, or a new one waiting for its first sign-in.
    sqlx::query("insert into users (email) values ($1::citext) on conflict (email) do nothing")
        .bind(&email)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    let row = sqlx::query(
        "select id::text as id, deleted_at is not null as deleted from users where email = $1::citext",
    )
    .bind(&email)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_error)?;
    if row.get::<bool, _>("deleted") {
        return Err(forbidden("this account has been deleted"));
    }
    let user_id: String = row.get("id");
    let inserted = sqlx::query(
        "insert into memberships (org_id, user_id, role) values ($1::uuid, $2::uuid, $3) \
         on conflict do nothing",
    )
    .bind(&org.id)
    .bind(&user_id)
    .bind(role)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    if inserted.rows_affected() == 0 {
        return Err(conflict(format!("{email} is already a member")));
    }
    tx.commit().await.map_err(db_error)?;
    Ok(Member {
        user_id,
        email,
        display_name: None,
        role: role.to_string(),
        added_at: String::new(),
        signed_in: false,
        is_you: false,
        can_remove: true,
    })
}

/// Remove `user_id` from `org`: the caller themself (leaving), or someone
/// the caller's role manages. The last owner cannot go — an org always has
/// someone who can manage it.
pub async fn remove(org: &Org, user: &User, user_id: &str) -> Result<(), ServerFnError> {
    let mut tx = super::connector::lock(&org.id).await?;
    sqlx::query("select id from orgs where id=$1::uuid for update").bind(&org.id)
        .fetch_one(&mut *tx).await.map_err(db_error)?;
    // Lock the org's memberships, so two removals cannot both see "another
    // owner remains".
    let rows = sqlx::query(
        "select user_id::text as user_id, role from memberships where org_id = $1::uuid for update",
    )
    .bind(&org.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    let members: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.get("user_id"), r.get("role")))
        .collect();
    let (_, role) = members
        .iter()
        .find(|(id, _)| id == user_id.trim())
        .ok_or_else(|| not_found("no such member"))?;
    let leaving = user_id.trim() == user.id;
    if !leaving && !may_manage(&org.role, role) {
        return Err(forbidden(format!("a {} cannot remove a {role}", org.role)));
    }
    let owners = members.iter().filter(|(_, r)| r == "owner").count();
    if role == "owner" && owners <= 1 {
        return Err(conflict(
            "an org needs an owner: add another owner before removing this one",
        ));
    }
    sqlx::query("delete from memberships where org_id = $1::uuid and user_id = $2::uuid")
        .bind(&org.id)
        .bind(user_id.trim())
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(())
}

/// Existing members can become owners before an account is deleted. Role
/// changes are serialized with deletion; the last owner cannot be demoted.
pub async fn set_role(org: &Org, user: &User, user_id: &str, role: &str) -> Result<(), ServerFnError> {
    if !ROLES.contains(&role) { return Err(bad_request("role: owner, admin or member")); }
    let mut tx = super::connector::lock(&org.id).await?;
    sqlx::query("select id from orgs where id=$1::uuid for update").bind(&org.id)
        .fetch_one(&mut *tx).await.map_err(db_error)?;
    let rows = sqlx::query("select user_id::text as id, role from memberships where org_id=$1::uuid")
        .bind(&org.id).fetch_all(&mut *tx).await.map_err(db_error)?;
    let actor = rows.iter().find(|r| r.get::<String, _>("id") == user.id)
        .ok_or_else(|| forbidden("you no longer belong to this organization"))?.get::<String, _>("role");
    let current = rows.iter().find(|r| r.get::<String, _>("id") == user_id.trim())
        .ok_or_else(|| not_found("no such member"))?.get::<String, _>("role");
    if !may_manage(&actor, &current) || !may_manage(&actor, role) {
        return Err(forbidden("your role cannot make this membership change"));
    }
    if current == "owner" && role != "owner" && rows.iter().filter(|r| r.get::<String, _>("role") == "owner").count() <= 1 {
        return Err(conflict("add another owner before changing the last owner's role"));
    }
    sqlx::query("update memberships set role=$3 where org_id=$1::uuid and user_id=$2::uuid")
        .bind(&org.id).bind(user_id.trim()).bind(role).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails() {
        assert_eq!(
            validate_email(" ada@example.com ").unwrap(),
            "ada@example.com"
        );
        for bad in [
            "ada",
            "ada@",
            "@example.com",
            "ada@example",
            "a da@x.com",
            "a@b@c.com",
            "a@.com",
        ] {
            assert!(validate_email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn who_manages_whom() {
        assert!(may_manage("owner", "owner") && may_manage("owner", "member"));
        assert!(may_manage("admin", "admin") && may_manage("admin", "member"));
        assert!(!may_manage("admin", "owner"));
        assert!(!may_manage("member", "member"));
    }
}
