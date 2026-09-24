use dioxus::prelude::*;
use ui::OrgPage;

/// `/orgs/:slug`, with the OAuth callback's `?connected=` / `?error=`.
#[component]
pub fn OrgView(slug: String, connected: String, error: String) -> Element {
    rsx! { OrgPage { slug, connected, error } }
}
