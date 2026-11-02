//! Read-only desktop projection v1. Contains no paths to private data or service controls.
use crate::{
    manifest::{self, FileKind, Permission, Window},
    Error, Manifest, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const LIMIT: usize = 2 * 1024 * 1024;
pub const MAX_ASSET: u64 = 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub size: u64,
    pub sha256: String,
    pub mime: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub package_id: String,
    pub version: semver::Version,
    pub desktop_id: String,
    pub title: String,
    pub categories: Vec<String>,
    pub window: Window,
    pub multiple_instances: bool,
    pub entry: String,
    pub icon: String,
    pub notifications: bool,
    pub manifest_sha256: String,
    pub assets: BTreeMap<String, Asset>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub schema_version: u32,
    pub apps: Vec<Entry>,
}
fn invalid(s: &str) -> Error {
    Error::Manifest(format!("desktop registry: {s}"))
}
pub fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn mime(path: &str, kind: &FileKind) -> Option<&'static str> {
    match kind {
        FileKind::Ui if path.ends_with(".html") => Some("text/html; charset=utf-8"),
        FileKind::Icon if path.ends_with(".png") => Some("image/png"),
        FileKind::Asset if path.ends_with(".js") || path.ends_with(".mjs") => {
            Some("application/javascript; charset=utf-8")
        }
        FileKind::Asset if path.ends_with(".css") => Some("text/css; charset=utf-8"),
        FileKind::Asset if path.ends_with(".png") => Some("image/png"),
        _ => None,
    }
}
impl Entry {
    pub fn asset_url(&self, path: &str) -> String {
        format!("/app-assets/{}/{}/{}", self.package_id, self.version, path)
    }
    pub fn validate(&self) -> Result<()> {
        if !manifest::package_id(&self.package_id)
            || self.desktop_id.is_empty()
            || self.desktop_id.len() > 64
            || !self
                .desktop_id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || self.id != format!("zpk:{}/{}", self.package_id, self.desktop_id)
            || self.title.trim().is_empty()
            || self.title.len() > 128
            || self.title.chars().any(char::is_control)
            || !hash(&self.manifest_sha256)
            || self.assets.len() > 2048
            || self.assets.is_empty()
            || self.categories.is_empty()
            || self.categories.len() > 16
            || self.categories.iter().any(|c| {
                ![
                    "utilities",
                    "development",
                    "multimedia",
                    "network",
                    "storage",
                    "backup",
                    "productivity",
                    "system",
                ]
                .contains(&c.as_str())
            })
        {
            return Err(invalid("identity, metadata or limits"));
        }
        let w = &self.window;
        if !(160..=7680).contains(&w.min_width)
            || !(120..=4320).contains(&w.min_height)
            || !(w.min_width..=7680).contains(&w.width)
            || !(w.min_height..=4320).contains(&w.height)
        {
            return Err(invalid("window dimensions"));
        }
        for (path, a) in &self.assets {
            if !manifest::safe_path(path)
                || a.size > MAX_ASSET
                || !hash(&a.sha256)
                || ![
                    "text/html; charset=utf-8",
                    "application/javascript; charset=utf-8",
                    "text/css; charset=utf-8",
                    "image/png",
                ]
                .contains(&a.mime.as_str())
                || !((path.starts_with("ui/")
                    && path.ends_with(".html")
                    && a.mime == "text/html; charset=utf-8")
                    || (path.starts_with("assets/")
                        && [
                            (".js", "application/javascript; charset=utf-8"),
                            (".mjs", "application/javascript; charset=utf-8"),
                            (".css", "text/css; charset=utf-8"),
                            (".png", "image/png"),
                        ]
                        .iter()
                        .any(|(suffix, m)| path.ends_with(suffix) && a.mime == *m)))
            {
                return Err(invalid("unsafe asset path, MIME or digest"));
            }
        }
        if self
            .assets
            .get(&self.entry)
            .is_none_or(|a| a.mime != "text/html; charset=utf-8")
            || self
                .assets
                .get(&self.icon)
                .is_none_or(|a| a.mime != "image/png")
        {
            return Err(invalid("entry/icon reference"));
        }
        Ok(())
    }
    pub fn from_manifest(
        m: &Manifest,
        assets: BTreeMap<String, Asset>,
        manifest_hash: String,
    ) -> Result<Vec<Self>> {
        m.validate()?;
        let mut out = vec![];
        for d in &m.desktop {
            let e = Self {
                id: format!("zpk:{}/{}", m.package.id, d.id),
                package_id: m.package.id.clone(),
                version: m.package.version.clone(),
                desktop_id: d.id.clone(),
                title: d.name.clone(),
                categories: d.categories.clone(),
                window: d.window.clone(),
                multiple_instances: d.multiple_instances,
                entry: d.entry.clone(),
                icon: d.icon.clone(),
                notifications: m.permissions.contains(&Permission::Notifications)
                    && d.permissions.contains(&Permission::Notifications),
                manifest_sha256: manifest_hash.clone(),
                assets: assets.clone(),
            };
            e.validate()?;
            out.push(e);
        }
        Ok(out)
    }
}
impl Registry {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > LIMIT {
            return Err(Error::Limit("desktop registry exceeds 2 MiB".into()));
        }
        let r: Self = serde_json::from_slice(bytes).map_err(|e| invalid(&e.to_string()))?;
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.apps.len() > 256 {
            return Err(invalid("unsupported version/entry count"));
        }
        let mut ids = BTreeSet::new();
        let mut packages = BTreeMap::new();
        for e in &self.apps {
            e.validate()?;
            if !ids.insert(&e.id) {
                return Err(invalid("duplicate entry"));
            }
            if let Some((version, manifest, assets)) = packages.insert(
                &e.package_id,
                (
                    &e.version,
                    &e.manifest_sha256,
                    serde_json::to_string(&e.assets).map_err(|_| invalid("assets"))?,
                ),
            ) {
                if version != &e.version
                    || manifest != &e.manifest_sha256
                    || assets != serde_json::to_string(&e.assets).map_err(|_| invalid("assets"))?
                {
                    return Err(invalid("inconsistent package snapshot"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> Entry {
        let m = Manifest::parse(include_str!(
            "../../../tests/zpk/fixtures/valid/multi-desktop.toml"
        ))
        .unwrap();
        let assets = BTreeMap::from([
            (
                "ui/index.html".into(),
                Asset {
                    size: 10,
                    sha256: "a".repeat(64),
                    mime: "text/html; charset=utf-8".into(),
                },
            ),
            (
                "assets/icon.png".into(),
                Asset {
                    size: 100,
                    sha256: "b".repeat(64),
                    mime: "image/png".into(),
                },
            ),
        ]);
        Entry::from_manifest(&m, assets, "c".repeat(64))
            .unwrap()
            .remove(0)
    }
    #[test]
    fn projection_roundtrips_without_executables_or_private_paths() {
        let registry = Registry {
            schema_version: 1,
            apps: vec![entry()],
        };
        let bytes = serde_json::to_vec(&registry).unwrap();
        let parsed = Registry::parse(&bytes).unwrap();
        assert!(!parsed.apps[0].multiple_instances);
        assert!(!parsed.apps[0].notifications);
        assert!(parsed.apps[0]
            .asset_url(&parsed.apps[0].entry)
            .starts_with("/app-assets/"));
        assert!(!String::from_utf8(bytes).unwrap().contains("payload/bin"));
    }
    #[test]
    fn projection_rejects_versions_duplicates_traversal_and_asset_type_confusion() {
        let mut r = Registry {
            schema_version: 2,
            apps: vec![entry()],
        };
        assert!(r.validate().is_err());
        r.schema_version = 1;
        r.apps.push(entry());
        assert!(r.validate().is_err());
        r.apps.pop();
        r.apps[0].assets.insert(
            "payload/bin/hello".into(),
            Asset {
                size: 64,
                sha256: "a".repeat(64),
                mime: "application/javascript; charset=utf-8".into(),
            },
        );
        assert!(r.validate().is_err());
        r.apps[0].assets.remove("payload/bin/hello");
        r.apps[0].entry = "ui/../secret.html".into();
        assert!(r.validate().is_err());
    }
    #[test]
    fn notifications_require_the_entry_permission_and_multiple_instances_are_explicit() {
        let mut m = Manifest::parse(include_str!(
            "../../../tests/zpk/fixtures/valid/multi-desktop.toml"
        ))
        .unwrap();
        m.permissions.push(Permission::Notifications);
        m.desktop[0].multiple_instances = true;
        let assets = entry().assets;
        let entries = Entry::from_manifest(&m, assets.clone(), "a".repeat(64)).unwrap();
        assert!(!entries[0].notifications);
        assert!(entries[0].multiple_instances);
        m.desktop[0].permissions.push(Permission::Notifications);
        assert!(Entry::from_manifest(&m, assets, "a".repeat(64)).unwrap()[0].notifications);
    }
}
