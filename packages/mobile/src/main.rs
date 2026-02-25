use dioxus::prelude::*;
use views::{Notes, NoteDetail, Settings, SidebarLayout};

mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Root {},
    #[layout(SidebarLayout)]
        #[route("/notes")]
        Notes {},
        #[route("/notes/:note_path")]
        NoteDetail { note_path: String },
        #[route("/settings")]
        Settings {},
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(|| Signal::new(ui::ActivityLog::default()));
    // Provide a dummy auth state so ui::use_auth() works without AuthProvider
    use_context_provider(|| Signal::new(ui::AuthState { user: None, loading: false }));

    // Theme context: None = system, Some("dark"), Some("light")
    let mut theme: ui::ThemeSignal = use_context_provider(|| Signal::new(Option::<String>::None));
    use_effect(move || {
        ui::load_theme_from_storage(&mut theme);
    });

    rsx! {
        document::Link { rel: "stylesheet", href: ui::TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: ui::DX_COMPONENTS_CSS }
        document::Link { rel: "stylesheet", href: "/fontawesome/css/all.min.css" }
        ui::components::ToastProvider {
            Router::<Route> {}
        }
    }
}

#[component]
fn Root() -> Element {
    let nav = use_navigator();
    nav.replace(Route::Notes {});
    rsx! {}
}
