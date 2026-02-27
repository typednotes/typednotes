//! Online/offline status indicator for the sidebar header.

use dioxus::prelude::*;

use crate::auth::use_auth;
use crate::Icon;
use crate::icons::{FaCloud, FaCloudArrowUp, FaUserSlash};

/// A small icon that shows the current connectivity and auth status.
///
/// - **Logged in + online**: green cloud icon ("Syncing")
/// - **Logged in + offline**: orange cloud-up icon ("Offline")
/// - **Anonymous**: gray slashed-user icon ("Anonymous — sign in to sync")
#[component]
pub fn OnlineIndicator() -> Element {
    let auth = use_auth();
    let state = auth();

    if state.loading {
        return rsx! {};
    }

    match (&state.user, state.online) {
        (Some(_), true) => rsx! {
            span {
                class: "online-indicator online-indicator--syncing",
                title: "Syncing",
                Icon { icon: FaCloud, width: 14, height: 14 }
            }
        },
        (Some(_), false) => rsx! {
            span {
                class: "online-indicator online-indicator--offline",
                title: "Offline",
                Icon { icon: FaCloudArrowUp, width: 14, height: 14 }
            }
        },
        (None, _) => rsx! {
            span {
                class: "online-indicator online-indicator--anonymous",
                title: "Anonymous — sign in to sync",
                Icon { icon: FaUserSlash, width: 14, height: 14 }
            }
        },
    }
}
