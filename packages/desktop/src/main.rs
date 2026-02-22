use dioxus::prelude::*;

mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Placeholder {},
}

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Router::<Route> {}
    }
}

#[component]
fn Placeholder() -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: center; height: 100vh;",
            p { "TypedNotes Desktop — coming soon" }
        }
    }
}
