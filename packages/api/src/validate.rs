//! Validation, identical on both sides: the forms explain before a round
//! trip, and the server still enforces.

use crate::Provider;

/// Why a slug (an org's or a project's) was refused.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    let ok = (3..=40).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err("slug: 3–40 characters, lowercase letters, digits and inner dashes".to_string())
    }
}

/// Why a display name was refused.
pub fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        Err("name: 1–100 characters".to_string())
    } else {
        Ok(())
    }
}

/// Why an org (or project) slug or name was refused.
pub fn validate_org(slug: &str, name: &str) -> Result<(), String> {
    validate_slug(slug)?;
    validate_name(name)
}

/// A slug suggested from a name: ASCII letters and digits, lowercased, runs
/// of anything else collapsed to one dash, at most 40 characters. May still
/// be too short to be valid.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => {
                'a'
            }
            'ç' | 'Ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
            'ñ' | 'Ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut out: String = out.chars().take(40).collect();
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// An API base URL: `https://host[:port][/path]`, no query, fragment,
/// userinfo or dot segments, returned without a trailing `/`. Plain `http`
/// only for `localhost`/`127.0.0.1` (local S3, model servers or bridges).
/// liaison re-checks all of this before any call.
pub fn validate_base_url(url: &str, allow_path: bool) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let rest = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split(['/', ':']).next().unwrap_or("");
        if host != "localhost" && host != "127.0.0.1" {
            return Err("the URL must start with https://".to_string());
        }
        rest
    } else {
        return Err("the URL must start with https://".to_string());
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let bad = |c: char| c.is_whitespace() || c.is_control() || "?#@\\\"<>{}|^`".contains(c);
    if authority.is_empty() || url.chars().any(bad) {
        return Err(
            "the URL must be a plain https://host[/path], without query or credentials".to_string(),
        );
    }
    if path
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        && !path.is_empty()
    {
        return Err("the URL path is malformed".to_string());
    }
    if !allow_path && !path.is_empty() {
        return Err(
            "the endpoint must not have a path, e.g. https://s3.us-east-1.amazonaws.com"
                .to_string(),
        );
    }
    Ok(url.to_string())
}

fn is_token(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.chars().all(|c| c.is_ascii_graphic())
}

/// An API token or key: printable ASCII, no spaces.
pub fn validate_api_key(key: &str) -> Result<String, String> {
    let key = key.trim();
    if is_token(key, 8, 512) {
        Ok(key.to_string())
    } else {
        Err("the API key must be 8–512 printable characters without spaces".to_string())
    }
}

/// A normalized S3 connection form.
#[derive(Clone, Debug, PartialEq)]
pub struct S3Form {
    /// `{endpoint}/{bucket}`: path-style, so liaison confines calls to the bucket.
    pub base_url: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

pub fn validate_s3(
    endpoint: &str,
    region: &str,
    bucket: &str,
    access_key_id: &str,
    secret_access_key: &str,
) -> Result<S3Form, String> {
    let endpoint = validate_base_url(endpoint, false)?;
    let region = region.trim().to_string();
    if !(1..=32).contains(&region.len())
        || !region
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("region: lowercase letters, digits and dashes, e.g. us-east-1".to_string());
    }
    let bucket = bucket.trim().to_string();
    let alnum = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if !(3..=63).contains(&bucket.len())
        || !bucket
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        || !alnum(bucket.chars().next())
        || !alnum(bucket.chars().last())
    {
        return Err("bucket: 3–63 lowercase letters, digits, dots and dashes".to_string());
    }
    let (access_key_id, secret_access_key) = (access_key_id.trim(), secret_access_key.trim());
    if !is_token(access_key_id, 1, 256) || !is_token(secret_access_key, 1, 256) {
        return Err("access key id and secret: printable characters without spaces".to_string());
    }
    Ok(S3Form {
        base_url: format!("{endpoint}/{bucket}"),
        region,
        bucket,
        access_key_id: access_key_id.to_string(),
        secret_access_key: secret_access_key.to_string(),
    })
}

/// The AWS S3 endpoint of a region (path-style requests).
pub fn aws_s3_endpoint(region: &str) -> String {
    format!("https://s3.{}.amazonaws.com", region.trim())
}

/// A normalized Azure Blob Storage connection form.
#[derive(Clone, Debug, PartialEq)]
pub struct AzureForm {
    /// `https://{account}.blob.core.windows.net/{container}`.
    pub base_url: String,
    pub account: String,
    pub container: String,
    /// The SAS query string, without its leading `?`.
    pub sas: String,
}

/// The SAS query parameters liaison appends to every Azure call; a caller's
/// URL may not carry any of them. Lowercase.
pub const SAS_PARAMS: [&str; 16] = [
    "sv", "ss", "srt", "sp", "se", "st", "spr", "sig", "si", "sr", "sdd", "skoid", "sktid", "skt",
    "ske", "sks",
];

