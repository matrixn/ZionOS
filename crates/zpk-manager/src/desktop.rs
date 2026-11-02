//! Public read-only projection. The unprivileged HTTP service never opens SQLite.
use crate::{
    digest, fail,
    filesystem::{self, Root},
    model::{Operation, Record},
    Result,
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use zpk_runtime::installed;
use zpk_schema::{
    desktop::{self, Asset, Entry, Registry},
    manifest::FileKind,
    Manifest,
};
pub fn directory(root: &Root) -> PathBuf {
    root.join("run/zionos/desktop")
}
pub fn invalidate(root: &Root) -> Result<()> {
    let dir = directory(root);
    root.mkdir(&dir, 0o755)?;
    let ready = dir.join("ready");
    if fs::symlink_metadata(&ready).is_ok() {
        root.check(&ready)?;
        fs::remove_file(ready)?;
        filesystem::sync(&dir)?;
    }
    Ok(())
}
pub fn invalidate_for(root: &Root, operation: &Operation) -> Result<()> {
    for r in [&operation.old, &operation.new].into_iter().flatten() {
        if !r.current().parsed()?.desktop.is_empty() {
            return invalidate(root);
        }
    }
    Ok(())
}
pub fn clean(root: &Root) -> bool {
    let dir = directory(root);
    let check = (|| -> Result<bool> {
        root.check(&dir)?;
        let ready = installed::read_file(&dir.join("ready"), 64)?;
        let bytes = installed::read_file(&dir.join("registry.json"), desktop::LIMIT as u64)?;
        root.check(&dir.join("ready"))?;
        root.check(&dir.join("registry.json"))?;
        Ok(ready == digest(&bytes).as_bytes() && Registry::parse(&bytes).is_ok())
    })();
    check.unwrap_or(false)
}
pub fn entries(m: &Manifest, path: &Path, manifest_hash: &str) -> Result<Vec<Entry>> {
    if m.desktop.is_empty() {
        return Ok(vec![]);
    }
    let mut assets = BTreeMap::new();
    for f in &m.files {
        let Some(mime) = desktop::mime(&f.path, &f.kind) else {
            continue;
        };
        let bytes = installed::read_file(&path.join(&f.path), desktop::MAX_ASSET)?;
        let mut content = f.clone();
        if mime == "image/png" {
            content.kind = FileKind::Icon;
        }
        zpk_schema::archive::validate_file_content(&content, &bytes)?;
        if (mime.starts_with("text/") || mime.starts_with("application/javascript"))
            && std::str::from_utf8(&bytes)?.contains('\0')
        {
            return fail("desktop text asset contains NUL");
        }
        assets.insert(
            f.path.clone(),
            Asset {
                size: bytes.len() as u64,
                sha256: digest(&bytes),
                mime: mime.into(),
            },
        );
    }
    Ok(Entry::from_manifest(m, assets, manifest_hash.into())?)
}
fn public_file(root: &Root, path: &Path, bytes: &[u8], tag: &str) -> Result<()> {
    filesystem::atomic(root, path, bytes, tag)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))?;
    filesystem::sync(path)?;
    filesystem::sync(path.parent().ok_or("missing projection parent")?)
}
pub fn publish(root: &Root, records: &BTreeMap<String, Record>) -> Result<()> {
    invalidate(root)?;
    let mut apps = vec![];
    for r in records.values().filter(|r| r.desktop_enabled) {
        let version = r.current();
        let manifest = version.parsed()?;
        if manifest.desktop.is_empty() {
            continue;
        }
        let p = installed::load(&root.path, &version.configuration)?;
        if digest(&installed::read_file(
            &p.layout.version.join("manifest.toml"),
            65536,
        )?) != version.manifest_sha256
            || digest(&installed::read_file(
                &p.layout.version.join("checksums.json"),
                1024 * 1024,
            )?) != version.inventory_sha256
        {
            return fail("desktop projection snapshot differs from committed registry");
        }
        apps.extend(entries(
            &manifest,
            &p.layout.version,
            &version.manifest_sha256,
        )?);
    }
    let registry = Registry {
        schema_version: 1,
        apps,
    };
    registry.validate()?;
    let bytes = serde_json::to_vec(&registry)?;
    if bytes.len() > desktop::LIMIT {
        return fail("desktop projection exceeds 2 MiB");
    }
    let dir = directory(root);
    public_file(root, &dir.join("registry.json"), &bytes, "desktop")?;
    public_file(root, &dir.join("ready"), digest(&bytes).as_bytes(), "ready")
}
