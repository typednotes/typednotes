//! Environment configuration (docs/connections.md §8). Every variable is read
//! when used, not at startup: a missing one disables its feature and says so,
//! rather than stopping a server that can still serve everything else.

use dioxus::fullstack::HeaderMap;

/// A non-empty environment variable.
pub fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// An OAuth client registered with a provider.
pub struct OAuthClient {
    pub id: String,
    pub secret: String,
}

fn client(id: &str, secret: &str) -> Option<OAuthClient> {
    Some(OAuthClient {
        id: env(id)?,
        secret: env(secret)?,
    })
}

/// The GitHub OAuth App: sign-in and the `github` connection.
pub fn github() -> Option<OAuthClient> {
    client("GITHUB_CLIENT_ID", "GITHUB_CLIENT_SECRET")
}

/// The Google OAuth client: sign-in and the `gdrive` connection.
pub fn google() -> Option<OAuthClient> {
    client("GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET")
}

/// Credits granted to a new org (`ledger`'s welcome grant). `0` disables it.
pub fn welcome_credits() -> i64 {
    match env("TYPEDNOTES_WELCOME_CREDITS") {
        None => 1000,
        Some(v) => v
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 0)
            .unwrap_or_else(|| {
                eprintln!(
                    "TYPEDNOTES_WELCOME_CREDITS={v:?} is not a non-negative integer; using 1000"
                );
                1000
            }),
    }
}

/// The app's public origin, for OAuth redirect URIs: `PUBLIC_URL` if set,
/// else rebuilt from the request. A forged `Host` only yields a redirect URI
/// the provider refuses, since each provider checks it against the ones
/// registered for the client.
pub fn public_url(headers: &HeaderMap) -> String {
    if let Some(url) = env("PUBLIC_URL") {
        return url.trim_end_matches('/').to_string();
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let host = header("x-forwarded-host")
        .or_else(|| header("host"))
        .unwrap_or_else(|| "localhost:8080".to_string());
    let local = host.starts_with("localhost") || host.starts_with("127.0.0.1");
    let proto = header("x-forwarded-proto")
        .unwrap_or_else(|| if local { "http" } else { "https" }.to_string());
    format!("{proto}://{host}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::fullstack::HeaderValue;

    #[test]
    fn public_url_prefers_forwarded_headers() {
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("internal:8080"));
        h.insert(
            "x-forwarded-host",
            HeaderValue::from_static("app.example.com"),
        );
        h.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        assert_eq!(public_url(&h), "https://app.example.com");
    }

    #[test]
    fn public_url_defaults_to_http_on_localhost_only() {
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("localhost:8080"));
        assert_eq!(public_url(&h), "http://localhost:8080");
        h.insert("host", HeaderValue::from_static("app.example.com"));
        assert_eq!(public_url(&h), "https://app.example.com");
    }
}