pub fn validate_azure(account: &str, container: &str, sas: &str) -> Result<AzureForm, String> {
    let account = account.trim().to_string();
    if !(3..=24).contains(&account.len())
        || !account
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err("storage account: 3–24 lowercase letters and digits".to_string());
    }
    let container = container.trim().to_string();
    let alnum = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if !(3..=63).contains(&container.len())
        || !container
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || !alnum(container.chars().next())
        || !alnum(container.chars().last())
        || container.contains("--")
    {
        return Err("container: 3–63 lowercase letters, digits and single dashes".to_string());
    }
    // Accept a pasted "Blob SAS URL" as well as the bare token.
    let sas = sas.trim();
    let sas = sas.split_once('?').map(|(_, q)| q).unwrap_or(sas);
    let sas = sas.trim_start_matches('?');
    let keys: Vec<String> = sas
        .split('&')
        .filter_map(|pair| pair.split_once('=').map(|(k, _)| k.to_ascii_lowercase()))
        .collect();
    let well_formed = is_token(sas, 16, 2048)
        && !sas.contains('#')
        && sas.split('&').all(|pair| pair.contains('='))
        && keys.iter().all(|k| SAS_PARAMS.contains(&k.as_str()));
    if !well_formed || !keys.iter().any(|k| k == "sig") || !keys.iter().any(|k| k == "sv") {
        return Err(
            "SAS token: the query string Azure generates (sv=…&…&sig=…), or the whole SAS URL"
                .to_string(),
        );
    }
    Ok(AzureForm {
        base_url: format!("https://{account}.blob.core.windows.net/{container}"),
        account,
        container,
        sas: sas.to_string(),
    })
}

/// A phone number in international form, returned as digits only (no `+`):
/// spaces, dots, dashes and parentheses are dropped; 6–15 digits (E.164).
pub fn validate_phone(number: &str) -> Result<String, String> {
    let digits: String = number
        .trim()
        .trim_start_matches('+')
        .chars()
        .filter(|c| !" .-()".contains(*c))
        .collect();
    if (6..=15).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit()) {
        Ok(digits)
    } else {
        Err("phone number: international format, e.g. +33 6 12 34 56 78".to_string())
    }
}

/// A WhatsApp Cloud API phone number id (the numeric id Meta shows next to
/// the number, not the number itself) and its access token.
pub fn validate_whatsapp(phone_number_id: &str, token: &str) -> Result<(String, String), String> {
    let id = phone_number_id.trim();
    if !(5..=32).contains(&id.len()) || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("phone number id: the numeric id from Meta's WhatsApp API setup".to_string());
    }
    let token = validate_api_key(token)?;
    Ok((id.to_string(), token))
}

/// A signal-cli-rest-api bridge: its URL, the registered number (`+` and
/// digits) and the token the bridge's reverse proxy expects.
pub fn validate_signal(
    base_url: &str,
    number: &str,
    token: &str,
) -> Result<(String, String, String), String> {
    let base_url = validate_base_url(base_url, true)?;
    let number = format!("+{}", validate_phone(number)?);
    let token = validate_api_key(token)?;
    Ok((base_url, number, token))
}

/// A message to send: 1–4000 characters.
pub fn validate_message(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 4000 {
        Err("message: 1–4000 characters".to_string())
    } else {
        Ok(text.to_string())
    }
}

