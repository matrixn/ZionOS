use crate::{digest, fail, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use zpk_schema::{
    manifest::{Permission, StorageKind},
    Manifest,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub address_space_bytes: u64,
    pub cpu_seconds: u64,
    pub open_files: u64,
    pub processes: u64,
    pub file_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            address_space_bytes: 256 * 1024 * 1024,
            cpu_seconds: 300,
            open_files: 64,
            processes: 32,
            file_bytes: 8 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Restart {
    pub max: u32,
    pub period_seconds: u32,
    pub delay_seconds: u32,
    pub stop_seconds: u32,
}
impl Default for Restart {
    fn default() -> Self {
        Self {
            max: 3,
            period_seconds: 60,
            delay_seconds: 1,
            stop_seconds: 5,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Activation {
    Background,
    OnDemand,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StorageMode {
    EphemeralDevelopment,
    PersistentRequired,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HealthMode {
    #[default]
    Process,
    UnixSocket,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Grants {
    pub data: Vec<String>,
    pub configuration_write: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub health: HealthMode,
    pub runtime_version: u32,
    pub package_id: String,
    pub version: semver::Version,
    pub identity: Identity,
    pub activation: Activation,
    pub desktop_entry: Option<String>,
    pub autostart: bool,
    pub storage_mode: StorageMode,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub restart: Restart,
    #[serde(default)]
    pub grants: Grants,
}
#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
    pub app: PathBuf,
    pub version: PathBuf,
    pub current: PathBuf,
    pub data: PathBuf,
    pub configuration: PathBuf,
    pub logs: PathBuf,
    pub run: PathBuf,
    pub control: PathBuf,
    pub config: PathBuf,
    pub service: String,
}
pub fn identity_name(id: &str) -> String {
    format!("zp_{}", &digest(id.as_bytes())[..20])
}
impl Layout {
    pub fn new(root: &Path, c: &RuntimeConfig) -> Result<Self> {
        if !root.is_absolute()
            || root.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return fail("runtime root must be absolute and canonical");
        }
        if !zpk_schema::manifest::package_id(&c.package_id) || c.version.to_string().len() > 100 {
            return fail("invalid runtime package ID/version");
        }
        let app = root.join("opt/zionos/apps").join(&c.package_id);
        let version = app.join("versions").join(c.version.to_string());
        let configuration = root.join("etc/zionos/apps").join(&c.package_id);
        let key = identity_name(&c.package_id);
        Ok(Self {
            root: root.into(),
            current: app.join("current"),
            app,
            version,
            data: root.join("var/lib/zionos/apps").join(&c.package_id),
            config: configuration.join("runtime.toml"),
            configuration,
            logs: root.join("var/log/zionos/apps").join(&c.package_id),
            run: root.join("run/zionos/apps").join(&key),
            control: root.join("run/zionos/zpk-supervisors"),
            service: format!("zpk-{}", &key[3..]),
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Program {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct Plan {
    pub config: RuntimeConfig,
    pub layout: Layout,
    pub manifest: Manifest,
    pub program: Option<Program>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopAction {
    EnsureSharedBackend,
    StartOnDemand,
    StaticAssets,
}
impl RuntimeConfig {
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > 64 * 1024 {
            return fail("runtime config exceeds 64 KiB");
        }
        Ok(toml::from_str(source)?)
    }
    pub fn plan(self, manifest: Manifest, root: &Path) -> Result<Plan> {
        manifest.validate()?;
        if self.runtime_version != 1
            || self.package_id != manifest.package.id
            || self.version != manifest.package.version
        {
            return fail("runtime/schema/package version mismatch");
        }
        if self.identity.name != identity_name(&self.package_id)
            || !(10000..=60000).contains(&self.identity.uid)
            || !(10000..=60000).contains(&self.identity.gid)
        {
            return fail(
                "a dedicated non-root package identity (UID/GID 10000..60000) is required",
            );
        }
        let l = &self.limits;
        if !(16 * 1024 * 1024..=16 * 1024 * 1024 * 1024).contains(&l.address_space_bytes)
            || !(1..=86400).contains(&l.cpu_seconds)
            || !(16..=1024).contains(&l.open_files)
            || !(1..=128).contains(&l.processes)
            || !(4096..=64 * 1024 * 1024).contains(&l.file_bytes)
        {
            return fail("unsupported resource limit");
        }
        let r = &self.restart;
        if !(1..=10).contains(&r.max)
            || !(10..=3600).contains(&r.period_seconds)
            || !(1..=60).contains(&r.delay_seconds)
            || !(1..=30).contains(&r.stop_seconds)
        {
            return fail("bounded respawn and shutdown limits are required");
        }
        if self.autostart && self.activation != Activation::Background {
            return fail("on-demand applications cannot autostart");
        }
        for (names, kind, permission) in [
            (
                &self.grants.data,
                StorageKind::Data,
                Permission::PersistentData,
            ),
            (
                &self.grants.configuration_write,
                StorageKind::Config,
                Permission::ConfigurationWrite,
            ),
        ] {
            let mut seen = BTreeSet::new();
            for name in names {
                if !seen.insert(name)
                    || !manifest.permissions.contains(&permission)
                    || !manifest.storage.iter().any(|s| {
                        s.name == *name
                            && std::mem::discriminant(&s.kind) == std::mem::discriminant(&kind)
                    })
                {
                    return fail("undeclared/duplicate persistent directory grant");
                }
            }
        }
        let layout = Layout::new(root, &self)?;
        if self.desktop_entry.is_some() && manifest.service.is_some() {
            return fail("desktop entries of a shared service use that service; select no separate desktop executable");
        }
        let program = if let Some(entry) = &self.desktop_entry {
            if self.activation != Activation::OnDemand {
                return fail("desktop entry execution must be on-demand");
            }
            let app = manifest
                .desktop
                .iter()
                .find(|a| a.id == *entry)
                .ok_or("unknown desktop entry")?;
            app.executable.as_ref().map(|path| Program {
                executable: layout.version.join(path),
                arguments: vec![],
            })
        } else if let Some(service) = &manifest.service {
            Some(Program {
                executable: layout.version.join(&service.executable),
                arguments: service.arguments.clone(),
            })
        } else {
            if manifest.desktop.is_empty() || self.activation == Activation::Background {
                return fail("no native service/desktop activation declared");
            }
            None
        };
        if self.autostart && program.is_none() {
            return fail("static desktop has no service to autostart");
        }
        Ok(Plan {
            config: self,
            layout,
            manifest,
            program,
        })
    }
}
impl Plan {
    pub fn desktop_action(&self, id: &str) -> Result<DesktopAction> {
        let app = self
            .manifest
            .desktop
            .iter()
            .find(|a| a.id == id)
            .ok_or("unknown desktop entry")?;
        Ok(if self.manifest.service.is_some() {
            DesktopAction::EnsureSharedBackend
        } else if app.executable.is_some() {
            DesktopAction::StartOnDemand
        } else {
            DesktopAction::StaticAssets
        })
    }
}
