use dioxus::prelude::*;

#[component]
pub fn Settings() -> Element {
    rsx! {
        ui::views::SettingsView {
            show_git_sync: true,
            show_theme: true,
        }
    }
}
