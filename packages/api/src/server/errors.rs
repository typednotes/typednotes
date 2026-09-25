//! `ServerFnError` constructors with the HTTP status each failure means, so
//! call sites say *what* went wrong and the client still gets a status code.

use dioxus::prelude::ServerFnError;

/// The server's own message, without the "error running server function:
/// … (details: None)" wrapping `Display` adds — for text that ends up in a
/// redirect's `?error=` and is shown as is.
pub fn message(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

fn with_code(code: u16, message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code,
        details: None,
    }
}

pub fn bad_request(message: impl Into<String>) -> ServerFnError {
    with_code(400, message)
}

pub fn unauthorized() -> ServerFnError {
    with_code(401, "sign in first")
}

pub fn forbidden(message: impl Into<String>) -> ServerFnError {
    with_code(403, message)
}

pub fn not_found(message: impl Into<String>) -> ServerFnError {
    with_code(404, message)
}

pub fn conflict(message: impl Into<String>) -> ServerFnError {
    with_code(409, message)
}

pub fn internal(message: impl Into<String>) -> ServerFnError {
    with_code(500, message)
}

/// A dependency (the vault, liaison, a provider) failed. Logged in full; the
/// client gets `message`, which must never carry a credential.
pub fn bad_gateway(message: impl Into<String>) -> ServerFnError {
    with_code(502, message)
}

/// A service this deployment has not been configured for (missing env var).
pub fn unavailable(message: impl Into<String>) -> ServerFnError {
    with_code(503, message)
}

pub fn db_error(e: sqlx::Error) -> ServerFnError {
    // Logged in full server-side; the client gets the message, which carries
    // no credentials (the URL never appears in sqlx's query errors).
    eprintln!("database error: {e}");
    internal(format!("database error: {e}"))
}
