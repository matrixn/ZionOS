use crate::{
    config::Project,
    elf::{self, Dependencies},
    fail,
    files::{self, MAX_FILE},
    hash, read_limited,
    sandbox::{self, Container},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};
use zpk_schema::{
    manifest::{FileKind, PackageFile},
    Manifest,
};
const MARKER: &str = "ZPK-FORGE-1\n";
#[derive(Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub forge_version: String,
    pub builder_image: String,
    pub builder_source: String,
    pub source_files: BTreeMap<String, String>,
    pub authoring_sha256: String,
    pub resolved_manifest_sha256: String,
    pub toolchains: serde_json::Value,
    pub staged_files: BTreeMap<String, String>,
    pub dependencies: Vec<Dependencies>,
    pub platform_libraries: Vec<String>,
    pub cpu_verification: String,
    pub isolation: String,
}
#[derive(Serialize, Deserialize)]
struct Current {
    generation: String,
}
fn canonical_root(root: &Path) -> Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    if !root.is_dir() {
        return fail("project is not a directory");
    }
    Ok(root)
}
pub fn state(root: &Path, create: bool) -> Result<PathBuf> {
    let project = canonical_root(root)?;
    let path = project.join(".zpk-forge");
    match fs::symlink_metadata(&path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        Ok(_) => return fail("Forge state must be a real directory, never a symlink"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
            fs::create_dir(&path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            }
            fs::write(path.join(".forge-owner"), MARKER)?;
        }
        Err(e) => return Err(e.into()),
    }
    if read_limited(&path.join(".forge-owner"), 64)? != MARKER.as_bytes()
        || fs::canonicalize(&path)?.parent() != Some(project.as_path())
    {
        return fail("unrecognized Forge state; refusing mutation/cleanup");
    }
    Ok(path)
}
fn write_atomic(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("output directory required")?)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    if replace {
        file.persist(path)?;
    } else {
        file.persist_noclobber(path)?;
    }
    Ok(())
}
pub fn build(project: &Path, prepare_builder: bool) -> Result<PathBuf> {
    let project = canonical_root(project)?;
    let config = Project::load(&project)?;
    crate::target::resolve(&config.manifest.target.architecture)?;
    if prepare_builder {
        sandbox::prepare()?;
    }
    let image = sandbox::image_id()?;
    let state = state(&project, true)?;
    let generation = tempfile::Builder::new().prefix("gen-").tempdir_in(&state)?;
    let stage = generation.path().join("stage");
    fs::create_dir(&stage)?;
    let control = serde_json::to_vec(
        &serde_json::json!({"manifest":config.manifest,"recipe":config.recipe}),
    )?;
    let source = files::snapshot(&project, &config.recipe, &control)?;
    let container = Container::create(&image, source.tar)?;
    let toolchains = container.build()?;
    let exported = container.export()?;
    files::read_stage(&exported, &stage)?;
    let mut manifest = config.manifest;
    check_inventory(&stage, &manifest)?;
    let (dependencies, platform_libraries) = collect_libraries(&stage, &mut manifest, &container)?;
    // Validate shared v1 semantics and every content/hash/mode constraint before publishing build state.
    let manifest_text = toml::to_string(&manifest)?;
    let archive = package_bytes(&stage, &manifest)?;
    inspect_package(&archive)?;
    let provenance=Provenance{
        forge_version:env!("CARGO_PKG_VERSION").into(),builder_image:image,builder_source:sandbox::builder_hash(),
        source_files:source.hashes,authoring_sha256:hash(config.authoring.as_bytes()),resolved_manifest_sha256:hash(manifest_text.as_bytes()),
        toolchains,staged_files:files::stage_hashes(&stage)?,dependencies,platform_libraries,
        cpu_verification:elf::CPU_LIMIT.into(),
        isolation:"Disposable non-root Docker build only; network none, root read-only, caps none, no-new-privileges, no host mounts/devices/credentials; CPU/memory/PID/byte/time limits. Shared-kernel isolation is not a VM security boundary.".into(),
    };
    fs::write(generation.path().join("manifest.toml"), manifest_text)?;
    fs::write(
        generation.path().join("provenance.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    let name = generation
        .path()
        .file_name()
        .ok_or("missing generation name")?
        .to_str()
        .ok_or("non-UTF8 generation")?
        .to_owned();
    let kept = generation.keep();
    write_atomic(
        &state.join("current.json"),
        &serde_json::to_vec(&Current { generation: name })?,
        true,
    )?;
    Ok(kept)
}
fn check_inventory(stage: &Path, manifest: &Manifest) -> Result<()> {
    let hashes = files::stage_hashes(stage)?;
    let expected: BTreeSet<_> = manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(["LICENSE"])
        .collect();
    if hashes.len() != expected.len() || hashes.keys().any(|p| !expected.contains(p.as_str())) {
        return fail("staging inventory does not exactly match declared files plus LICENSE");
    }
    Ok(())
}
fn collect_libraries(
    stage: &Path,
    manifest: &mut Manifest,
    container: &Container,
) -> Result<(Vec<Dependencies>, Vec<String>)> {
    let mut queue: VecDeque<_> = manifest
        .files
        .iter()
        .filter(|f| matches!(f.kind, FileKind::Executable | FileKind::Library))
        .map(|f| f.path.clone())
        .collect();
    let mut visited = BTreeSet::new();
    let mut reports = Vec::new();
    let mut platform = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
        if !visited.insert(path.clone()) {
            continue;
        }
        if visited.len() > 64 {
            return fail("transitive ELF library limit (64)");
        }
        let report = elf::inspect(&path, &read_limited(&stage.join(&path), MAX_FILE)?)?;
        for needed in &report.needed {
            if elf::platform_library(needed) {
                platform.insert(needed.clone());
                continue;
            }
            let destination = format!("payload/lib/{needed}");
            if !manifest.files.iter().any(|f| f.path == destination) {
                let bytes = container.library(needed)?;
                elf::inspect(&destination, &bytes)?;
                fs::create_dir_all(stage.join("payload/lib"))?;
                fs::write(stage.join(&destination), bytes)?;
                manifest.files.push(PackageFile {
                    path: destination.clone(),
                    kind: FileKind::Library,
                    linkage: None,
                    permissions: vec![],
                });
            }
            queue.push_back(destination);
        }
        reports.push(report);
    }
    manifest.validate()?;
    Ok((reports, platform.into_iter().collect()))
}
pub fn package_bytes(stage: &Path, manifest: &Manifest) -> Result<Vec<u8>> {
    manifest.validate()?;
    check_inventory(stage, manifest)?;
    let manifest_text = toml::to_string(manifest)?;
    let mut tar = tar::Builder::new(Vec::new());
    let mut hashes = BTreeMap::new();
    files::append(
        &mut tar,
        "manifest.toml",
        manifest_text.as_bytes(),
        0o644,
        0,
    )?;
    hashes.insert("manifest.toml".to_owned(), hash(manifest_text.as_bytes()));
    let mut entries: Vec<_> = manifest
        .files
        .iter()
        .map(|f| {
            (
                f.path.as_str(),
                if f.kind == FileKind::Executable {
                    0o755
                } else {
                    0o644
                },
            )
        })
        .chain([("LICENSE", 0o644)])
        .collect();
    entries.sort();
    let mut total = manifest_text.len() as u64;
    for (path, mode) in entries {
        let data = read_limited(&stage.join(path), MAX_FILE)?;
        total += data.len() as u64;
        if total > files::MAX_TOTAL {
            return fail("staged total exceeds package budget");
        }
        hashes.insert(path.to_owned(), hash(&data));
        files::append(&mut tar, path, &data, mode, 0)?;
    }
    let checksums = serde_json::to_vec(
        &serde_json::json!({"schema_version":1,"algorithm":"sha256","files":hashes}),
    )?;
    files::append(&mut tar, "checksums.json", &checksums, 0o644, 0)?;
    let bytes = zstd::stream::encode_all(Cursor::new(tar.into_inner()?), 3)?;
    zpk_schema::inspect(Cursor::new(&bytes), zpk_schema::Limits::default())?;
    Ok(bytes)
}
#[derive(Debug, Serialize)]
pub struct PackageReport {
    pub package: zpk_schema::Inspection,
    pub dependencies: Vec<Dependencies>,
    pub cpu_verification: &'static str,
}
pub fn inspect_package(bytes: &[u8]) -> Result<PackageReport> {
    let package = zpk_schema::inspect(Cursor::new(bytes), zpk_schema::Limits::default())?;
    let decoder = zstd::stream::read::Decoder::new(Cursor::new(bytes))?;
    let mut tar = tar::Archive::new(decoder);
    let mut reports = vec![];
    use std::io::Read;
    for item in tar.entries()? {
        let item = item?;
        let path = item
            .path()?
            .to_str()
            .ok_or("non-UTF8 package path")?
            .to_owned();
        if package
            .manifest
            .files
            .iter()
            .any(|f| f.path == path && matches!(f.kind, FileKind::Executable | FileKind::Library))
        {
            let mut data = vec![];
            item.take(MAX_FILE + 1).read_to_end(&mut data)?;
            reports.push(elf::inspect(&path, &data)?);
        }
    }
    let bundled: BTreeSet<_> = package
        .manifest
        .files
        .iter()
        .filter(|f| f.kind == FileKind::Library)
        .filter_map(|f| f.path.strip_prefix("payload/lib/"))
        .collect();
    for report in &reports {
        for needed in &report.needed {
            if !elf::platform_library(needed) && !bundled.contains(needed.as_str()) {
                return fail(format!(
                    "unresolved dynamic dependency {needed} in {}",
                    report.file
                ));
            }
        }
        if report.needed.iter().any(|n| !elf::platform_library(n))
            && !report.runpaths.iter().any(|p| {
                p == if report.file.starts_with("payload/bin/") {
                    "$ORIGIN/../lib"
                } else {
                    "$ORIGIN"
                }
            })
        {
            return fail(format!(
                "bundled libraries need a relative loader RUNPATH: {}",
                report.file
            ));
        }
    }
    Ok(PackageReport {
        package,
        dependencies: reports,
        cpu_verification: elf::CPU_LIMIT,
    })
}
pub fn package(project: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let project = canonical_root(project)?;
    let config = Project::load(&project)?;
    let state = state(&project, false)?;
    let current: Current =
        serde_json::from_slice(&read_limited(&state.join("current.json"), 4096)?)?;
    if !current.generation.starts_with("gen-")
        || current.generation.contains('/')
        || !zpk_schema::manifest::safe_path(&current.generation)
    {
        return fail("unsafe build generation");
    }
    let generation = state.join(&current.generation);
    if fs::symlink_metadata(&generation)?.file_type().is_symlink() {
        return fail("build generation symlink forbidden");
    }
    let provenance: Provenance = serde_json::from_slice(&read_limited(
        &generation.join("provenance.json"),
        2 * 1024 * 1024,
    )?)?;
    let text = read_limited(&generation.join("manifest.toml"), 64 * 1024)?;
    if provenance.authoring_sha256 != hash(config.authoring.as_bytes())
        || provenance.resolved_manifest_sha256 != hash(&text)
    {
        return fail("manifest changed since build; rebuild before packaging");
    }
    let stage = generation.join("stage");
    if fs::symlink_metadata(&stage)?.file_type().is_symlink()
        || provenance.staged_files != files::stage_hashes(&stage)?
    {
        return fail("staging changed since build");
    }
    // Recheck source snapshot hashes without invoking Docker or any build scripts.
    let snapshot = files::snapshot(&project, &config.recipe, b"")?;
    if snapshot.hashes != provenance.source_files {
        return fail("source changed since build; rebuild before packaging");
    }
    let manifest = Manifest::parse(std::str::from_utf8(&text)?)?;
    let bytes = package_bytes(&stage, &manifest)?;
    inspect_package(&bytes)?;
    let destination = output.map(Path::to_path_buf).unwrap_or_else(|| {
        state.join(format!(
            "{}-{}.zpk",
            manifest.package.id, manifest.package.version
        ))
    });
    let report_path = destination.with_extension("provenance.json");
    if destination.exists() || report_path.exists() {
        return fail("package/provenance output already exists; choose a new --output path");
    }
    let report = serde_json::json!({"build":provenance,"package_sha256":hash(&bytes),"authenticated":false,"warning":"Unsigned local development package; provenance is an unauthenticated record."});
    write_atomic(&destination, &bytes, false)?;
    write_atomic(&report_path, &serde_json::to_vec_pretty(&report)?, false)?;
    Ok(destination)
}
pub fn clean(project: &Path) -> Result<()> {
    let project = canonical_root(project)?;
    match fs::symlink_metadata(project.join(".zpk-forge")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    let state = state(&project, false)?;
    // Exact canonical, marker-owned directory under this project; never arbitrary source/build targets.
    fs::remove_dir_all(state)?;
    Ok(())
}
