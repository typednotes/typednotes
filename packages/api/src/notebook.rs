//! Notebook declarations and organization-owned execution permissions.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{node_of, validate_ident, Cell, CellType, GraphNode, Provider};

/// Display order never changes a cell's declaration number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellOrder {
    #[default]
    Declaration,
    Name,
    Topological,
}

/// Explicit `@cell_name` references in prose. Ordinary words are not dependencies.
pub fn cell_references(description: &str) -> Vec<String> {
    let chars: Vec<char> = description.chars().collect();
    let mut names = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if c != '@' || (i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')) {
            continue;
        }
        let name: String = chars[i + 1..]
            .iter()
            .copied()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if validate_ident("reference", &name).is_ok() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Rename explicit references, preserving ordinary prose, email addresses and
/// longer identifiers. The caller saves these dependent declarations atomically.
pub fn renamed_dependents(cells: &[Cell], old: &str, new: &str) -> Vec<Cell> {
    cells.iter().filter_map(|cell| {
        let mut next = cell.clone();
        if let Some(deps) = &mut next.config.dependencies {
            for dep in deps { if dep == old { *dep = new.into(); } }
        }
        let chars: Vec<(usize, char)> = cell.description.char_indices().collect();
        let mut description = String::new();
        let mut copied = 0;
        for (i, &(offset, ch)) in chars.iter().enumerate() {
            if ch != '@' || (i > 0 && (chars[i - 1].1.is_alphanumeric() || chars[i - 1].1 == '_')) { continue; }
            let end = chars[i + 1..].iter().find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_')
                .map_or(cell.description.len(), |(offset, _)| *offset);
            if &cell.description[offset + 1..end] == old {
                description.push_str(&cell.description[copied..offset + 1]);
                description.push_str(new);
                copied = end;
            }
        }
        description.push_str(&cell.description[copied..]);
        next.description = description;
        if next.description != cell.description || next.config != cell.config { Some(next) } else { None }
    }).collect()
}

/// Named arguments in their declared order. Old cells without a declaration
/// retain their built graph's dependencies until edited.
pub fn cell_dependencies(cell: &Cell, cells: &[Cell], nodes: &[GraphNode]) -> Vec<String> {
    if let Some(names) = &cell.config.dependencies {
        return names.clone();
    }
    let references = cell_references(&cell.description);
    if !references.is_empty() {
        return references;
    }
    node_of(nodes, cell)
        .map(|node| {
            node.args.iter().filter_map(|id| {
                cells.iter().find(|c| node_of(nodes, c).is_some_and(|n| n.id == *id))
                    .map(|c| c.name.clone())
            }).collect()
        })
        .unwrap_or_default()
}

/// Dependencies must name value-producing cells, be distinct, and form a DAG.
pub fn validate_dependencies(cells: &[Cell]) -> Result<(), String> {
    let mut remaining: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for cell in cells {
        let deps = cell.config.dependencies.clone().unwrap_or_else(|| cell_references(&cell.description));
        if cell.cell_type.is_source() && !deps.is_empty() {
            return Err(format!("{}: source cells cannot depend on another cell", cell.name));
        }
        let mut set = BTreeSet::new();
        for dep in &deps {
            let target = cells.iter().find(|c| c.name == *dep)
                .ok_or_else(|| format!("{}: no cell named @{dep}", cell.name))?;
            if target.id == cell.id {
                return Err(format!("{}: a cell cannot depend on itself", cell.name));
            }
            if target.cell_type == CellType::Secret {
                return Err(format!("@{dep} is a secret capability, not a cell value"));
            }
            if !set.insert(target.name.as_str()) {
                return Err(format!("{}: @{dep} is listed twice", cell.name));
            }
        }
        remaining.insert(&cell.name, set);
    }
    while !remaining.is_empty() {
        let ready: Vec<_> = remaining.iter().filter(|(_, deps)| deps.is_empty()).map(|(name, _)| *name).collect();
        if ready.is_empty() {
            return Err(format!("dependency cycle: {}", remaining.keys().copied().collect::<Vec<_>>().join(" → ")));
        }
        for name in ready {
            remaining.remove(name);
            for deps in remaining.values_mut() {
                deps.remove(name);
            }
        }
    }
    Ok(())
}

/// Stable topological order, with name then id as the ready-node tie-breaker.
pub fn ordered_cells(cells: &[Cell], nodes: &[GraphNode], order: CellOrder) -> Vec<Cell> {
    let mut out = cells.to_vec();
    match order {
        CellOrder::Declaration => out.sort_by(|a, b| (a.position, &a.id).cmp(&(b.position, &b.id))),
        CellOrder::Name => out.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id))),
        CellOrder::Topological => {
            out.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
            let mut done = BTreeSet::new();
            let mut sorted = Vec::new();
            while !out.is_empty() {
                let ready = out.iter().position(|c| cell_dependencies(c, cells, nodes).iter().all(|n| done.contains(n)));
                let Some(index) = ready else {
                    // Legacy/unavailable graph metadata stays visible, even if inconsistent.
                    sorted.extend(out);
                    return sorted;
                };
                let cell = out.remove(index);
                done.insert(cell.name.clone());
                sorted.push(cell);
            }
            out = sorted;
        }
    }
    out
}

