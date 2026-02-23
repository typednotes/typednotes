use dioxus::prelude::*;

use crate::activity_log::{LogLevel, use_activity_log};

const ACTIVITY_LOG_CSS: Asset = asset!("/assets/styling/activity_log.css");

#[component]
pub fn ActivityLogPanel() -> Element {
    let mut log = use_activity_log();

    if !log().visible {
        return rsx! {};
    }

    let entries = log().entries.clone();

    rsx! {
        document::Stylesheet { href: ACTIVITY_LOG_CSS }

        div {
            class: "activity-log-panel",
            div {
                class: "activity-log-header",
                span { "Activity Log" }
                div {
                    class: "activity-log-header-actions",
                    button {
                        onclick: move |_| log.write().entries.clear(),
                        "Clear"
                    }
                    button {
                        onclick: move |_| log.write().visible = false,
                        "Close"
                    }
                }
            }
            div {
                class: "activity-log-entries",
                for entry in entries.iter().rev() {
                    div {
                        class: match entry.level {
                            LogLevel::Error => "activity-log-entry error",
                            LogLevel::Warning => "activity-log-entry warning",
                            LogLevel::Success => "activity-log-entry success",
                            LogLevel::Info => "activity-log-entry info",
                        },
                        span { class: "activity-log-time", "{entry.timestamp}" }
                        span { " {entry.message}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ActivityLogToggle() -> Element {
    let mut log = use_activity_log();
    let count = log().entries.len();
    let has_errors = log().entries.iter().any(|e| e.level == LogLevel::Error);

    rsx! {
        button {
            class: if has_errors { "activity-log-toggle has-errors" } else { "activity-log-toggle" },
            onclick: move |_| {
                let visible = log().visible;
                log.write().visible = !visible;
            },
            title: "Activity log",
            if count > 0 {
                "{count}"
            } else {
                "Log"
            }
        }
    }
}
