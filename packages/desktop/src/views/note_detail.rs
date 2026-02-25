use dioxus::prelude::*;
use crate::Route;

#[component]
pub fn NoteDetail(note_path: String) -> Element {
    let nav = use_navigator();
    let decoded = note_path.replace('~', "/");

    let navigate_notes = move |_: ()| {
        nav.push(Route::Notes {});
    };

    let navigate_note = move |path: String| {
        let encoded = path.replace('/', "~");
        nav.replace(Route::NoteDetail { note_path: encoded });
    };

    rsx! {
        ui::views::NoteDetailView {
            note_path: decoded,
            on_navigate_notes: navigate_notes,
            on_navigate_note: navigate_note,
        }
    }
}
