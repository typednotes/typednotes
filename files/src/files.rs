use dioxus::prelude::*;

#[component]
pub fn Files(repo_url: String) -> Element {

    
    // let (repo, _outcome) = use_hook gix::clone::PrepareFetch::new(repo_url)?
    // .configure_remote(|remote| {
    //     // Optionally configure the remote if needed
    //     Ok(remote)
    // })
    // .build()
    // .await?;

    rsx! {
        h1 {"Simple git repo"}
        // p {}
    }
}