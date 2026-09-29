//! The types client and server exchange. Ids and timestamps travel as text:
//! the client only displays them, and this keeps `uuid`/`chrono` out of the
//! wasm build.

use serde::{Deserialize, Serialize};

/// A signed-in user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
}

/// An org, as a member sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Org {
    pub id: String,
    pub slug: String,
    pub name: String,
    /// The caller's role: `owner`, `admin` or `member`.
    pub role: String,
    pub created_at: String,
}

/// An org's page: the org and its spendable credits (`None` when `ledger`'s
/// tables are not there).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrgDetail {
    pub org: Org,
    pub credits: Option<i64>,
}

/// The signed-in user's account page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub user: User,
    /// How they sign in.
    pub identities: Vec<Identity>,
    /// Signed-in browsers, this one included.
    pub sessions: i64,
    pub orgs: Vec<Org>,
}

/// A way of signing in: a GitHub or Google account linked to the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    /// `GitHub`, `Google`.
    pub provider: String,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

/// A member of an org, as the org's page lists them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub email: String,
    pub display_name: Option<String>,
    /// `owner`, `admin` or `member`.
    pub role: String,
    pub added_at: String,
    /// Whether they have signed in yet (an invited address has not).
    pub signed_in: bool,
    pub is_you: bool,
    /// Whether the caller may remove them.
    pub can_remove: bool,
}

/// The roles a member can be given, most powerful first.
pub const ROLES: [&str; 3] = ["owner", "admin", "member"];

/// Whether a caller of role `actor` may add someone as `role`, or remove a
/// member of role `role`: owners manage everyone, admins manage admins and
/// members, members manage nobody (but may leave).
pub fn may_manage(actor: &str, role: &str) -> bool {
    match actor {
        "owner" => ROLES.contains(&role),
        "admin" => role == "admin" || role == "member",
        _ => false,
    }
}

/// An org's settings, as its settings page shows them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrgSettings {
    /// Rewrites of a notebook's code Typednotes may start by itself — after
    /// a failed build or a failing cell — before a member asks for one
    /// again. `None`: the deployment's default.
    pub auto_repairs: Option<i32>,
    /// The deployment's default.
    pub default_auto_repairs: i32,
    /// Whether the caller may change them (owners and admins).
    pub can_edit: bool,
}

/// The largest cap an org may set on automatic rewrites.
pub const MAX_AUTO_REPAIRS: i32 = 10;

/// Whether a slug can be used, checked while the user types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlugCheck {
    pub available: bool,
    /// Why not, or a confirmation.
    pub message: String,
}

/// What this deployment can do, for the status line and for probing a fresh
/// deploy: each flag is a dependency that is reachable or configured.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub database: bool,
    /// The app's own history is applied (through `channels`).
    pub schema: bool,
    /// `ledger`'s tables exist.
    pub ledger: bool,
    /// The vault is configured *and* accepts the app's login.
    pub vault: bool,
    pub liaison: bool,
    pub github: bool,
    pub google: bool,
    #[serde(default)]
    pub microsoft: bool,
    pub gitlab: bool,
    pub dropbox: bool,
    pub slack: bool,
    /// Slack's signing secret, without which inbound events are refused.
    pub slack_events: bool,
    /// WhatsApp's app secret and verify token, for the inbound webhook.
    pub whatsapp_webhook: bool,
    /// The computations history (`0004`) is applied.
    pub computations: bool,
    /// `LODE_URL` and `LODE_TOKEN`: notebooks can be implemented.
    pub lode: bool,
    /// `LUN_URL` and `LUN_TOKEN`: notebooks can be built and run.
    pub lun: bool,
    /// `COMPUTE_DB_URL`: `db` sinks can get their per-user schema.
    pub compute: bool,
}

