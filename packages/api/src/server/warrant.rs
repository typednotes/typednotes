//! Warrant minting — `core`'s half of the macaroon chain liaison verifies
//! (`liaison/Liaison/Warrant/{Caveat,Tag}.lean`, docs/services/broker.md).
//!
//! `s₀ = HMAC(rootKey, lp(id) ‖ lp(orgId))`, then `sᵢ = HMAC(sᵢ₋₁, bytes(cᵢ))`
//! folded over the caveats **in minting order**; the tag is the last `s`.
//! `lp` is a big-endian u64 byte length followed by the UTF-8 bytes. The JSON
//! form lists caveats most-recent-first, because liaison's `attenuate`
//! prepends. Any change here must be mirrored in `Tag.lean`, or every warrant
//! this app mints is refused as `tag_invalid`.

use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

use super::config::env;

#[derive(Clone, Debug, PartialEq)]
pub enum Caveat {
    /// Unix seconds; liaison permits `now < t`.
    ExpiresAt(u64),
    Capability {
        provider: String,
        action: String,
    },
    Resource(String),
    Budget(u64),
    RunId(String),
}

fn u64_bytes(n: u64) -> [u8; 8] {
    n.to_be_bytes()
}

fn len_prefixed(s: &str) -> Vec<u8> {
    let mut out = u64_bytes(s.len() as u64).to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

impl Caveat {
    /// `Caveat.toBytes`: a distinct leading byte per constructor.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Caveat::ExpiresAt(t) => {
                out.push(0);
                out.extend_from_slice(&u64_bytes(*t));
            }
            Caveat::Capability { provider, action } => {
                out.push(1);
                out.extend(len_prefixed(provider));
                out.extend(len_prefixed(action));
            }
            Caveat::Resource(id) => {
                out.push(2);
                out.extend(len_prefixed(id));
            }
            Caveat::Budget(c) => {
                out.push(3);
                out.extend_from_slice(&u64_bytes(*c));
            }
            Caveat::RunId(r) => {
                out.push(4);
                out.extend(len_prefixed(r));
            }
        }
        out
    }

    /// The caveat's JSON in liaison's request format (numbers as strings).
    pub fn to_json(&self) -> Value {
        match self {
            Caveat::ExpiresAt(t) => json!({"kind": "expiresAt", "value": t.to_string()}),
            Caveat::Capability { provider, action } => {
                json!({"kind": "capability", "provider": provider, "action": action})
            }
            Caveat::Resource(id) => json!({"kind": "resource", "value": id}),
            Caveat::Budget(c) => json!({"kind": "budget", "value": c.to_string()}),
            Caveat::RunId(r) => json!({"kind": "runId", "value": r}),
        }
    }
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Warrant {
    pub id: String,
    pub org_id: String,
    /// In minting order (first applied first).
    pub caveats: Vec<Caveat>,
    pub tag: Vec<u8>,
}

/// Mint a warrant over `caveats`, given in minting order.
pub fn mint(root_key: &[u8], id: &str, org_id: &str, caveats: Vec<Caveat>) -> Warrant {
    let mut input = len_prefixed(id);
    input.extend(len_prefixed(org_id));
    let s0 = hmac(root_key, &input);
    let tag = caveats.iter().fold(s0, |s, c| hmac(&s, &c.to_bytes()));
    Warrant {
        id: id.to_string(),
        org_id: org_id.to_string(),
        caveats,
        tag,
    }
}

impl Warrant {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "orgId": self.org_id,
            "tag": hex::encode(&self.tag),
            "caveats": self.caveats.iter().rev().map(Caveat::to_json).collect::<Vec<_>>(),
        })
    }
}

/// `LIAISON_ROOT_KEY`, hex — the same key liaison verifies with.
pub fn root_key() -> Result<Vec<u8>, String> {
    let hex_key = env("LIAISON_ROOT_KEY").ok_or("LIAISON_ROOT_KEY is not set")?;
    let key = hex::decode(&hex_key).map_err(|_| "LIAISON_ROOT_KEY is not hex".to_string())?;
    if key.is_empty() {
        return Err("LIAISON_ROOT_KEY is empty".to_string());
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(p: &str, a: &str) -> Caveat {
        Caveat::Capability {
            provider: p.into(),
            action: a.into(),
        }
    }

    #[test]
    fn caveat_bytes_match_tag_lean() {
        assert_eq!(
            Caveat::ExpiresAt(0x0102).to_bytes(),
            vec![0, 0, 0, 0, 0, 0, 0, 1, 2]
        );
        assert_eq!(
            cap("gh", "r").to_bytes(),
            [
                vec![1],
                vec![0, 0, 0, 0, 0, 0, 0, 2],
                b"gh".to_vec(),
                vec![0, 0, 0, 0, 0, 0, 0, 1],
                b"r".to_vec()
            ]
            .concat()
        );
        assert_eq!(Caveat::Resource("x".into()).to_bytes()[0], 2);
        assert_eq!(
            Caveat::Budget(5).to_bytes(),
            vec![3, 0, 0, 0, 0, 0, 0, 0, 5]
        );
        assert_eq!(Caveat::RunId("r".into()).to_bytes()[0], 4);
    }

    /// Attenuation without the root key: extending a tag by one caveat equals
    /// minting with that caveat appended — the property liaison relies on.
    #[test]
    fn chain_extends_like_attenuate() {
        let key = [7u8; 32];
        let base = mint(&key, "w", "o", vec![Caveat::ExpiresAt(10)]);
        let longer = mint(
            &key,
            "w",
            "o",
            vec![Caveat::ExpiresAt(10), Caveat::Budget(3)],
        );
        assert_eq!(hmac(&base.tag, &Caveat::Budget(3).to_bytes()), longer.tag);
        // Binding: another org, or reordered caveats, is another tag.
        assert_ne!(
            mint(&key, "w", "p", vec![Caveat::ExpiresAt(10)]).tag,
            base.tag
        );
        let swapped = mint(
            &key,
            "w",
            "o",
            vec![Caveat::Budget(3), Caveat::ExpiresAt(10)],
        );
        assert_ne!(swapped.tag, longer.tag);
    }

    #[test]
    fn json_lists_caveats_most_recent_first() {
        let w = mint(
            &[1],
            "w",
            "o",
            vec![Caveat::ExpiresAt(9), Caveat::Budget(0)],
        );
        let j = w.to_json();
        assert_eq!(j["caveats"][0], json!({"kind": "budget", "value": "0"}));
        assert_eq!(j["caveats"][1], json!({"kind": "expiresAt", "value": "9"}));
        assert_eq!(j["tag"].as_str().unwrap().len(), 64);
    }
}
