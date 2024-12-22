use dioxus::prelude::*;

#[component]
pub fn Files(name: String) -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        h1 {"{name}"}
        h1 { "High-Five counter: {count}" }
        button { onclick: move |_| count += 1, "Up high!" }
        button { onclick: move |_| count -= 1, "Down low!" }
    }
}