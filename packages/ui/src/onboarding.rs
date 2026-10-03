use api::{get_workspace, list_graphs, list_orgs, list_projects, set_workspace_defaults};
use dioxus::prelude::*;
use crate::components::{button::{Button,ButtonSize,ButtonVariant}, card::{Card,CardContent,CardHeader,CardTitle,CardDescription}, select::{Select,SelectOption}};
use crate::{auth::LoginPanel,error_message};

pub const SETUP_CSS: Asset = asset!("/assets/styling/setup.css");
pub static WORKSPACE_REVISION: GlobalSignal<u64> = Signal::global(|| 0);
pub(crate) fn refresh_workspace() { *WORKSPACE_REVISION.write() += 1; }

/// Setup is persisted user state. The landing opens the relevant standard page.
#[component]
pub fn WorkspaceLanding() -> Element {
    let workspace = use_server_future(get_workspace)?;
    let nav = navigator();
    use_effect(move || { if let Some(Ok(w))=workspace() { nav.replace(w.default_url.unwrap_or(w.setup_url)); } });
    rsx! { match workspace() {
        Some(Err(ServerFnError::ServerError{code:401,..})) => rsx!{LoginPanel{error:None}},
        Some(Err(e)) => rsx!{p{class:"orgs-error","Could not open your workspace: {error_message(&e)}"}},
        _ => rsx!{p{role:"status","Opening your workspace…"}},
    } }
}

/// Legacy bookmarks redirect into standard UI; there is no setup-only screen.
#[component]
pub fn OnboardingPage() -> Element {
    rsx! { WorkspaceLanding {} }
}

#[component]
pub fn WorkspaceGuide(current_path: ReadSignal<String>) -> Element {
    let mut workspace=use_resource(move || { let _=current_path(); let _=*WORKSPACE_REVISION.read(); get_workspace() });
    rsx! {
        document::Stylesheet{href:SETUP_CSS}
        if let Some(Ok(w))=workspace() { if w.setup_required {
            aside{class:"setup-guide",aria_label:"Workspace setup guidance",
                div{class:"setup-guide-copy",
                    strong{"Set up your workspace"}
                    if let Some(step)=w.next_step{p{"{step}"}}
                }
                div{class:"conn-actions",
                    Link{to:w.setup_url,"Continue setup"}
                    Button{size:ButtonSize::Sm,variant:ButtonVariant::Ghost,onclick:move |_|workspace.restart(),"Refresh setup status"}
                }
            }
        } }
    }
}

/// Default selection lives in the existing account settings, like profile and
/// session preferences. Selecting a target never widens its execution grants.
#[component]
pub(crate) fn DefaultWorkspace() -> Element {
    let mut workspace=use_server_future(get_workspace)?;
    let orgs=use_resource(list_orgs);
    let projects=use_resource(move || async move {match workspace().and_then(Result::ok).and_then(|w|w.org){Some(o)=>list_projects(o.slug).await,None=>Ok(Vec::new())}});
    let graphs=use_resource(move || async move {match workspace().and_then(Result::ok){Some(w)=>match(w.org,w.project){(Some(o),Some(p))=>list_graphs(o.slug,p.slug).await,_=>Ok(Vec::new())},None=>Ok(Vec::new())}});
    let selected_org=use_memo(move ||workspace().and_then(Result::ok).and_then(|w|w.org.map(|o|o.slug)));
    let selected_project=use_memo(move ||workspace().and_then(Result::ok).and_then(|w|w.project.map(|p|p.slug)));
    let selected_graph=use_memo(move ||workspace().and_then(Result::ok).and_then(|w|w.notebook.map(|g|g.slug)));
    let mut error=use_signal(||None::<String>);let mut busy=use_signal(||false);
    let save=use_callback(move |(o,p,g):(String,Option<String>,Option<String>)|{spawn(async move{
        busy.set(true);match set_workspace_defaults(o,p,g,false).await{Ok(_)=>{error.set(None);workspace.restart();refresh_workspace();},Err(e)=>error.set(Some(error_message(&e)))}busy.set(false);
    });});
    rsx!{Card{CardHeader{CardTitle{"Default workspace"}CardDescription{"Choose the organization, project and notebook to open after sign-in. Setup guidance follows these choices in the standard pages."}}
        CardContent{div{class:"conn-subform",
            if let Some(Ok(options))=orgs(){Select::<String>{value:Some(selected_org.into()),aria_label:"Default organization",disabled:busy(),on_value_change:move |v:Option<String>|{if let Some(v)=v{save.call((v,None,None));}},
                for(index,o)in options.iter().enumerate(){SelectOption::<String>{index,value:o.slug.clone(),text_value:o.name.clone(),"{o.name}"}}}}
            if let Some(org)=selected_org(){if let Some(Ok(options))=projects(){Select::<String>{value:Some(selected_project.into()),aria_label:"Default project",disabled:busy(),on_value_change:{let org=org.clone();move |v:Option<String>|{if let Some(v)=v{save.call((org.clone(),Some(v),None));}}},
                for(index,p)in options.iter().enumerate(){SelectOption::<String>{index,value:p.slug.clone(),text_value:p.name.clone(),"{p.name}"}}}}}
            if let(Some(org),Some(project))=(selected_org(),selected_project()){if let Some(Ok(options))=graphs(){Select::<String>{value:Some(selected_graph.into()),aria_label:"Default notebook",disabled:busy(),on_value_change:move |v:Option<String>|{if let Some(v)=v{save.call((org.clone(),Some(project.clone()),Some(v)));}},
                for(index,g)in options.iter().enumerate(){SelectOption::<String>{index,value:g.slug.clone(),text_value:g.name.clone(),"{g.name}"}}}}}
            if let Some(Ok(w))=workspace(){Link{to:w.default_url.unwrap_or(w.setup_url),"Open workspace"}}
            if let Some(e)=error(){p{class:"orgs-error","{e}"}}
        }}
    }}
}
