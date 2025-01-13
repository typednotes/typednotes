use dioxus::prelude::*;
use ui::{Login, Echo, Hero};

#[component]
pub fn Home() -> Element {
    rsx! {
        Login {}
        Hero {}
        Echo {}
    }
}