pub const NOTEBOOK_EFFECTS: &[&str] = &["Trace", "Error", "HTTP", "FileSystem", "PostgreSQL", "SecretStore", "ObjectStore", "Connector"];
pub const WRITER_TOOLS: &[&str] = &["read", "ls", "grep", "write", "edit", "bash", "todo", "check", "lsp", "publish", "lun_build", "lun_call"];

/// Admin-owned upper bounds. Empty lists grant nothing; permissions are never
/// widened by a cell's generated code or by a session update.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectPolicy {
    pub effects: Vec<String>,
    pub domains: Vec<String>,
    /// Default: derive exact domains from configured notebook URLs. An admin
    /// can switch to an explicit list, including an empty deny-all list.
    #[serde(default = "default_true")]
    pub configured_domains: bool,
    pub tools: Vec<String>,
    pub providers: Vec<String>,
    /// Optional per-provider operation/resource ceilings; omitted inherits the
    /// connection ceiling. An explicit empty scope list denies all operations.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connector_ceilings: BTreeMap<String, crate::ConnectorPermissions>,
}

fn default_true() -> bool { true }

impl Default for EffectPolicy {
    fn default() -> Self {
        Self {
            effects: NOTEBOOK_EFFECTS.iter().map(|s| s.to_string()).collect(),
            domains: Vec::new(),
            configured_domains: true,
            tools: WRITER_TOOLS.iter().map(|s| s.to_string()).collect(),
            providers: Provider::ALL.iter().map(|p| p.id().to_string()).collect(),
            connector_ceilings: BTreeMap::new(),
        }
    }
}

impl EffectPolicy {
    pub fn validate(&self) -> Result<Self, String> {
        let normalize = |values: &[String], allowed: &[&str], what: &str| -> Result<Vec<String>, String> {
            let mut result = BTreeSet::new();
            for value in values {
                let value = value.trim();
                if !allowed.contains(&value) {
                    return Err(format!("{what}: unknown '{value}'"));
                }
                result.insert(value.to_string());
            }
            Ok(result.into_iter().collect())
        };
        let providers: Vec<_> = Provider::ALL.iter().map(|p| p.id()).collect();
        let mut domains = BTreeSet::new();
        for domain in &self.domains {
            let domain = domain.trim().to_ascii_lowercase();
            if domain.is_empty() { continue; }
            if domain.len() > 253 || domain.parse::<std::net::IpAddr>().is_ok()
                || !domain.contains('.') || domain.ends_with(".internal") || domain.ends_with(".localhost")
                || domain.split('.').any(|part| part.is_empty() || part.len() > 63
                    || part.starts_with('-') || part.ends_with('-')
                    || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
                return Err(format!("domain: '{domain}' must be a public DNS hostname, without a path or wildcard"));
            }
            domains.insert(domain);
        }
        if domains.len() > 100 { return Err("domains: at most 100".into()); }
        let mut ceilings = BTreeMap::new();
        for (id, permissions) in &self.connector_ceilings {
            let validated = if let Some(service) = crate::LocalService::from_id(id) {
                permissions.validate_local(service)?
            } else {
                let provider = Provider::from_id(id).ok_or_else(|| format!("unknown connector provider '{id}'"))?;
                permissions.validate(provider)?
            };
            ceilings.insert(id.clone(), validated);
        }
        Ok(Self {
            effects: normalize(&self.effects, NOTEBOOK_EFFECTS, "effect")?,
            domains: domains.into_iter().collect(),
            configured_domains: self.configured_domains,
            tools: normalize(&self.tools, WRITER_TOOLS, "tool")?,
            providers: normalize(&self.providers, &providers, "provider")?,
            connector_ceilings: ceilings,
        })
    }