/// A kind of third-party account (docs/connections.md §3.1). The ids are
/// shared with the vault path, liaison's warrants and the `connections`
/// check constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Github,
    Gitlab,
    Gdrive,
    GoogleCalendar,
    MicrosoftCalendar,
    Caldav,
    Gmail,
    Outlook,
    Jmap,
    Notion,
    Dropbox,
    S3,
    Azure,
    Mistral,
    Openai,
    Anthropic,
    OpenaiCompatible,
    Slack,
    Whatsapp,
    Signal,
}

impl Provider {
    pub const ALL: [Provider; 20] = [
        Provider::Github,
        Provider::Gitlab,
        Provider::Gdrive,
        Provider::GoogleCalendar,
        Provider::MicrosoftCalendar,
        Provider::Caldav,
        Provider::Gmail,
        Provider::Outlook,
        Provider::Jmap,
        Provider::Notion,
        Provider::Dropbox,
        Provider::S3,
        Provider::Azure,
        Provider::Mistral,
        Provider::Openai,
        Provider::Anthropic,
        Provider::OpenaiCompatible,
        Provider::Slack,
        Provider::Whatsapp,
        Provider::Signal,
    ];

    /// Code hosts: a project's primary repository is read through one.
    pub const CODE: [Provider; 2] = [Provider::Github, Provider::Gitlab];

    /// File storage, in the order the storage menu shows it.
    pub const STORAGE: [Provider; 4] = [
        Provider::S3,
        Provider::Azure,
        Provider::Dropbox,
        Provider::Gdrive,
    ];

    /// The AI providers, connected with an API token, alphabetically by name.
    pub const AI: [Provider; 4] = [
        Provider::Anthropic,
        Provider::Mistral,
        Provider::Openai,
        Provider::OpenaiCompatible,
    ];

    /// Messaging: a project's interfaces.
    pub const CHANNELS: [Provider; 3] = [Provider::Signal, Provider::Slack, Provider::Whatsapp];

    pub const CALENDARS: [Provider; 3] = [
        Provider::GoogleCalendar,
        Provider::MicrosoftCalendar,
        Provider::Caldav,
    ];

    pub const MAIL: [Provider; 3] = [Provider::Gmail, Provider::Outlook, Provider::Jmap];

    pub const WORKSPACES: [Provider; 1] = [Provider::Notion];

