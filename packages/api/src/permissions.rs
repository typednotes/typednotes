//! Rich connector capabilities: operations and structured resource scopes.
use serde::{Deserialize, Serialize};
use crate::Provider;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorScope {
    pub operation: String,
    pub root: Vec<String>,
    pub descendants: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorPermissions {
    pub scopes: Vec<ConnectorScope>,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellConnector {
    pub connection: String,
    /// Omitted means inherit this connection's ceiling; advanced grants narrow it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<ConnectorPermissions>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionPreset { ReadOnly, ReadWrite, Custom }

macro_rules! operations {
    ($($id:literal => $label:literal, $write:literal);* $(;)?) => {
        &[$(ConnectorOperation { id: $id, label: $label, write: $write }),*]
    };
}

/// Bound runtime services are not external provider connections. Their account
/// and connection IDs are derived from authenticated execution bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalService { Postgres, Vault }

impl LocalService {
    pub fn id(self) -> &'static str { match self { Self::Postgres => "postgres", Self::Vault => "vault" } }
    pub fn effect(self) -> &'static str { match self { Self::Postgres => "PostgreSQL", Self::Vault => "SecretStore" } }
    /// Reserved cell connection selections; they never identify a credential row.
    pub fn selection(self) -> &'static str { match self { Self::Postgres => "compute", Self::Vault => "graph" } }
    pub fn from_selection(value: &str) -> Option<Self> {
        match value { "compute" => Some(Self::Postgres), "graph" => Some(Self::Vault), _ => None }
    }
    pub fn from_id(value: &str) -> Option<Self> {
        match value { "postgres" => Some(Self::Postgres), "vault" => Some(Self::Vault), _ => None }
    }
    pub fn operations(self) -> &'static [ConnectorOperation] {
        match self {
            Self::Postgres => operations!["rows.select" => "Read rows", false; "rows.insert" => "Insert rows", true;
                "rows.update" => "Update rows", true; "rows.delete" => "Delete rows", true],
            Self::Vault => operations!["secrets.read" => "Read secret values", false; "secrets.describe" => "Describe secrets", false;
                "secrets.list" => "List secret names", false; "secrets.write" => "Write secrets", true],
        }
    }
}

pub struct ConnectorOperation {
    pub id: &'static str,
    pub label: &'static str,
    pub write: bool,
}

pub fn connector_operations(provider: Provider) -> &'static [ConnectorOperation] {
    if provider == Provider::TypeSafe {
        return operations!["models.list" => "List models", false; "classification.evaluate" => "Evaluate typed questions", false];
    }
    if provider == Provider::OpencodeZen {
        return operations!["models.list" => "List models", false; "inference.generate" => "Use models", false;
            "classification.evaluate" => "Evaluate typed questions", false];
    }
    if provider.is_ai() {
        return operations!["models.list" => "List models", false; "inference.generate" => "Use models", false];
    }
    match provider {
        Provider::S3 | Provider::Azure => operations![
            "objects.list" => "List objects", false; "objects.read" => "Read objects", false;
            "objects.write" => "Write objects", true; "objects.delete" => "Delete objects", true;
        ],
        Provider::Gdrive | Provider::Dropbox => operations![
            "files.list" => "List files", false; "files.read" => "Read files", false;
            "files.create" => "Create files", true; "files.update" => "Update files", true;
            "files.delete" => "Delete files", true; "files.share" => "Share files", true;
        ],
        Provider::GoogleCalendar | Provider::MicrosoftCalendar => operations![
            "calendars.list" => "List calendars", false; "events.read" => "Read events", false;
            "events.create" => "Create events", true; "events.update" => "Update events", true;
            "events.delete" => "Delete events", true; "events.invite" => "Invite attendees", true;
        ],
        Provider::Caldav => operations![
            "calendars.list" => "List calendars", false; "events.read" => "Read events", false;
            "events.create" => "Create events", true; "events.update" => "Update events", true;
            "events.delete" => "Delete events", true;
        ],
        Provider::Gmail | Provider::Outlook | Provider::Jmap => operations![
            "mailboxes.list" => "List mailboxes", false; "messages.read" => "Read messages", false;
            "messages.search" => "Search messages", false; "drafts.create" => "Create drafts", true;
            "messages.send" => "Send messages", true; "messages.update" => "Label or move messages", true;
            "messages.delete" => "Delete messages", true; "attachments.read" => "Read attachments", false;
        ],
        Provider::Notion => operations![
            "pages.read" => "Read pages", false; "databases.query" => "Query databases", false;
            "pages.create" => "Create pages", true; "pages.update" => "Update pages", true;
            "pages.delete" => "Delete pages", true; "comments.create" => "Comment", true;
        ],
        Provider::Slack => operations![
            "channels.list" => "List channels", false; "messages.read" => "Read messages", false;
            "messages.send" => "Send messages", true; "messages.update" => "Edit messages", true;
            "messages.delete" => "Delete messages", true;
        ],
        Provider::Whatsapp | Provider::Signal => operations!["messages.send" => "Send messages", true],
        Provider::Github | Provider::Gitlab => operations![
            "repositories.list" => "List repositories", false; "repositories.read" => "Read code", false;
            "repositories.write" => "Write code", true; "repositories.delete" => "Delete code files", true; "issues.read" => "Read issues", false;
            "issues.write" => "Update issues", true; "pull_requests.write" => "Update pull requests", true;
        ],
        _ => &[],
    }
}

/// Resource levels remain typed by the provider: no user-written URL patterns.
pub fn connector_scope_label(provider: Provider) -> &'static str {
    if provider.is_ai() { return "Model IDs"; }
    match provider {
        Provider::S3 => "Object prefixes inside this bucket",
        Provider::Azure => "Blob prefixes inside this container",
        Provider::Gdrive | Provider::Dropbox => "Folder IDs or paths",
        Provider::GoogleCalendar | Provider::MicrosoftCalendar | Provider::Caldav => "Calendar IDs or paths",
        Provider::Gmail | Provider::Outlook | Provider::Jmap => "Mailbox / folder / label IDs",
        Provider::Notion => "Page or database IDs",
        Provider::Slack | Provider::Whatsapp | Provider::Signal => "Channels or recipients",
        Provider::Github | Provider::Gitlab => "Repository names",
        _ => "Resources",
    }
}

