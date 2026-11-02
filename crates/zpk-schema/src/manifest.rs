use crate::{Error, Result};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    DesktopUi,
    BackgroundService,
    PersistentData,
    ConfigurationWrite,
    NetworkClient,
    NetworkListen,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub package: Package,
    pub target: Target,
    pub build: Build,
    pub runtime: Runtime,
    pub compatibility: Compatibility,
    pub permissions: Vec<Permission>,
    pub files: Vec<PackageFile>,
    #[serde(default)]
    pub service: Option<Service>,
    #[serde(default)]
    pub desktop: Vec<DesktopApp>,
    #[serde(default)]
    pub storage: Vec<Storage>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub author: String,
    pub license: String,
    pub categories: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub os: String,
    pub architecture: String,
    pub cpu: String,
    pub libc: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub system: BuildSystem,
    pub toolchain: String,
    pub reproducible: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildSystem {
    Rust,
    Make,
    Cmake,
    Meson,
    Custom,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub dependencies: Vec<Dependency>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub id: String,
    pub version: VersionReq,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub min_zionos: Version,
    pub upgrades_from: VersionReq,
    pub data_schema: u32,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    Executable,
    Library,
    Ui,
    Icon,
    Asset,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Linkage {
    Static,
    MuslDynamic,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFile {
    pub path: String,
    pub kind: FileKind,
    #[serde(default)]
    pub linkage: Option<Linkage>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub executable: String,
    pub arguments: Vec<String>,
    pub run_as: String,
    pub autostart: bool,
    pub permissions: Vec<Permission>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub entry: String,
    pub icon: String,
    #[serde(default)]
    pub executable: Option<String>,
    pub categories: Vec<String>,
    pub permissions: Vec<Permission>,
    pub window: Window,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub resizable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageKind {
    Data,
    Config,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Storage {
    pub name: String,
    pub kind: StorageKind,
    pub schema: u32,
    pub max_bytes: u64,
    pub preserve_on_upgrade: bool,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Manifest(message.into())
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
pub fn package_id(id: &str) -> bool {
    id.len() <= 253
        && id.split('.').count() >= 3
        && id.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && part.as_bytes()[0].is_ascii_lowercase()
                && part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
}
fn local_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
/// One portable spelling; rejects aliases, Windows separators/drives, controls and Unicode ambiguity.
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 240
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.len() <= 100
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
}
fn unique<T: Ord>(values: impl IntoIterator<Item = T>) -> bool {
    let mut seen = BTreeSet::new();
    values.into_iter().all(|v| seen.insert(v))
}
fn categories(values: &[String]) -> bool {
    !values.is_empty()
        && values.len() <= 16
        && unique(values.iter())
        && values.iter().all(|v| {
            [
                "utilities",
                "development",
                "multimedia",
                "network",
                "storage",
                "backup",
                "productivity",
                "system",
            ]
            .contains(&v.as_str())
        })
}
impl Manifest {
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > MANIFEST_LIMIT {
            return Err(Error::Limit("manifest exceeds 64 KiB".into()));
        }
        // Check the version before applying v1's strict field/type contract.
        let envelope: toml::Value = toml::from_str(source).map_err(|e| invalid(e.to_string()))?;
        let version = envelope
            .get("schema_version")
            .and_then(toml::Value::as_integer)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| invalid("schema_version must be a u32"))?;
        if version != SCHEMA_VERSION {
            return Err(Error::UnsupportedSchema(version));
        }
        let manifest: Self = toml::from_str(source).map_err(|e| invalid(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::UnsupportedSchema(self.schema_version));
        }
        if !package_id(&self.package.id) {
            return Err(invalid(
                "package.id must be a lowercase reverse-domain identifier (at least 3 components)",
            ));
        }
        for (field, value, limit) in [
            ("name", &self.package.name, 128),
            ("description", &self.package.description, 4096),
            ("author", &self.package.author, 256),
            ("license", &self.package.license, 128),
            ("build.toolchain", &self.build.toolchain, 256),
        ] {
            if !text(value, limit) {
                return Err(invalid(format!("invalid {field}")));
            }
        }
        if !categories(&self.package.categories) {
            return Err(invalid("invalid/duplicate package categories"));
        }
        if self.target.os != "linux"
            || self.target.architecture != "x86_64"
            || self.target.cpu != "x86-64-v1"
            || self.target.libc != "musl"
        {
            return Err(Error::UnsupportedTarget(format!(
                "os={} architecture={} cpu={} libc={}; v1 supports linux/x86_64/x86-64-v1/musl",
                self.target.os, self.target.architecture, self.target.cpu, self.target.libc
            )));
        }
        if !unique(self.permissions.iter()) {
            return Err(invalid("duplicate permission"));
        }
        let declared: BTreeSet<_> = self.permissions.iter().collect();
        let check_permissions = |needed: &[Permission]| -> Result<()> {
            if !unique(needed.iter()) || needed.iter().any(|p| !declared.contains(p)) {
                return Err(invalid("undeclared or duplicate feature/file permission"));
            }
            Ok(())
        };
        if self.files.is_empty()
            || self.files.len() > 2048
            || !unique(self.files.iter().map(|f| &f.path))
        {
            return Err(invalid("files must have 1..2048 unique paths"));
        }
        let files: BTreeMap<_, _> = self.files.iter().map(|f| (f.path.as_str(), f)).collect();
        for file in &self.files {
            if !safe_path(&file.path) {
                return Err(invalid(format!("unsafe file path: {}", file.path)));
            }
            let correct = match file.kind {
                FileKind::Executable => {
                    file.path.starts_with("payload/bin/") && file.linkage.is_some()
                }
                FileKind::Library => {
                    file.path.starts_with("payload/lib/") && file.linkage.is_none()
                }
                FileKind::Ui => file.path.starts_with("ui/") && file.linkage.is_none(),
                FileKind::Icon => {
                    file.path.starts_with("assets/")
                        && file.path.ends_with(".png")
                        && file.linkage.is_none()
                }
                FileKind::Asset => file.path.starts_with("assets/") && file.linkage.is_none(),
            };
            if !correct {
                return Err(invalid(format!(
                    "file kind/layout/linkage mismatch: {}",
                    file.path
                )));
            }
            check_permissions(&file.permissions)?;
            // Reject file-vs-directory aliases irrespective of declaration ordering.
            if file
                .path
                .match_indices('/')
                .any(|(i, _)| files.contains_key(&file.path[..i]))
            {
                return Err(invalid("file path is also an ancestor directory"));
            }
        }
        if !self.files.iter().any(|f| f.kind == FileKind::Executable) {
            return Err(invalid("at least one native executable is required"));
        }
        let reference = |path: &str, kind: FileKind| -> Result<()> {
            if !files.get(path).is_some_and(|f| f.kind == kind) {
                return Err(invalid(format!(
                    "missing/wrong-kind file reference: {path}"
                )));
            }
            Ok(())
        };
        if let Some(service) = &self.service {
            check_permissions(&[Permission::BackgroundService])?;
            check_permissions(&service.permissions)?;
            reference(&service.executable, FileKind::Executable)?;
            if service.run_as != "package"
                || service.arguments.len() > 64
                || service.arguments.iter().any(|s| !text(s, 1024))
            {
                return Err(invalid(
                    "service must use run_as=package and bounded literal arguments",
                ));
            }
        }
        if self.desktop.len() > 32 || !unique(self.desktop.iter().map(|d| &d.id)) {
            return Err(invalid("too many or duplicate desktop entries"));
        }
        for app in &self.desktop {
            check_permissions(&[Permission::DesktopUi])?;
            check_permissions(&app.permissions)?;
            if !local_id(&app.id)
                || !text(&app.name, 128)
                || !categories(&app.categories)
                || !app.entry.ends_with(".html")
            {
                return Err(invalid("invalid desktop id/name/categories/HTML entry"));
            }
            reference(&app.entry, FileKind::Ui)?;
            reference(&app.icon, FileKind::Icon)?;
            if let Some(executable) = &app.executable {
                reference(executable, FileKind::Executable)?;
            }
            let w = &app.window;
            if !(160..=7680).contains(&w.min_width)
                || !(120..=4320).contains(&w.min_height)
                || !(w.min_width..=7680).contains(&w.width)
                || !(w.min_height..=4320).contains(&w.height)
            {
                return Err(invalid("invalid desktop window dimensions"));
            }
        }
        if self.storage.len() > 32 || !unique(self.storage.iter().map(|s| &s.name)) {
            return Err(invalid("too many or duplicate storage requirements"));
        }
        for storage in &self.storage {
            if !local_id(&storage.name) || storage.schema == 0 || storage.max_bytes == 0 {
                return Err(invalid("invalid persistent storage requirement"));
            }
            check_permissions(&[match storage.kind {
                StorageKind::Data => Permission::PersistentData,
                StorageKind::Config => Permission::ConfigurationWrite,
            }])?;
        }
        let c = &self.compatibility;
        if c.data_schema == 0 {
            return Err(invalid("compatibility.data_schema must be positive"));
        }
        let deps: Vec<_> = self
            .runtime
            .dependencies
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        for group in [
            deps,
            c.conflicts.iter().map(String::as_str).collect(),
            c.replaces.iter().map(String::as_str).collect(),
        ] {
            if group.len() > 128
                || !unique(group.iter())
                || group
                    .iter()
                    .any(|id| !package_id(id) || *id == self.package.id)
            {
                return Err(invalid(
                    "invalid/duplicate/self dependency, conflict or replacement",
                ));
            }
        }
        if self
            .runtime
            .dependencies
            .iter()
            .any(|d| c.conflicts.contains(&d.id))
        {
            return Err(invalid("dependency also listed as conflict"));
        }
        Ok(())
    }
    /// Checks explicit target OS version and, optionally, an in-place upgrade source.
    pub fn check_compatibility(&self, zionos: &Version, installed: Option<&Version>) -> Result<()> {
        self.validate()?;
        if zionos < &self.compatibility.min_zionos {
            return Err(Error::Incompatible(format!(
                "requires ZionOS >= {}",
                self.compatibility.min_zionos
            )));
        }
        if let Some(previous) = installed {
            if previous >= &self.package.version
                || !self.compatibility.upgrades_from.matches(previous)
            {
                return Err(Error::Incompatible(
                    "installed version is outside upgrades_from or is not older".into(),
                ));
            }
        }
        Ok(())
    }
}
