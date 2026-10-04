use api::{get_workspace, list_orgs, list_projects, set_workspace_defaults};
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
        Some(Err(e)) => rsx!{p{class:"orgs-error","Could not open your project: {error_message(&e)}"}},
        _ => rsx!{p{role:"status","Opening your project…"}},
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
            aside{class:"setup-guide",aria_label:"Project setup guidance",
                div{class:"setup-guide-copy",
                    strong{"Connect your project"}
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
    let selected_org=use_memo(move ||workspace().and_then(Result::ok).and_then(|w|w.org.map(|o|o.slug)));
    let selected_project=use_memo(move ||workspace().and_then(Result::ok).and_then(|w|w.project.map(|p|p.slug)));
    let mut error=use_signal(||None::<String>);let mut busy=use_signal(||false);
    let save=use_callback(move |(o,p,g):(String,Option<String>,Option<String>)|{spawn(async move{
        busy.set(true);match set_workspace_defaults(o,p,g,false).await{Ok(_)=>{error.set(None);workspace.restart();refresh_workspace();},Err(e)=>error.set(Some(error_message(&e)))}busy.set(false);
    });});
    rsx!{Card{CardHeader{CardTitle{"Default project"}CardDescription{"Choose the organization and project to open after sign-in."}}
        CardContent{div{class:"conn-subform",
            if let Some(Ok(options))=orgs(){Select::<String>{value:Some(selected_org.into()),aria_label:"Default organization",disabled:busy(),on_value_change:move |v:Option<String>|{if let Some(v)=v{save.call((v,None,None));}},
                for(index,o)in options.iter().enumerate(){SelectOption::<String>{index,value:o.slug.clone(),text_value:o.name.clone(),"{o.name}"}}}}
            if let Some(org)=selected_org(){if let Some(Ok(options))=projects(){Select::<String>{value:Some(selected_project.into()),aria_label:"Default project",disabled:busy(),on_value_change:{let org=org.clone();move |v:Option<String>|{if let Some(v)=v{save.call((org.clone(),Some(v),None));}}},
                for(index,p)in options.iter().enumerate(){SelectOption::<String>{index,value:p.slug.clone(),text_value:p.name.clone(),"{p.name}"}}}}}
            if let Some(Ok(w))=workspace(){Link{to:w.default_url.unwrap_or(w.setup_url),"Open project"}}
            if let Some(e)=error(){p{class:"orgs-error","{e}"}}
        }}
    }}
}

#[component]
pub(crate) fn FirstProjectSetup()->Element{
    let mut setup=use_server_future(get_workspace)?;let mut error=use_signal(||None::<String>);
    rsx!{if let Some(Ok(w))=setup(){if w.setup_required&&w.project.is_none(){
        Card{CardHeader{CardTitle{"Connect a repository and AI account"}CardDescription{"Choose GitHub or GitLab and an AI provider. Your first project uses the repository's name; its organization uses the repository owner."}}
            CardContent{
                if let Some(org)=w.org{Link{to:"/orgs/{org.slug}/settings/connections","Connect accounts and choose repository"}}
                else{crate::components::button::Button{onclick:move |_|async move{match api::begin_repository_setup().await{Ok(org)=>crate::navigate_to(&format!("/orgs/{}/settings/connections",org.slug)),Err(e)=>error.set(Some(error_message(&e)))}},"Connect GitHub/GitLab and AI"}}
                crate::components::button::Button{variant:crate::components::button::ButtonVariant::Ghost,onclick:move |_|async move{match api::skip_repository_setup().await{Ok(_)=>{setup.restart();refresh_workspace();},Err(e)=>error.set(Some(error_message(&e)))}},"Skip and create organization/project manually"}
                if let Some(e)=error(){p{class:"orgs-error","{e}"}}
            }}
    }}}
}

#[component]
pub(crate) fn RepositoryProjectSetup(slug:ReadSignal<String>)->Element{
    let setup=use_server_future(get_workspace)?;
    let connections=use_resource(move ||{let _=*WORKSPACE_REVISION.read();api::list_connections(slug())});
    let mut connection=use_signal(String::new);let mut repository=use_signal(String::new);let mut page=use_signal(||1u32);
    let repos=use_resource(move ||async move{if connection().is_empty(){Ok(api::RepoPage{repos:Vec::new(),next_page:None,truncated:false})}else{api::list_repo_page(slug(),connection(),page()).await}});
    let selected_connection=crate::components::select::use_selected(connection.into());
    let selected_repository=crate::components::select::use_selected(repository.into());
    let mut error=use_signal(||None::<String>);let mut busy=use_signal(||false);
    rsx!{if let Some(Ok(w))=setup(){if w.project.is_none()&&w.org.is_some_and(|o|o.slug==slug()){
        Card{CardHeader{CardTitle{"Choose your first repository"}CardDescription{"Connect an AI account above, then choose a repository. You can keep managing organizations and projects manually."}}
            CardContent{div{class:"conn-subform repo-picker",
                if let Some(Ok(accounts))=connections(){
                    div{class:"conn-grid",div{class:"orgs-field",Select::<String>{value:Some(selected_connection),aria_label:"Code connection",on_value_change:move |v:Option<String>|{connection.set(v.unwrap_or_default());repository.set(String::new());page.set(1);},for(i,c)in accounts.iter().filter(|c|c.provider.is_code()).enumerate(){SelectOption::<String>{index:i,value:c.id.clone(),text_value:c.label.clone(),"{c.provider.name()} {c.label}"}}}}
                    div{class:"orgs-field",if let Some(Ok(repos))=repos(){Select::<String>{value:Some(selected_repository),aria_label:"Repository",on_value_change:move |v:Option<String>|repository.set(v.unwrap_or_default()),for(i,r)in repos.repos.iter().enumerate(){SelectOption::<String>{index:i,value:r.full_name.clone(),text_value:r.full_name.clone(),"{r.full_name}"}}}}}}
                    if let Some(Ok(repos))=repos(){if let Some(next)=repos.next_page{crate::components::button::Button{variant:crate::components::button::ButtonVariant::Outline,onclick:move |_|page.set(next),"Next repositories"}}}
                    crate::components::button::Button{disabled:busy()||repository().is_empty()||!accounts.iter().any(|c|c.status=="active"&&c.provider.can_generate()),onclick:move |_|async move{busy.set(true);match api::create_project_from_repository(slug(),connection(),repository()).await{Ok(d)=>{refresh_workspace();crate::navigate_to(&format!("/orgs/{}/projects/{}/settings/model",d.org.slug,d.project.slug));},Err(e)=>error.set(Some(error_message(&e)))}busy.set(false);},"Create project from repository"}
                }
                if let Some(e)=error(){p{class:"orgs-error","{e}"}}
            }}
        }
    }}}
}