    pub fn allows_provider(&self, provider: Provider) -> bool {
        self.providers.iter().any(|p| p == provider.id())
    }

    pub fn for_cells(&self, cells: &[Cell]) -> Self {
        let mut policy = self.clone();
        if policy.configured_domains {
            policy.domains = cells.iter().filter_map(|cell| cell.config.url.as_deref())
                .filter_map(|url| url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")))
                .map(|rest| crate::host_of(rest.split(['/', '?']).next().unwrap_or("")).to_ascii_lowercase())
                .collect::<BTreeSet<_>>().into_iter().collect();
        }
        policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CellConfig;

    fn cell(name: &str, position: i32, deps: &[&str]) -> Cell {
        Cell { id: name.into(), name: name.into(), position, cell_type: CellType::Node,
            description: "Compute".into(), config: CellConfig { dependencies: Some(deps.iter().map(|s| s.to_string()).collect()), ..Default::default() },
            implementation: None, secret_set: false, has_endpoint: false, last_input: None,
            next_due_at: None, last_check: None, writing: false, issue: None, stale: false }
    }

    #[test]
    fn names_form_a_dag_and_sort_without_changing_declaration_numbers() {
        let mut cells = vec![cell("total", 0, &["price", "tax"]), cell("tax", 1, &[]), cell("price", 2, &[])];
        assert!(validate_dependencies(&cells).is_ok());
        let order = ordered_cells(&cells, &[], CellOrder::Topological);
        assert_eq!(order.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["price", "tax", "total"]);
        assert_eq!(order[0].position, 2);
        cells[2].config.dependencies = Some(vec!["total".into()]);
        assert!(validate_dependencies(&cells).unwrap_err().contains("cycle"));
        cells[2].config.dependencies = Some(vec!["missing".into()]);
        assert!(validate_dependencies(&cells).unwrap_err().contains("no cell"));
        cells[2].config.dependencies = Some(vec!["price".into()]);
        assert!(validate_dependencies(&cells).unwrap_err().contains("itself"));
    }

    #[test]
    fn references_are_explicit_and_permissions_are_exact() {
        assert_eq!(cell_references("Add @price to @tax; @price is repeated. me@example.com"), ["price", "tax"]);
        let policy = EffectPolicy { domains: vec!["API.Example.com".into()], ..Default::default() }.validate().unwrap();
        assert_eq!(policy.domains, ["api.example.com"]);
        for domain in ["*.example.com", "api.example.com/path", "127.0.0.1", "api.localhost"] {
            assert!(EffectPolicy { domains: vec![domain.into()], ..Default::default() }.validate().is_err());
        }
        assert!(!EffectPolicy { providers: Vec::new(), ..Default::default() }.allows_provider(Provider::Openai));
        assert!(WRITER_TOOLS.contains(&"lsp"));
        assert!(EffectPolicy::default().tools.iter().any(|tool| tool == "lsp"));
        assert!(EffectPolicy { tools: vec!["lsp".into()], ..Default::default() }.validate().is_ok());
        assert!(EffectPolicy { tools: vec!["lsp_rpc".into()], ..Default::default() }.validate().is_err());
    }

    #[test]
    fn renaming_updates_arguments_and_whole_prose_references() {
        let mut total = cell("total", 2, &["amount", "tax"]);
        total.description = "Résumé: @amount plus @tax; @amount_old and me@amount stay. (@amount)".into();
        let changes = renamed_dependents(&[total], "amount", "price");
        assert_eq!(changes[0].config.dependencies.as_ref().unwrap(), &["price", "tax"]);
        assert_eq!(changes[0].description, "Résumé: @price plus @tax; @amount_old and me@amount stay. (@price)");
    }
}
