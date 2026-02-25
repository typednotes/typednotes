use dioxus::prelude::*;

use crate::activity_log::{LogLevel, use_activity_log};
use crate::components::{Button, ButtonVariant};

#[component]
pub fn ActivityLogPanel() -> Element {
    let mut log = use_activity_log();

    if !log().visible {
        return rsx! {};
    }

    let entries = log().entries.clone();

    rsx! {
        div {
            class: "h-[200px] shrink-0 bg-console-bg text-console-text font-mono text-xs flex flex-col border-t border-console-border",
            div {
                class: "flex items-center justify-between px-3 py-1.5 bg-console-header border-b border-console-border text-[0.6875rem] font-semibold uppercase tracking-wider text-[#cccccc]",
                span { "Activity Log" }
                div {
                    class: "flex gap-2",
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: "text-[#888] text-[0.6875rem] px-1.5 py-0.5 hover:bg-console-border hover:text-console-text",
                        onclick: move |_| log.write().entries.clear(),
                        "Clear"
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: "text-[#888] text-[0.6875rem] px-1.5 py-0.5 hover:bg-console-border hover:text-console-text",
                        onclick: move |_| log.write().visible = false,
                        "Close"
                    }
                }
            }
            div {
                class: "flex-1 overflow-y-auto px-3 py-1.5",
                for entry in entries.iter().rev() {
                    div {
                        class: match entry.level {
                            LogLevel::Error => "py-0.5 leading-relaxed text-[#f48771]",
                            LogLevel::Warning => "py-0.5 leading-relaxed text-warning",
                            LogLevel::Success => "py-0.5 leading-relaxed text-[#89d185]",
                            LogLevel::Info => "py-0.5 leading-relaxed text-[#9e9e9e]",
                        },
                        span { class: "text-[#666] mr-2", "{entry.timestamp}" }
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
        Button {
            variant: ButtonVariant::Ghost,
            class: if has_errors {
                "text-danger text-[0.6875rem] px-2 py-1 hover:bg-neutral-200 hover:text-neutral-800"
            } else {
                "text-neutral-600 text-[0.6875rem] px-2 py-1 hover:bg-neutral-200 hover:text-neutral-800"
            },
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