/// Gemini inventory names use a `models/` prefix; its native selector is the
/// actual model ID, not an invented ancestry beneath a `models` resource.
pub fn model_resource(provider: Provider, model: &str) -> Vec<String> {
    let model = if provider == Provider::Gemini { model.strip_prefix("models/").unwrap_or(model) } else { model };
    model.split('/').map(str::to_string).collect()
}

impl ConnectorPermissions {
    pub fn deny_all() -> Self {
        Self { scopes: Vec::new(), max_request_bytes: 1, max_response_bytes: 1 }
    }

    /// Native selectors are component-wise, byte bounded, and never URL encoded.
    pub fn valid_resource(resource: &[String]) -> bool {
        resource.len() <= 32 && resource.iter().all(|segment| !segment.is_empty()
            && segment.len() <= 255 && segment != "." && segment != ".."
            && !segment.chars().any(char::is_control)
            && !segment.contains(['/', '\\', '%']))
    }

    pub fn scoped(operation: &str, root: Vec<String>, descendants: bool) -> Self {
        Self { scopes: vec![ConnectorScope { operation: operation.into(), root, descendants }],
            max_request_bytes: 1_048_576, max_response_bytes: 16_777_216 }
    }
    /// Pairwise intersections keep the most specific compatible resource root.
    /// An exact grant never becomes a descendant grant through intersection.
    pub fn intersect(&self, other: &Self) -> Self {
        let mut scopes = Vec::new();
        for a in &self.scopes {
            for b in &other.scopes {
                if a.operation != b.operation { continue; }
                let grant = if a.root == b.root {
                    Some(ConnectorScope { operation: a.operation.clone(), root: a.root.clone(), descendants: a.descendants && b.descendants })
                } else if a.descendants && b.root.starts_with(&a.root) {
                    Some(b.clone())
                } else if b.descendants && a.root.starts_with(&b.root) {
                    Some(a.clone())
                } else { None };
                if let Some(grant) = grant { if !scopes.contains(&grant) { scopes.push(grant); } }
            }
        }
        Self { scopes, max_request_bytes: self.max_request_bytes.min(other.max_request_bytes), max_response_bytes: self.max_response_bytes.min(other.max_response_bytes) }
    }

