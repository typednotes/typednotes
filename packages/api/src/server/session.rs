//! Browser sessions and the users behind them (docs/connections.md §2).
//!
//! The cookie `tn_session` carries 32 random bytes (base64url). The database
//! stores only their SHA-256, so a leaked `sessions` table is not a set of
//! usable cookies.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dioxus::fullstack::{FullstackContext, HeaderMap};
use dioxus::prelude::ServerFnError;
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::db::pool;
use super::errors::{db_error, forbidden, internal, unauthorized};
use crate::User;

pub const COOKIE: &str = "tn_session";
const LIFETIME_SECS: i64 = 30 * 24 * 3600;

/// `n` bytes from the OS CSPRNG, base64url without padding.
pub fn random_token(n: usize) -> Result<String, ServerFnError> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| internal(format!("no randomness: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// The session cookie's value, from every `Cookie` header of the request.
pub fn cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// `Set-Cookie` for a new session. `Secure` unless the app is served over
/// plain http (local development), where browsers other than Chrome would
/// drop a secure cookie.
pub fn set_cookie(token: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={LIFETIME_SECS}{secure}")
}

pub fn clear_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}")
}

/// Open a session for `user_id`; returns the cookie value. Expired sessions
/// and OAuth flows are swept here, on the one path every user goes through.
pub async fn create(user_id: &str) -> Result<String, ServerFnError> {
    let pool = pool()?;
    let _ = sqlx::query("delete from sessions where expires_at < now()")
        .execute(pool)
        .await;
    let _ = sqlx::query("delete from oauth_flows where expires_at < now()")
        .execute(pool)
        .await;
    let token = random_token(32)?;
    sqlx::query(
        "insert into sessions (id_hash, user_id, expires_at) \
         values ($1, $2::uuid, now() + make_interval(secs => $3))",
    )
    .bind(hash(&token))
    .bind(user_id)
    .bind(LIFETIME_SECS as f64)
    .execute(pool)
    .await
    .map_err(db_error)?;
    Ok(token)
}

/// The signed-in user for these request headers, if any.
pub async fn user_for(headers: &HeaderMap) -> Result<Option<User>, ServerFnError> {
    let Some(token) = cookie_value(headers) else {
        return Ok(None);
    };
    let row = sqlx::query(
        "select u.id::text as id, u.email::text as email, u.display_name \
         from sessions s join users u on u.id = s.user_id \
         where s.id_hash = $1 and s.expires_at > now() and u.deleted_at is null",
    )
    .bind(hash(&token))
    .fetch_optional(pool()?)
    .await
    .map_err(db_error)?;
    Ok(row.map(|row| User {
        id: row.get("id"),
        email: row.get("email"),
        display_name: row.get("display_name"),
    }))
}

pub async fn destroy(headers: &HeaderMap) -> Result<(), ServerFnError> {
    if let Some(token) = cookie_value(headers) {
        sqlx::query("delete from sessions where id_hash = $1")
            .bind(hash(&token))
            .execute(pool()?)
            .await
            .map_err(db_error)?;
    }
    Ok(())
}

/// The current request's headers, inside a server function or an SSR render.
pub async fn request_headers() -> Result<HeaderMap, ServerFnError> {
    FullstackContext::extract::<HeaderMap, _>().await
}

pub async fn current_user() -> Result<Option<User>, ServerFnError> {
    user_for(&request_headers().await?).await
}

/// The signed-in user, or `401`.
pub async fn require_user() -> Result<User, ServerFnError> {
    current_user().await?.ok_or_else(unauthorized)
}

/// What a provider vouches for about the person signing in.
pub struct ProviderIdentity {
    pub issuer: &'static str,
    pub subject: String,
    /// Verified by the provider; unverified addresses are refused upstream.
    /// The email a new user gets.
    pub email: String,
    /// Every address the provider verified for this person, `email` first:
    /// any of them links to an existing user.
    pub verified_emails: Vec<String>,
    pub display_name: Option<String>,
}

