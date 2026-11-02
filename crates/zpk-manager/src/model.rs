use crate::{digest, fail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zpk_runtime::config::RuntimeConfig;
use zpk_schema::{manifest::Permission, Manifest};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub approved_declarations: Vec<Permission>,
    pub runtime_policy: String,
    pub manifest: String,
    pub configuration: RuntimeConfig,
    pub source: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub inventory_sha256: String,
}
impl Version {
    pub fn parsed(&self) -> Result<Manifest> {
        let m = Manifest::parse(&self.manifest)?;
        if digest(self.manifest.as_bytes()) != self.manifest_sha256
            || !hash(&self.archive_sha256)
            || !hash(&self.inventory_sha256)
            || self.source.len() > 4096
            || self.approved_declarations != m.permissions
            || self.runtime_policy.is_empty()
            || self.runtime_policy.len() > 2048
        {
            return fail("corrupt version digest/source metadata");
        }
        Ok(m)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub active: String,
    pub previous: Option<String>,
    pub versions: BTreeMap<String, Version>,
    pub service_state: String,
}
impl Record {
    pub fn validate(&self, root: &std::path::Path) -> Result<()> {
        if !zpk_schema::manifest::package_id(&self.id)
            || self.versions.is_empty()
            || self.versions.len() > 64
            || !self.versions.contains_key(&self.active)
            || self
                .previous
                .as_ref()
                .is_some_and(|p| p == &self.active || !self.versions.contains_key(p))
            || !["stopped", "running", "not-required", "unknown"]
                .contains(&self.service_state.as_str())
        {
            return fail("corrupt package registry record");
        }
        let active_identity = &self.current().configuration.identity;
        for (key, v) in &self.versions {
            let m = v.parsed()?;
            if m.package.id != self.id
                || m.package.version.to_string() != *key
                || v.configuration.identity.name != active_identity.name
                || v.configuration.identity.uid != active_identity.uid
                || v.configuration.identity.gid != active_identity.gid
            {
                return fail("registry package/version/identity mismatch");
            }
            v.configuration.clone().plan(m, root)?;
        }
        Ok(())
    }
    pub fn current(&self) -> &Version {
        &self.versions[&self.active]
    }
}
pub fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Kind {
    Install,
    Upgrade,
    Rollback,
    Uninstall,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub kind: Kind,
    pub package_id: String,
    pub old: Option<Record>,
    pub new: Option<Record>,
    pub was_running: bool,
    pub start: bool,
}
impl Operation {
    pub fn validate(&self, root: &std::path::Path) -> Result<()> {
        if !zpk_schema::manifest::package_id(&self.package_id) {
            return fail("corrupt journal package ID");
        }
        for record in [&self.old, &self.new].into_iter().flatten() {
            record.validate(root)?;
            if record.id != self.package_id {
                return fail("journal identity mismatch");
            }
        }
        let valid = match self.kind {
            Kind::Install => self.old.is_none() && self.new.is_some(),
            Kind::Upgrade | Kind::Rollback => self.old.is_some() && self.new.is_some(),
            Kind::Uninstall => self.old.is_some() && self.new.is_none(),
        };
        if !valid {
            return fail("corrupt journal operation shape");
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct Review {
    pub desktop_entry: Option<String>,
    pub trusted_local_development: bool,
    pub approved_permissions: Vec<Permission>,
    pub start: bool,
    pub autostart: bool,
    pub unix_health: bool,
}
impl Review {
    pub fn check(&self, m: &Manifest) -> Result<()> {
        if !self.trusted_local_development {
            return fail("unsigned packages require --trust-local-development; SHA-256 does not authenticate publishers");
        }
        let mut supplied = self.approved_permissions.clone();
        supplied.sort();
        let mut declared = m.permissions.clone();
        declared.sort();
        if supplied != declared {
            return fail(format!(
                "permission review must approve exactly the manifest declarations: {declared:?}"
            ));
        }
        if declared
            .iter()
            .any(|p| matches!(p, Permission::NetworkClient | Permission::NetworkListen))
        {
            return fail("network permission grants are not supported by the enforced server-only Runtime profile");
        }
        Ok(())
    }
}
