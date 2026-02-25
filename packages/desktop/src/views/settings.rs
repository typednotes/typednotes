use dioxus::prelude::*;

#[component]
pub fn Settings() -> Element {
    rsx! {
        ui::views::SettingsView {
            show_theme: true,
        }
    }
}
