use dioxus::prelude::*;
use ui::OrgsPanel;

#[component]
pub fn Home() -> Element {
    rsx! {
        OrgsPanel {}
    }
}
