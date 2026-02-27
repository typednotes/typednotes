use dioxus::prelude::*;
use crate::Route;

#[component]
pub fn SidebarLayout() -> Element {
    let nav = use_navigator();
    let route = use_route::<Route>();
    let active_path = match route {
        Route::NoteDetail { ref note_path } => Some(note_path.replace('~', "/")),
        _ => None,
    };

    let navigate_settings = move |_: ()| {
        nav.push(Route::Settings {});
    };

    let navigate_note = move |path: String| {
        let encoded = path.replace('/', "~");
        nav.push(Route::NoteDetail { note_path: encoded });
    };

    let navigate_login = move |_: ()| {
        nav.push(Route::Login {});
    };

    rsx! {
        ui::views::SidebarLayoutView {
            active_path: active_path,
            on_navigate_note: navigate_note,
            on_navigate_settings: navigate_settings,
            on_navigate_login: navigate_login,
            Outlet::<Route> {}
        }
    }
}
