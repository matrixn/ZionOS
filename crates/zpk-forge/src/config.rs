use crate::{fail, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};
use zpk_schema::{
    manifest::{safe_path, BuildSystem},
    Manifest,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum System {
    Rust,
    Cmake,
    Go,
    Make,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub from: String,
    pub to: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub system: System,
    pub inputs: Vec<String>,
    pub license_file: String,
    pub stage: Vec<Rule>,
    #[serde(default = "default_jobs")]
    pub jobs: u8,
    pub cargo_bin: Option<String>,
    pub go_package: Option<String>,
    pub make_target: Option<String>,
}
fn default_jobs() -> u8 {
    2
}
pub struct Project {
    pub manifest: Manifest,
    pub recipe: Recipe,
    pub authoring: String,
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
pub fn source_path(path: &str) -> bool {
    safe_path(path)
        && !path.split('/').any(|p| {
            matches!(
                p,
                ".git"
                    | ".ssh"
                    | ".aws"
                    | ".docker"
                    | ".kube"
                    | ".netrc"
                    | ".git-credentials"
                    | ".npmrc"
                    | ".pypirc"
                    | ".vault-token"
                    | "id_ecdsa"
                    | "id_dsa"
                    | ".azure"
                    | ".gnupg"
                    | ".env"
                    | "credentials"
                    | "credentials.toml"
                    | "id_rsa"
                    | "id_ed25519"
                    | "target"
                    | "node_modules"
                    | ".zpk-forge"
            ) || p.starts_with(".env.")
                || [".pem", ".key", ".rsa", ".p12", ".pfx"]
                    .iter()
                    .any(|ext| p.ends_with(ext))
        })
}
impl Project {
    pub fn load(root: &Path) -> Result<Self> {
        let text = crate::read_limited(&root.join("zpk.toml"), 64 * 1024)?;
        Self::parse(std::str::from_utf8(&text)?)
    }
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > 64 * 1024 {
            return fail("Forge manifest exceeds 64 KiB");
        }
        let mut value: toml::Table = toml::from_str(source)?;
        let mut unresolved = Vec::new();
        unresolved_fields(&toml::Value::Table(value.clone()), "", &mut unresolved);
        if !unresolved.is_empty() {
            return fail(format!(
                "unresolved developer input: {}",
                unresolved.join(", ")
            ));
        }
        let recipe: Recipe = value
            .remove("forge")
            .ok_or("missing [forge] recipe")?
            .try_into()?;
        let manifest = Manifest::parse(&toml::to_string(&value)?)?;
        recipe.validate(&manifest)?;
        Ok(Self {
            manifest,
            recipe,
            authoring: source.to_owned(),
        })
    }
}
impl Recipe {
    pub fn validate(&self, manifest: &Manifest) -> Result<()> {
        if self.version != 1 || !(1..=8).contains(&self.jobs) {
            return fail("unsupported Forge recipe version or jobs (1..8)");
        }
        let expected = match self.system {
            System::Rust => BuildSystem::Rust,
            System::Cmake => BuildSystem::Cmake,
            System::Go => BuildSystem::Go,
            System::Make => BuildSystem::Make,
        };
        if std::mem::discriminant(&expected) != std::mem::discriminant(&manifest.build.system) {
            return fail("forge.system and build.system disagree");
        }
        if self.inputs.is_empty() || self.inputs.len() > 256 || !source_path(&self.license_file) {
            return fail("explicit safe source inputs and license_file are required");
        }
        let mut inputs = BTreeSet::new();
        for input in &self.inputs {
            if !source_path(input) || !inputs.insert(input) {
                return fail("unsafe/duplicate input or forbidden credential/build path");
            }
        }
        if self.stage.is_empty() || self.stage.len() > 2048 {
            return fail("explicit declarative staging rules are required (including Make)");
        }
        let mut destinations = BTreeSet::new();
        for rule in &self.stage {
            if !safe_path(&rule.from)
                || !["build/", "source/", "dest/"]
                    .iter()
                    .any(|p| rule.from.starts_with(p))
                || !manifest.files.iter().any(|f| f.path == rule.to)
                || !destinations.insert(&rule.to)
            {
                return fail("unsafe/duplicate/unlisted staging rule");
            }
        }
        if manifest
            .files
            .iter()
            .any(|f| !destinations.contains(&f.path))
        {
            return fail("every declared file needs one staging rule");
        }
        match self.system {
            System::Rust if !self.cargo_bin.as_deref().is_some_and(identifier) => {
                return fail("Rust requires an explicit cargo_bin")
            }
            System::Go
                if !self
                    .go_package
                    .as_deref()
                    .is_some_and(|s| s == "." || s.strip_prefix("./").is_some_and(safe_path)) =>
            {
                return fail("Go requires a relative go_package (pure Go only)")
            }
            System::Make if !self.make_target.as_deref().is_some_and(identifier) => {
                return fail("Make requires an explicit make_target and staging rules")
            }
            _ => {}
        }
        if (self.system != System::Rust && self.cargo_bin.is_some())
            || (self.system != System::Go && self.go_package.is_some())
            || (self.system != System::Make && self.make_target.is_some())
        {
            return fail("build-system options do not match the selected adapter");
        }
        Ok(())
    }
}
pub fn unresolved_fields(value: &toml::Value, path: &str, out: &mut Vec<String>) {
    match value {
        toml::Value::String(s) if s.contains("UNRESOLVED") => out.push(path.into()),
        toml::Value::Table(t) => {
            for (k, v) in t {
                unresolved_fields(
                    v,
                    &if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    },
                    out,
                );
            }
        }
        toml::Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                unresolved_fields(v, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

pub fn detect(root: &Path) -> Vec<System> {
    [
        ("Cargo.toml", System::Rust),
        ("CMakeLists.txt", System::Cmake),
        ("go.mod", System::Go),
        ("Makefile", System::Make),
    ]
    .into_iter()
    .filter_map(|(file, system)| root.join(file).is_file().then_some(system))
    .collect()
}
pub fn init(root: &Path, selected: Option<System>) -> Result<Vec<String>> {
    let found = detect(root);
    let system=match selected {
        Some(s) if found.contains(&s)=>s,
        Some(_)=>return fail("selected build system has no matching project file"),
        None if found.len()==1=>found[0].clone(),
        _=>return fail(format!("ambiguous/unsupported project detection: {found:?}; choose --system rust|cmake|go|make explicitly")),
    };
    let mut m = Manifest::parse(include_str!("../../../examples/zpk-hello/zpk.toml"))?;
    m.package.id = "UNRESOLVED".into();
    m.package.author = "UNRESOLVED".into();
    m.package.license = "UNRESOLVED".into();
    m.package.name = "UNRESOLVED".into();
    m.package.description = "UNRESOLVED".into();
    let mut recipe = Recipe {
        version: 1,
        system: system.clone(),
        inputs: vec![],
        license_file: "UNRESOLVED".into(),
        stage: vec![],
        jobs: 2,
        cargo_bin: None,
        go_package: None,
        make_target: None,
    };
    let mut binary = "UNRESOLVED".to_owned();
    match system {
        System::Rust => {
            m.build.system = BuildSystem::Rust;
            let cargo_bytes = crate::read_limited(&root.join("Cargo.toml"), 1024 * 1024)?;
            let cargo: toml::Value = toml::from_str(std::str::from_utf8(&cargo_bytes)?)?;
            if let Some(p) = cargo.get("package") {
                for (field, target) in [
                    ("name", &mut m.package.name),
                    ("description", &mut m.package.description),
                    ("license", &mut m.package.license),
                ] {
                    if let Some(value) = p.get(field).and_then(toml::Value::as_str) {
                        *target = value.into();
                    }
                }
                if let Some(v) = p.get("version").and_then(toml::Value::as_str) {
                    m.package.version = v.parse()?;
                }
                if let Some(a) = p
                    .get("authors")
                    .and_then(toml::Value::as_array)
                    .filter(|a| !a.is_empty())
                {
                    m.package.author = a
                        .iter()
                        .filter_map(toml::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ");
                }
                let mut candidates = BTreeSet::new();
                if let Some(bins) = cargo.get("bin").and_then(toml::Value::as_array) {
                    for bin in bins {
                        if let Some(name) = bin.get("name").and_then(toml::Value::as_str) {
                            candidates.insert(name.to_owned());
                        }
                    }
                }
                if p.get("autobins").and_then(toml::Value::as_bool) != Some(false)
                    && root.join("src/main.rs").is_file()
                    && m.package.name != "UNRESOLVED"
                {
                    candidates.insert(m.package.name.clone());
                }
                if candidates.len() == 1 && !root.join("src/bin").exists() {
                    binary = candidates.into_iter().next().ok_or("missing candidate")?;
                }
            }
            recipe.cargo_bin = Some(binary.clone());
            recipe.inputs = vec!["Cargo.toml".into(), "Cargo.lock".into(), "src".into()];
            recipe.stage.push(Rule {
                from: "build/forge-app".into(),
                to: format!("payload/bin/{binary}"),
            });
        }
        System::Cmake => {
            m.build.system = BuildSystem::Cmake;
            recipe.inputs = vec!["CMakeLists.txt".into(), "UNRESOLVED".into()];
            recipe.stage.push(Rule {
                from: "UNRESOLVED".into(),
                to: "payload/bin/UNRESOLVED".into(),
            });
        }
        System::Go => {
            m.build.system = BuildSystem::Go;
            recipe.inputs = vec!["go.mod".into(), "UNRESOLVED".into()];
            recipe.go_package = Some("UNRESOLVED".into());
            recipe.stage.push(Rule {
                from: "build/forge-app".into(),
                to: "payload/bin/UNRESOLVED".into(),
            });
        }
        System::Make => {
            m.build.system = BuildSystem::Make;
            recipe.inputs = vec!["Makefile".into(), "UNRESOLVED".into()];
            recipe.make_target = Some("UNRESOLVED".into());
            recipe.stage.push(Rule {
                from: "UNRESOLVED".into(),
                to: "payload/bin/UNRESOLVED".into(),
            });
        }
    }
    m.files[0].path = recipe.stage[0].to.clone();
    if system != System::Rust {
        m.files[0].linkage = Some(zpk_schema::manifest::Linkage::MuslDynamic);
        m.package.version = semver::Version::new(0, 1, 0);
    }
    if system == System::Go {
        m.files[0].linkage = Some(zpk_schema::manifest::Linkage::Static);
    }
    m.build.toolchain = "Forge trusted Alpine builder; x86-64 baseline".into();
    for name in ["LICENSE", "COPYING"] {
        if root.join(name).is_file() {
            recipe.license_file = name.into();
            recipe.inputs.push(name.into());
            break;
        }
    }
    let mut table = toml::Value::try_from(m)?;
    table
        .as_table_mut()
        .ok_or("expected TOML table")?
        .insert("forge".into(), toml::Value::try_from(recipe)?);
    let mut unresolved = vec![];
    unresolved_fields(&table, "", &mut unresolved);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("zpk.toml"))?;
    use std::io::Write;
    output.write_all(toml::to_string_pretty(&table)?.as_bytes())?;
    Ok(unresolved)
}