/// The user for a provider identity, creating or linking as needed:
///
/// 1. a known `(issuer, subject)` is that user;
/// 2. else an existing user whose email is one of the identity's verified
///    addresses gets a new identity — so signing in with Google, then with
///    GitHub under the same address, is one account. Emails compare
///    case-insensitively: `users.email` is `citext`, and the parameter is
///    cast to it (a `text` parameter would compare case-sensitively);
/// 3. else a new user.
///
/// A soft-deleted user is refused rather than resurrected. Two first
/// sign-ins racing on the same email both end on the same user.
pub async fn user_for_identity(id: &ProviderIdentity) -> Result<String, ServerFnError> {
    let mut tx = pool()?.begin().await.map_err(db_error)?;

    let known = sqlx::query(
        "select i.user_id::text as user_id, u.deleted_at is not null as deleted \
         from identities i join users u on u.id = i.user_id \
         where i.issuer = $1 and i.subject = $2",
    )
    .bind(id.issuer)
    .bind(&id.subject)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;

    let user_id = if let Some(row) = known {
        if row.get::<bool, _>("deleted") {
            return Err(forbidden("this account has been deleted"));
        }
        let user_id: String = row.get("user_id");
        sqlx::query(
            "update identities set last_login_at = now() where issuer = $1 and subject = $2",
        )
        .bind(id.issuer)
        .bind(&id.subject)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        user_id
    } else {
        let user_id = match existing_user(&mut tx, &id.verified_emails).await? {
            Some((_, true)) => return Err(forbidden("this account has been deleted")),
            Some((user_id, false)) => user_id,
            None => {
                let inserted = sqlx::query(
                    "insert into users (email, display_name) values ($1, $2) \
                     on conflict (email) do nothing returning id::text as id",
                )
                .bind(&id.email)
                .bind(&id.display_name)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db_error)?;
                match inserted {
                    Some(row) => row.get("id"),
                    // Created by a concurrent sign-in since the lookup.
                    None => match existing_user(&mut tx, std::slice::from_ref(&id.email)).await? {
                        Some((_, true)) => return Err(forbidden("this account has been deleted")),
                        Some((user_id, false)) => user_id,
                        None => return Err(internal("could not create the account")),
                    },
                }
            }
        };
        sqlx::query(
            "insert into identities (user_id, issuer, subject, last_login_at) \
             values ($1::uuid, $2, $3, now())",
        )
        .bind(&user_id)
        .bind(id.issuer)
        .bind(&id.subject)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        user_id
    };

    sqlx::query("update users set display_name = $2 where id = $1::uuid and display_name is null")
        .bind(&user_id)
        .bind(&id.display_name)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(user_id)
}

/// The user (id, soft-deleted?) whose email is one of `emails`, preferring
/// earlier addresses.
async fn existing_user(
    tx: &mut sqlx::PgConnection,
    emails: &[String],
) -> Result<Option<(String, bool)>, ServerFnError> {
    let row = sqlx::query(
        "select u.id::text as id, u.deleted_at is not null as deleted \
         from unnest($1::text[]) with ordinality as e(email, n) \
         join users u on u.email = e.email::citext \
         order by e.n limit 1",
    )
    .bind(emails)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    Ok(row.map(|row| (row.get("id"), row.get("deleted"))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::fullstack::HeaderValue;

    #[test]
    fn finds_the_session_cookie_among_others() {
        let mut h = HeaderMap::new();
        h.append(
            "cookie",
            HeaderValue::from_static("a=1; tn_session=abc; b=2"),
        );
        assert_eq!(cookie_value(&h).as_deref(), Some("abc"));
        h.clear();
        h.append("cookie", HeaderValue::from_static("x=1"));
        h.append("cookie", HeaderValue::from_static("tn_session=def"));
        assert_eq!(cookie_value(&h).as_deref(), Some("def"));
        h.clear();
        h.append(
            "cookie",
            HeaderValue::from_static("tn_session_old=zzz; tn_session="),
        );
        assert_eq!(cookie_value(&h), None);
    }

    #[test]
    fn cookie_attributes() {
        let c = set_cookie("t", true);
        assert!(c.starts_with("tn_session=t; Path=/; HttpOnly; SameSite=Lax; Max-Age="));
        assert!(c.ends_with("; Secure"));
        assert!(!set_cookie("t", false).contains("Secure"));
        assert!(clear_cookie(true).contains("Max-Age=0"));
    }

    #[test]
    fn tokens_are_random_and_long() {
        let (a, b) = (random_token(32).unwrap(), random_token(32).unwrap());
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
        assert_eq!(hash(&a).len(), 32);
    }
}
