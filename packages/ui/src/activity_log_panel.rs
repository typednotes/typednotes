use dioxus::prelude::*;

use crate::activity_log::{LogLevel, use_activity_log};
use crate::components::{Badge, BadgeVariant};

const LOG_PANEL_CSS: Asset = asset!("/src/views/log_panel.css");

#[component]
pub fn ActivityLogPanel() -> Element {
    let mut log = use_activity_log();

    if !log().visible {
        return rsx! {};
    }

    let entries = log().entries.clone();

    rsx! {
        document::Link { rel: "stylesheet", href: LOG_PANEL_CSS }
        div {
            class: "log-panel",
            height: "200px",
            flex_shrink: "0",
            div {
                class: "log-panel-header",
                span { "Activity Log" }
                div {
                    class: "log-panel-actions",
                    button {
                        class: "log-panel-action",
                        onclick: move |_| log.write().entries.clear(),
                        "Clear"
                    }
                    button {
                        class: "log-panel-action",
                        onclick: move |_| log.write().visible = false,
                        "Close"
                    }
                }
            }
            div {
                class: "log-panel-body",
                for entry in entries.iter().rev() {
                    div {
                        class: match entry.level {
                            LogLevel::Error => "log-panel-entry log-entry-error",
                            LogLevel::Warning => "log-panel-entry log-entry-warning",
                            LogLevel::Success => "log-panel-entry log-entry-success",
                            LogLevel::Info => "log-panel-entry log-entry-info",
                        },
                        span { class: "log-panel-timestamp", "{entry.timestamp}" }
                        span { "{entry.message}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ActivityLogToggle() -> Element {
    let log = use_activity_log();
    let count = log().entries.len();

    rsx! {
        if count > 0 {
            Badge {
                variant: BadgeVariant::Primary,
                "{count}"
            }
        }
    }
}
