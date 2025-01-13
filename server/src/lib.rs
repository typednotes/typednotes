//! This crate contains all shared fullstack server functions.
#[cfg(feature = "server")]
mod application;
#[cfg(feature = "server")]
mod auth;
#[cfg(feature = "server")]
mod database;
#[cfg(feature = "server")]
mod settings;
#[cfg(feature = "server")]
mod user;

#[cfg(feature = "server")]
pub use application::launch;
use dioxus::prelude::*;

/// Echo the user input on the server.
#[server(Echo)]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    Ok(format!("Hello world {input}"))
}

/// Echo the user input on the server.
#[server(Test)]
pub async fn test(input: String) -> Result<String, ServerFnError> {
    Ok(format!("Test {input}"))
}

// Auth related endpoints

#[server(GetUserName)]
pub async fn get_user_name() -> Result<String, ServerFnError> {
    let session: auth::Session = extract().await?;
    Ok(session.0.current_user.unwrap().username.to_string())
}

#[server(Login)]
pub async fn login() -> Result<(), ServerFnError> {
    let auth: auth::Session = extract().await?;
    auth.login_user(2);
    Ok(())
}

#[server(Permissions)]
pub async fn get_permissions() -> Result<String, ServerFnError> {
    let method: axum::http::Method = extract().await?;
    let auth: auth::Session = extract().await?;
    let current_user = auth.current_user.clone().context("No user")?;

    // lets check permissions only and not worry about if they are anon or not
    if !axum_session_auth::Auth::<user::User, i32, sqlx::PgPool>::build(
        [axum::http::Method::POST],
        false,
    )
    .requires(axum_session_auth::Rights::any([
        axum_session_auth::Rights::permission("Category::View"),
        axum_session_auth::Rights::permission("Admin::View"),
    ]))
    .validate(&current_user, &method, None)
    .await
    {
        return Ok(format!(
            "User {}, Does not have permissions needed to view this page please login",
            current_user.username
        ));
    }

    Ok(format!(
        "User has Permissions needed. Here are the Users permissions: {:?}",
        current_user.permissions
    ))
}