    pub fn capability_json(&self, provider: Provider, connection: &str) -> serde_json::Value {
        self.named_capability_json(provider.id(), connection)
    }

    pub fn named_capability_json(&self, provider: &str, connection: &str) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        value["provider"] = serde_json::json!(provider);
        value["connection"] = serde_json::json!(connection);
        value
    }
    /// The bucket/container/base credential already confines the account.
    /// Read-write omits destructive/sharing/invitation/sending operations;
    /// those remain independent advanced grants.
    pub fn preset(provider: Provider, preset: PermissionPreset) -> Self {
        if preset == PermissionPreset::Custom { return Self::deny_all(); }
        let scopes = connector_operations(provider).iter().filter(|op| {
            !op.write || (preset == PermissionPreset::ReadWrite &&
                !["delete", "share", "invite", "send"].iter().any(|word| op.id.ends_with(word)))
        }).map(|op| ConnectorScope { operation: op.id.into(), root: Vec::new(), descendants: true }).collect();
        Self { scopes, max_request_bytes: 1_048_576, max_response_bytes: 16_777_216 }
    }

    pub fn validate(&self, provider: Provider) -> Result<Self, String> {
        self.validate_operations(connector_operations(provider), provider.name(), false)
    }

    pub fn validate_local(&self, service: LocalService) -> Result<Self, String> {
        self.validate_operations(service.operations(), service.id(), service == LocalService::Postgres)
    }

    pub fn local_preset(service: LocalService, preset: PermissionPreset, root: Vec<String>) -> Self {
        if preset == PermissionPreset::Custom { return Self::deny_all(); }
        Self { scopes: service.operations().iter().filter(|op| !op.write || (preset == PermissionPreset::ReadWrite && !op.id.ends_with("delete")))
            .map(|op| ConnectorScope { operation: op.id.into(), root: root.clone(), descendants: true }).collect(),
            max_request_bytes: 1_048_576, max_response_bytes: 16_777_216 }
    }

    fn validate_operations(&self, operations: &[ConnectorOperation], name: &str, postgres: bool) -> Result<Self, String> {
        if self.scopes.len() > 128 { return Err("permissions: at most 128 resource grants".into()); }
        for scope in &self.scopes {
            if !operations.iter().any(|operation| operation.id == scope.operation) {
                return Err(format!("permission: {name} does not support '{}'", scope.operation));
            }
            if !Self::valid_resource(&scope.root) {
                return Err("resource scopes: plain path components, without . or ..".into());
            }
            if postgres && (scope.root.len() > 2 || scope.root.iter().any(|part| part.len() > 63)) {
                return Err("PostgreSQL scopes: schema/table identifiers of at most 63 UTF-8 bytes".into());
            }
        }
        if self.max_request_bytes == 0 || self.max_request_bytes > 67_108_864
            || self.max_response_bytes == 0 || self.max_response_bytes > 67_108_864 {
            return Err("request/response limit: 1 byte to 64 MiB".into());
        }
        Ok(self.clone())
    }

    pub fn permits(&self, operation: &str, resource: &[String]) -> bool {
        self.scopes.iter().any(|scope| scope.operation == operation &&
            if scope.descendants { resource.starts_with(&scope.root) } else { resource == scope.root })
    }

    pub fn narrow(&self, requested: &Self) -> Result<Self, String> {
        if requested.max_request_bytes > self.max_request_bytes || requested.max_response_bytes > self.max_response_bytes
            || requested.scopes.iter().any(|grant| !self.scopes.iter().any(|parent| parent.operation == grant.operation &&
                if parent.descendants { grant.root.starts_with(&parent.root) } else { !grant.descendants && grant.root == parent.root })) {
            return Err("a cell cannot widen its connection permissions".into());
        }
        Ok(requested.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_connector_has_valid_defaults_and_independent_rich_operations() {
        for &provider in Provider::ALL {
            assert!(!connector_operations(provider).is_empty(), "{provider:?}");
            let permissions = ConnectorPermissions::preset(provider, PermissionPreset::ReadOnly);
            assert!(permissions.validate(provider).is_ok());
            assert!(permissions.scopes.iter().all(|s| !connector_operations(provider).iter().find(|op| op.id == s.operation).unwrap().write));
        }
        let mail = ConnectorPermissions::preset(Provider::Gmail, PermissionPreset::ReadWrite);
        assert!(mail.permits("drafts.create", &[]));
        assert!(!mail.permits("messages.send", &[]));
        assert!(!mail.permits("messages.delete", &[]));
    }
    #[test]
    fn resource_scope_is_component_wise_and_can_only_be_narrowed() {
        let parent = ConnectorPermissions { scopes: vec![ConnectorScope { operation: "objects.read".into(), root: vec!["reports".into()], descendants: true }], ..ConnectorPermissions::preset(Provider::S3, PermissionPreset::ReadOnly) };
        assert!(parent.permits("objects.read", &["reports".into(), "2026".into()]));
        assert!(!parent.permits("objects.read", &["reports-private".into()]));
        let mut child = parent.clone();
        child.scopes[0].root.push("2026".into());
        assert!(parent.narrow(&child).is_ok());
        child.scopes[0].operation = "objects.write".into();
        assert!(parent.narrow(&child).is_err());
    }

    #[test]
    fn catalog_matches_native_support_and_presets_never_restore_removed_operations() {
        for (provider, denied) in [
            (Provider::Caldav, vec!["events.invite"]),
            (Provider::Slack, vec!["files.read"]),
            (Provider::Signal, vec!["channels.list", "messages.read", "messages.update", "messages.delete", "files.read"]),
            (Provider::Whatsapp, vec!["channels.list", "messages.read", "messages.update", "messages.delete", "files.read"]),
        ] {
            for operation in denied {
                assert!(!connector_operations(provider).iter().any(|op| op.id == operation));
                for preset in [PermissionPreset::ReadOnly, PermissionPreset::ReadWrite, PermissionPreset::Custom] {
                    assert!(!ConnectorPermissions::preset(provider, preset).permits(operation, &[]));
                }
                assert!(ConnectorPermissions::scoped(operation, Vec::new(), true).validate(provider).is_err());
            }
        }
        assert!(Provider::Radius.can_generate());
        assert!(connector_operations(Provider::Radius).iter().any(|op| op.id == "inference.generate"));
        assert!(!ConnectorPermissions::preset(Provider::Github, PermissionPreset::ReadWrite).permits("repositories.delete", &[]));
        assert!(connector_operations(Provider::OpencodeZen).iter().any(|op| op.id == "classification.evaluate"));
    }

    #[test]
    fn strict_policy_schema_and_utf8_selectors_match_broker() {
        for raw in [
            r#"{"scopes":[{"operation":"objects.read","root":[]}],"maxRequestBytes":1,"maxResponseBytes":1}"#,
            r#"{"scopes":[],"maxRequestBytes":1,"maxResponseBytes":1,"extra":true}"#,
            r#"{"scopes":[],"maxRequestBytes":1.5,"maxResponseBytes":1}"#,
            r#"{"scopes":[],"scopes":[],"maxRequestBytes":1,"maxResponseBytes":1}"#,
        ] { assert!(serde_json::from_str::<ConnectorPermissions>(raw).is_err()); }
        for segment in ["%2e", "a/b", "a\\b", ".", "..", "a\u{85}", ""] {
            assert!(!ConnectorPermissions::valid_resource(&[segment.into()]));
        }
        assert!(!ConnectorPermissions::valid_resource(&["é".repeat(128)]));
        assert!(ConnectorPermissions::valid_resource(&["é".repeat(127)]));
        assert!(ConnectorPermissions::valid_resource(&["query?&= Unicode ✓".into()]));
        let recursive = ConnectorPermissions::scoped("objects.read", vec!["reports".into()], true);
        let exact = ConnectorPermissions::scoped("objects.read", vec!["reports".into()], false);
        assert!(!recursive.intersect(&exact).permits("objects.read", &["reports".into(), "file".into()]));
        assert!(exact.narrow(&recursive).is_err());
    }
}
