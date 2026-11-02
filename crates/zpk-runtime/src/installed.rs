use crate::{
    config::{Layout, Plan, RuntimeConfig},
    digest, fail, Result,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};
use zpk_schema::{archive::FileInfo, manifest::FileKind, Manifest};
pub fn read_file(path: &Path, max: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max {
        return fail(format!("not a bounded regular file: {}", path.display()));
    }
    let mut file = fs::OpenOptions::new();
    file.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut bytes = vec![];
    file.open(path)?.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return fail("file limit exceeded");
    }
    Ok(bytes)
}
pub fn secure_path(root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(root)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return fail("noncanonical path");
        }
        current.push(component);
        let m = fs::symlink_metadata(&current)?;
        if m.file_type().is_symlink() {
            return fail(format!(
                "installed symlink is forbidden: {}",
                current.display()
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if m.mode() & 0o022 != 0 || (root == Path::new("/") && m.uid() != 0) {
                return fail(
                    "installed files/ancestors must be root-owned and not group/world writable",
                );
            }
        }
    }
    Ok(())
}
pub fn load(root: &Path, config: &RuntimeConfig) -> Result<Plan> {
    let layout = Layout::new(root, config)?;
    secure_path(root, &layout.app.join("versions"))?;
    if fs::read_link(&layout.current)?
        != std::path::PathBuf::from("versions").join(config.version.to_string())
    {
        return fail("current must reference exactly the configured version within versions/");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if root == Path::new("/") && fs::symlink_metadata(&layout.current)?.uid() != 0 {
            return fail("current reference must be root-owned");
        }
    }
    secure_path(root, &layout.version)?;
    let source = read_file(&layout.version.join("manifest.toml"), 64 * 1024)?;
    let manifest = Manifest::parse(std::str::from_utf8(&source)?)?;
    let plan = config.clone().plan(manifest, root)?;
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    inventory(
        root,
        &layout.version,
        &layout.version,
        &mut files,
        &mut total,
    )?;
    let expected: BTreeSet<_> = plan
        .manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(["manifest.toml", "LICENSE", "checksums.json"])
        .collect();
    if files.len() != expected.len() || files.keys().any(|p| !expected.contains(p.as_str())) {
        return fail("installed inventory mismatch");
    }
    files.remove("checksums.json");
    zpk_schema::archive::verify_checksums(
        &read_file(&layout.version.join("checksums.json"), 1024 * 1024)?,
        &files,
    )?;
    if std::str::from_utf8(&read_file(&layout.version.join("LICENSE"), 64 * 1024)?)?
        .trim()
        .is_empty()
    {
        return fail("empty installed LICENSE");
    }
    let mut reports = vec![];
    let mut libraries = BTreeSet::new();
    for file in &plan.manifest.files {
        let data = read_file(&layout.version.join(&file.path), 64 * 1024 * 1024)?;
        zpk_schema::archive::validate_file_content(file, &data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(layout.version.join(&file.path))?
                .permissions()
                .mode()
                & 0o7777;
            if mode
                != if file.kind == FileKind::Executable {
                    0o755
                } else {
                    0o644
                }
            {
                return fail("installed file mode mismatch");
            }
        }
        if matches!(file.kind, FileKind::Executable | FileKind::Library) {
            reports.push(zpk_common::elf::inspect(&file.path, &data)?);
        }
        if file.kind == FileKind::Library {
            libraries.insert(
                file.path
                    .strip_prefix("payload/lib/")
                    .ok_or("library layout")?
                    .to_owned(),
            );
        }
    }
    for report in reports {
        for name in &report.needed {
            if !zpk_common::elf::platform_library(name) && !libraries.contains(name) {
                return fail(format!("missing runtime ELF dependency: {name}"));
            }
            if !zpk_common::elf::platform_library(name)
                && !report.runpaths.iter().any(|p| {
                    p == if report.file.starts_with("payload/bin/") {
                        "$ORIGIN/../lib"
                    } else {
                        "$ORIGIN"
                    }
                })
            {
                return fail("missing relative runtime library RUNPATH");
            }
        }
    }
    let release = read_file(&root.join("etc/zionos-release"), 4096)?;
    let os = std::str::from_utf8(&release)?
        .split_whitespace()
        .find_map(|v| semver::Version::parse(v).ok())
        .ok_or("missing ZionOS version")?;
    plan.manifest.check_compatibility(&os, None)?;
    for dependency in &plan.manifest.runtime.dependencies {
        let app = root.join("opt/zionos/apps").join(&dependency.id);
        secure_path(root, &app.join("versions"))?;
        let link = fs::read_link(app.join("current"))?;
        let mut components = link.components();
        if components.next()
            != Some(std::path::Component::Normal(std::ffi::OsStr::new(
                "versions",
            )))
            || components.clone().count() != 1
        {
            return fail("invalid dependency current reference");
        }
        let version = components
            .next()
            .ok_or("missing dependency version")?
            .as_os_str()
            .to_str()
            .ok_or("dependency version encoding")?
            .parse::<semver::Version>()?;
        if !dependency.version.matches(&version) {
            return fail(format!(
                "runtime dependency version mismatch: {}",
                dependency.id
            ));
        }
        let directory = app.join("versions").join(version.to_string());
        secure_path(root, &directory)?;
        let other = Manifest::parse(std::str::from_utf8(&read_file(
            &directory.join("manifest.toml"),
            64 * 1024,
        )?)?)?;
        if other.package.id != dependency.id || other.package.version != version {
            return fail("dependency identity mismatch");
        }
    }
    Ok(plan)
}
fn inventory(
    root: &Path,
    base: &Path,
    path: &Path,
    out: &mut BTreeMap<String, FileInfo>,
    total: &mut u64,
) -> Result<()> {
    let entries = fs::read_dir(path)?
        .take(4097)
        .collect::<std::io::Result<Vec<_>>>()?;
    if entries.len() > 4096 {
        return fail("installed directory limit");
    }
    for entry in entries {
        secure_path(root, &entry.path())?;
        let m = fs::symlink_metadata(entry.path())?;
        if m.is_dir() {
            inventory(root, base, &entry.path(), out, total)?;
        } else {
            if out.len() >= 4096 {
                return fail("installed file count limit");
            }
            let name = entry
                .path()
                .strip_prefix(base)?
                .to_str()
                .ok_or("installed filename encoding")?
                .replace('\\', "/");
            if !zpk_schema::manifest::safe_path(&name) {
                return fail("unsafe installed filename");
            }
            let data = read_file(&entry.path(), 64 * 1024 * 1024)?;
            *total += data.len() as u64;
            if *total > 128 * 1024 * 1024 {
                return fail("installed byte budget");
            }
            out.insert(
                name,
                FileInfo {
                    size: data.len() as u64,
                    sha256: digest(&data),
                },
            );
        }
    }
    Ok(())
}
