use dioxus::prelude::*;
use views::{Login, Notes, NoteDetail, Register, Settings, SidebarLayout};

mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Root {},
    #[route("/login")]
    Login {},
    #[route("/register")]
    Register {},
    #[layout(SidebarLayout)]
        #[route("/notes")]
        Notes {},
        #[route("/notes/:note_path")]
        NoteDetail { note_path: String },
        #[route("/settings")]
        Settings {},
}

fn main() {
    dioxus::fullstack::set_server_url("https://typednotes.org");
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(|| Signal::new(ui::ActivityLog::default()));

    // Theme context: None = system, Some("dark"), Some("light")
    let mut theme: ui::ThemeSignal = use_context_provider(|| Signal::new(Option::<String>::None));
    use_effect(move || {
        ui::load_theme_from_storage(&mut theme);
    });

    rsx! {
        document::Link { rel: "stylesheet", href: ui::TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: ui::DX_COMPONENTS_CSS }
        ui::AuthProvider {
            ui::components::ToastProvider {
                Router::<Route> {}
            }
        }
    }
}

#[component]
fn Root() -> Element {
    let auth = ui::use_auth();
    let nav = use_navigator();

    // Redirect based on auth state
    if !auth().loading {
        if auth().user.is_some() {
            nav.replace(Route::Notes {});
        } else {
            nav.replace(Route::Login {});
        }
    }

    rsx! {}
}