/// A repository's full name on a code host: `owner/name` on GitHub, `group[/
/// subgroup…]/name` on GitLab. Segments of letters, digits, `.`, `_`, `-`,
/// never `.` or `..`.
pub fn validate_repo_name(provider: Provider, full_name: &str) -> Result<String, String> {
    let name = full_name.trim().trim_matches('/');
    let segments: Vec<&str> = name.split('/').collect();
    let segment_ok = |s: &&str| {
        (1..=100).contains(&s.len())
            && *s != "."
            && *s != ".."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    };
    let count_ok = match provider {
        Provider::Github => segments.len() == 2,
        Provider::Gitlab => (2..=20).contains(&segments.len()),
        _ => false,
    };
    if count_ok && segments.iter().all(segment_ok) {
        Ok(name.to_string())
    } else {
        Err("repository: owner/name".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_plain_slug() {
        assert!(validate_org("acme-labs", "Acme Labs").is_ok());
    }

    #[test]
    fn refuses_malformed_slugs() {
        for slug in ["ab", "Acme", "acme_labs", "-acme", "acme-", "acme labs"] {
            assert!(
                validate_org(slug, "Acme").is_err(),
                "{slug} should be refused"
            );
        }
    }

    #[test]
    fn refuses_blank_or_long_names() {
        assert!(validate_org("acme", "   ").is_err());
        assert!(validate_org("acme", &"x".repeat(101)).is_err());
    }

    #[test]
    fn slugs_from_names() {
        assert_eq!(slugify("Acme Labs"), "acme-labs");
        assert_eq!(slugify("  Équipe  Données — R&D "), "equipe-donnees-r-d");
        assert_eq!(slugify("---"), "");
        assert_eq!(slugify(&"a b ".repeat(30)).len(), 39);
        assert!(validate_slug(&slugify(&"a b ".repeat(30))).is_ok());
    }

    #[test]
    fn provider_ids_round_trip() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_id(p.id()), Some(p));
        }
        assert_eq!(Provider::from_id("notion"), None);
        assert!(Provider::Anthropic.is_ai() && !Provider::S3.is_ai());
        assert!(Provider::Gitlab.is_code() && Provider::Signal.is_channel());
    }

    #[test]
    fn ai_providers_are_alphabetical() {
        let names: Vec<&str> = Provider::AI.iter().map(|p| p.name()).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|n| n.to_lowercase());
        assert_eq!(names, sorted);
    }

    #[test]
    fn base_urls() {
        assert_eq!(
            validate_base_url("https://api.example.com/v1/", true).unwrap(),
            "https://api.example.com/v1"
        );
        assert_eq!(
            validate_base_url("http://localhost:9000", false).unwrap(),
            "http://localhost:9000"
        );
        for bad in [
            "http://api.example.com",
            "ftp://x",
            "https://",
            "https://user@host",
            "https://host/v1?x=1",
            "https://host/#f",
            "https://host/a/../b",
            "https://host//v1",
            "https://ho st",
        ] {
            assert!(
                validate_base_url(bad, true).is_err(),
                "{bad} should be refused"
            );
        }
        assert!(validate_base_url("https://s3.example.com/path", false).is_err());
    }

    #[test]
    fn s3_forms() {
        let f = validate_s3(
            &aws_s3_endpoint("eu-west-3"),
            "eu-west-3",
            "my-bucket",
            "AK",
            "SK",
        )
        .unwrap();
        assert_eq!(f.base_url, "https://s3.eu-west-3.amazonaws.com/my-bucket");
        assert!(validate_s3("https://s3.x", "FR", "b-1", "a", "s").is_err());
        assert!(validate_s3("https://s3.x", "fr", "-b", "a", "s").is_err());
        assert!(validate_s3("https://s3.x", "fr", "bkt", "a b", "s").is_err());
    }

    #[test]
    fn azure_forms() {
        let sas =
            "sv=2022-11-02&ss=b&srt=co&sp=rl&se=2027-01-01T00:00:00Z&spr=https&sig=abc%2Bdef%3D";
        let f = validate_azure("acmestore", "notes", sas).unwrap();
        assert_eq!(f.base_url, "https://acmestore.blob.core.windows.net/notes");
        assert_eq!(f.sas, sas);
        // A pasted SAS URL, or a leading `?`, gives the same token.
        let url = format!("https://acmestore.blob.core.windows.net/notes?{sas}");
        assert_eq!(validate_azure("acmestore", "notes", &url).unwrap().sas, sas);
        assert_eq!(
            validate_azure("acmestore", "notes", &format!("?{sas}"))
                .unwrap()
                .sas,
            sas
        );
        assert!(validate_azure("Acme", "notes", sas).is_err());
        assert!(validate_azure("acmestore", "a--b", sas).is_err());
        assert!(validate_azure("acmestore", "notes", "sv=1&sp=r").is_err());
        assert!(validate_azure("acmestore", "notes", &format!("{sas}&comp=list")).is_err());
    }

    #[test]
    fn phones_and_channels() {
        assert_eq!(
            validate_phone("+33 6 12-34.56 (78)").unwrap(),
            "33612345678"
        );
        assert!(validate_phone("12345").is_err());
        assert!(validate_phone("+33 6 12 34 56 7a").is_err());
        assert!(validate_whatsapp("106540352242922", "EAAG-token-1234").is_ok());
        assert!(validate_whatsapp("+33612345678", "EAAG-token-1234").is_err());
        let (url, number, _) = validate_signal(
            "https://signal.example.com/",
            "+33 612345678",
            "secret-token",
        )
        .unwrap();
        assert_eq!(url, "https://signal.example.com");
        assert_eq!(number, "+33612345678");
        assert!(validate_message("  ").is_err());
        assert_eq!(validate_message(" hi ").unwrap(), "hi");
    }

    #[test]
    fn repo_names() {
        assert_eq!(
            validate_repo_name(Provider::Github, "/octo/hello.world/").unwrap(),
            "octo/hello.world"
        );
        assert!(validate_repo_name(Provider::Github, "a/b/c").is_err());
        assert!(validate_repo_name(Provider::Gitlab, "group/sub/proj").is_ok());
        assert!(validate_repo_name(Provider::Gitlab, "group/../proj").is_err());
        assert!(validate_repo_name(Provider::Gitlab, "group/pro j").is_err());
        assert!(validate_repo_name(Provider::S3, "a/b").is_err());
    }

    #[test]
    fn api_keys() {
        assert_eq!(validate_api_key("  sk-12345678 ").unwrap(), "sk-12345678");
        assert!(validate_api_key("short").is_err());
        assert!(validate_api_key("has a space in it").is_err());
    }
}