    pub fn id(self) -> &'static str {
        match self {
            Provider::Github => "github",
            Provider::Gitlab => "gitlab",
            Provider::Gdrive => "gdrive",
            Provider::GoogleCalendar => "google-calendar",
            Provider::MicrosoftCalendar => "microsoft-calendar",
            Provider::Caldav => "caldav",
            Provider::Gmail => "gmail",
            Provider::Outlook => "outlook",
            Provider::Jmap => "jmap",
            Provider::Notion => "notion",
            Provider::Dropbox => "dropbox",
            Provider::S3 => "s3",
            Provider::Azure => "azure",
            Provider::Mistral => "mistral",
            Provider::Openai => "openai",
            Provider::Anthropic => "anthropic",
            Provider::OpenaiCompatible => "openai-compatible",
            Provider::Slack => "slack",
            Provider::Whatsapp => "whatsapp",
            Provider::Signal => "signal",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Github => "GitHub",
            Provider::Gitlab => "GitLab",
            Provider::Gdrive => "Google Drive",
            Provider::GoogleCalendar => "Google Calendar",
            Provider::MicrosoftCalendar => "Microsoft Calendar",
            Provider::Caldav => "CalDAV",
            Provider::Gmail => "Gmail",
            Provider::Outlook => "Outlook Mail",
            Provider::Jmap => "JMAP / Fastmail",
            Provider::Notion => "Notion",
            Provider::Dropbox => "Dropbox",
            Provider::S3 => "S3 bucket",
            Provider::Azure => "Azure Blob Storage",
            Provider::Mistral => "Mistral",
            Provider::Openai => "OpenAI",
            Provider::Anthropic => "Anthropic",
            Provider::OpenaiCompatible => "OpenAI-compatible",
            Provider::Slack => "Slack",
            Provider::Whatsapp => "WhatsApp",
            Provider::Signal => "Signal",
        }
    }

    /// The API root liaison confines calls to, when it does not depend on
    /// the account (S3, Azure, Signal bridges and OpenAI-compatible endpoints
    /// are entered by the user).
    pub fn fixed_base_url(self) -> Option<&'static str> {
        match self {
            Provider::Github => Some("https://api.github.com"),
            Provider::Gitlab => Some("https://gitlab.com/api/v4"),
            Provider::Gdrive | Provider::GoogleCalendar | Provider::Gmail => {
                Some("https://www.googleapis.com")
            }
            Provider::MicrosoftCalendar | Provider::Outlook => {
                Some("https://graph.microsoft.com/v1.0")
            }
            Provider::Notion => Some("https://api.notion.com/v1"),
            Provider::Dropbox => Some("https://api.dropboxapi.com"),
            Provider::Mistral => Some("https://api.mistral.ai/v1"),
            Provider::Openai => Some("https://api.openai.com/v1"),
            Provider::Anthropic => Some("https://api.anthropic.com/v1"),
            Provider::Slack => Some("https://slack.com/api"),
            Provider::Whatsapp => Some("https://graph.facebook.com/v21.0"),
            Provider::S3
            | Provider::Azure
            | Provider::OpenaiCompatible
            | Provider::Signal
            | Provider::Caldav
            | Provider::Jmap => None,
        }
    }

    /// Connected through an OAuth round trip rather than a form.
    pub fn is_oauth(self) -> bool {
        matches!(
            self,
            Provider::Github
                | Provider::Gitlab
                | Provider::Gdrive
                | Provider::GoogleCalendar
                | Provider::MicrosoftCalendar
                | Provider::Gmail
                | Provider::Outlook
                | Provider::Dropbox
                | Provider::Slack
        )
    }

    pub fn is_ai(self) -> bool {
        Self::AI.contains(&self)
    }

    pub fn is_code(self) -> bool {
        Self::CODE.contains(&self)
    }

    pub fn is_channel(self) -> bool {
        Self::CHANNELS.contains(&self)
    }
}

/// A connected account. Never carries its credential.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub provider: Provider,
    pub label: String,
    pub base_url: String,
    /// The provider-side identity, where the app keeps one: a Slack team id,
    /// a WhatsApp phone number id, a Signal number; for JMAP, the session URL.
    pub external_id: Option<String>,
    /// `pending`, `active` or `failed`.
    pub status: String,
    pub owner_email: String,
    pub created_at: String,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
    /// Whether the caller may remove it (its creator, or an org admin).
    pub can_remove: bool,
}

/// The outcome of a connection test through liaison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
}

/// A project: work inside an org, with an optional primary repository.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub created_at: String,
    pub repo: Option<RepoRef>,
}

/// A project's primary repository, as recorded when it was chosen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepoRef {
    pub provider: Provider,
    pub full_name: String,
    pub web_url: String,
    pub default_branch: Option<String>,
    /// The connection it is read through; `None` once that was removed.
    pub connection_id: Option<String>,
}

/// A project's page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub org: Org,
    pub project: Project,
}

/// A repository a code connection can see, to pick a primary one from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Repo {
    pub full_name: String,
    pub web_url: String,
    pub default_branch: Option<String>,
    pub private: bool,
}

/// A project's messaging interface: one inbound address.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub provider: Provider,
    pub label: String,
    pub connection_id: String,
    pub created_at: String,
}

/// A Slack conversation a bot can post to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlackChannel {
    pub id: String,
    pub name: String,
    pub private: bool,
}

/// One message of a project's inbox, either way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub provider: Provider,
    pub channel_label: String,
    /// `in` or `out`.
    pub direction: String,
    /// The sender (in) or recipient (out).
    pub peer: String,
    pub peer_name: Option<String>,
    pub body: String,
    pub created_at: String,
}
