use api::{create_notebook_share, feed_shared_notebook, get_shared_notebook, list_notebook_shares, revoke_notebook_share, SharedCell};
use dioxus::prelude::*;
use serde_json::Value;
use crate::components::{button::{Button, ButtonVariant}, input::Input, label::Label, select::{use_selected, Select, SelectOption}, textarea::Textarea};
use crate::{error_message, render::Output, orgs::ORGS_CSS, connections::CONNECTIONS_CSS};

#[component]
pub(crate) fn ShareControls(slug: String, project: String, graph: String, ready: bool) -> Element {
    let (o,p,g) = (slug.clone(), project.clone(), graph.clone());
    let mut shares = use_resource(move || list_notebook_shares(o.clone(), p.clone(), g.clone()));
    let mut url = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let (o,p,g) = (slug.clone(), project.clone(), graph.clone());
    rsx! {
        details { class: "nb-advanced share-controls",
            summary { "Share read-only notebook" }
            p { class: "conn-meta", "Anyone with the link can view this snapshot's descriptions, input values and results. Only UI inputs are editable, in isolated visitor sessions. External effects, credentials, secret cells, code and configuration are unavailable to viewers. Results may contain data from private sources: review them before publishing." }
            Button { disabled: busy() || !ready, onclick: move |_| { let (o,p,g) = (o.clone(),p.clone(),g.clone()); async move {
                busy.set(true); match create_notebook_share(o,p,g).await { Ok(created) => { url.set(Some(created.url)); error.set(None); shares.restart(); }, Err(e) => error.set(Some(error_message(&e))) } busy.set(false);
            } }, "Create read-only link" }
            if let Some(url) = url() {
                Label { html_for: "public-notebook-url", "Copy this public link now" }
                Input { id: "public-notebook-url", readonly: true, value: url }
            }
            if let Some(Ok(items)) = shares() { for share in items {
                div { class: "conn-actions", key: "{share.id}",
                    span { class: "conn-meta", "Created {share.created_at}" }
                    Button { variant: ButtonVariant::Outline, disabled: busy(), onclick: {
                        let (o,p,g,id) = (slug.clone(),project.clone(),graph.clone(),share.id.clone()); move |_| { let (o,p,g,id)=(o.clone(),p.clone(),g.clone(),id.clone()); async move {
                            busy.set(true); match revoke_notebook_share(o,p,g,id).await { Ok(())=> { url.set(None); error.set(None); shares.restart(); }, Err(e)=>error.set(Some(error_message(&e))) } busy.set(false);
                        } }
                    }, "Revoke link" }
                }
            } }
            if let Some(error) = error() { p { class: "orgs-error", "{error}" } }
        }
    }
}

#[component]
pub fn SharedNotebookPage(token: ReadSignal<String>) -> Element {
    let mut snapshot = use_server_future(move || get_shared_notebook(token()))?;
    rsx! {
        document::Stylesheet { href: ORGS_CSS }
        document::Stylesheet { href: CONNECTIONS_CSS }
        document::Stylesheet { href: asset!("/assets/styling/notebook.css") }
        document::Meta { name: "robots", content: "noindex,nofollow" }
        div { class: "orgs nb shared-notebook",
            match snapshot() {
                None => rsx! { p { "Loading public notebook…" } },
                Some(Err(error)) => rsx! { p { class: "orgs-error", "Could not open this link: {error_message(&error)}" } },
                Some(Ok(notebook)) => rsx! {
                    header { h2 { "{notebook.name}" } p { class: "conn-meta", "Public read-only snapshot. UI inputs affect only this browser's isolated session. External effects are disabled." } }
                    for cell in notebook.cells.iter() {
                        section { class: "nb-cell", key: "{cell.id}",
                            h3 { "{cell.name}" } p { class: "nb-description", "{cell.description}" }
                            if let Some(input) = &cell.input {
                                SharedInput { token: token(), cell: cell.clone(), initial: notebook.inputs.get(input).cloned(),
                                    on_fed: move |result| snapshot.set(Some(Ok(result))) }
                            }
                            if let Some(outcome) = cell.node_id.and_then(|id| notebook.nodes.iter().find(|n| n.id == id)).and_then(|node| node.outcome.as_ref()) {
                                if let Some(value) = &outcome.output { Output { format: cell.renderer.clone().unwrap_or_else(|| "json".into()), value: value.clone() } }
                                if let Some(error) = &outcome.error { p { class: "nb-error", "{error}" } }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn SharedInput(token: String, cell: SharedCell, initial: Option<Value>, on_fed: EventHandler<api::SharedNotebook>) -> Element {
    let is_string = cell.input_type.as_deref().is_some_and(|ty| ty.trim() == "String");
    let is_bool = cell.input_type.as_deref().is_some_and(|ty| ty.trim() == "Bool");
    let numeric = cell.input_type.as_deref().is_some_and(|ty| matches!(ty.trim(), "Nat" | "Int" | "Float" | "Float32"));
    let initial = initial.map(|v| if is_string { v.as_str().unwrap_or("").to_string() } else { v.to_string() }).unwrap_or_default();
    let mut value = use_signal(|| initial);
    let selected = use_selected(value.into());
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let id = format!("shared-input-{}", cell.id);
    let submit = move |event: FormEvent| {
        event.prevent_default(); let (token,id)=(token.clone(),cell.id.clone());
        let parsed = if is_string { Ok(Value::String(value())) } else if is_bool { Ok(Value::Bool(value()=="true")) } else { serde_json::from_str::<Value>(&value()).map_err(|e| e.to_string()) };
        async move { let parsed=match parsed {Ok(v)=>v,Err(e)=>{error.set(Some(e));return;}};
            busy.set(true); match feed_shared_notebook(token,id,parsed).await {Ok(result)=>{error.set(None);on_fed.call(result);},Err(e)=>error.set(Some(error_message(&e)))} busy.set(false);
        }
    };
    rsx! {
        form { class: "conn-subform", onsubmit: submit,
            Label { html_for: id.clone(), "{cell.name} input" }
            if is_string && !cell.choices.is_empty() {
                Select::<String> { id: id.clone(), value: Some(selected), aria_label: "{cell.name} input", disabled: busy(), on_value_change: move |choice: Option<String>| {if let Some(choice)=choice{value.set(choice);}},
                    for (index,choice) in cell.choices.iter().enumerate() { SelectOption::<String> {index,value:choice.clone(),text_value:choice.clone(),"{choice}"} }
                }
            } else if is_bool {
                Input { id: id.clone(), r#type: "checkbox", checked: value()=="true", disabled: busy(), oninput: move |_: FormEvent| value.set(if value()=="true" {"false"} else {"true"}.into()) }
            } else if is_string || numeric {
                Input {id:id.clone(), r#type: if numeric {"number"} else {"text"}, value:value(), disabled:busy(), oninput:move |e: FormEvent| value.set(e.value())}
            } else {
                Textarea {id:id.clone(),value:value(),disabled:busy(),oninput:move |e: FormEvent| value.set(e.value())}
            }
            Button {r#type:"submit",disabled:busy(),"Update input"}
            if let Some(error)=error(){p{class:"orgs-error","{error}"}}
        }
    }
}
