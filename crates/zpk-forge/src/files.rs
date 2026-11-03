use crate::{
    config::{source_path, Recipe},
    fail, hash, read_limited, Result,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};
use zpk_schema::manifest::safe_path;
pub const MAX_FILE: u64 = 64 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 128 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 4096;
pub fn append<W: Write>(
    tar: &mut tar::Builder<W>,
    path: &str,
    data: &[u8],
    mode: u32,
    uid: u64,
) -> Result<()> {
    let mut header = tar::Header::new_ustar();
    header.set_size(data.len() as u64);
    header.set_mode(mode);
    header.set_uid(uid);
    header.set_gid(uid);
    header.set_mtime(0);
    header.set_cksum();
    tar.append_data(&mut header, path, Cursor::new(data))?;
    Ok(())
}
pub struct Snapshot {
    pub tar: Vec<u8>,
    pub hashes: BTreeMap<String, String>,
}
pub fn snapshot(root: &Path, recipe: &Recipe, control: &[u8]) -> Result<Snapshot> {
    let mut paths = BTreeMap::new();
    let mut total = 0u64;
    let mut visited = 0usize;
    for input in &recipe.inputs {
        walk(root, input, &mut paths, &mut total, &mut visited)?;
    }
    if !paths.contains_key(&recipe.license_file) {
        return fail("license_file must be included in source inputs");
    }
    if recipe.system == crate::config::System::Rust && !paths.contains_key("Cargo.lock") {
        return fail(
            "Rust requires a supplied Cargo.lock; vendor dependencies before building offline",
        );
    }
    let mut hashes = BTreeMap::new();
    let mut tar = tar::Builder::new(Vec::new());
    for (path, (data, mode)) in paths {
        let mut fingerprint = mode.to_le_bytes().to_vec();
        fingerprint.extend_from_slice(&data);
        hashes.insert(path.clone(), hash(&fingerprint));
        append(&mut tar, &format!("source/{path}"), &data, mode, 10000)?;
    }
    append(&mut tar, "control.json", control, 0o444, 10000)?;
    Ok(Snapshot {
        tar: tar.into_inner()?,
        hashes,
    })
}
fn walk(
    root: &Path,
    path: &str,
    out: &mut BTreeMap<String, (Vec<u8>, u32)>,
    total: &mut u64,
    visited: &mut usize,
) -> Result<()> {
    if !source_path(path) {
        return fail(format!(
            "forbidden credential/output or unsafe source path: {path:?}"
        ));
    }
    *visited += 1;
    if *visited > MAX_ENTRIES {
        return fail("source entry count limit (including directories)");
    }
    let mut prefix = root.to_path_buf();
    for component in path.split('/') {
        prefix.push(component);
        if fs::symlink_metadata(&prefix)?.file_type().is_symlink() {
            return fail("source symlink ancestor forbidden");
        }
    }
    let full = root.join(path);
    let metadata = fs::symlink_metadata(&full)?;
    if metadata.file_type().is_symlink() {
        return fail("source symlinks are unsupported; no host link is followed");
    }
    if metadata.is_dir() {
        let mut children = fs::read_dir(full)?
            .take(MAX_ENTRIES + 1)
            .collect::<std::io::Result<Vec<_>>>()?;
        if children.len() > MAX_ENTRIES {
            return fail("source directory entry limit");
        }
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 source name")?;
            walk(root, &format!("{path}/{name}"), out, total, visited)?;
        }
    } else if metadata.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() > 1 {
                return fail("source hardlinks are unsupported");
            }
        }
        if out.len() >= MAX_ENTRIES || out.contains_key(path) {
            return fail("duplicate/overlapping source inputs or source file count limit");
        }
        let bytes = read_limited(&full, MAX_FILE)?;
        *total = total
            .checked_add(bytes.len() as u64)
            .ok_or("source size overflow")?;
        if *total > MAX_TOTAL {
            return fail("source snapshot exceeds 128 MiB");
        }
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 != 0 {
                0o755
            } else {
                0o644
            }
        };
        #[cfg(not(unix))]
        let mode = 0o644;
        out.insert(path.into(), (bytes, mode));
    } else {
        return fail("non-regular source input");
    }
    Ok(())
}
/// Read a builder's untrusted staging tar without invoking tar extraction.
pub fn read_stage(bytes: &[u8], destination: &Path) -> Result<BTreeMap<String, String>> {
    if bytes.len() as u64 > MAX_TOTAL {
        return fail("staging archive exceeds 128 MiB");
    }
    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let mut entries = archive.entries()?.raw(true);
    let mut hashes = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for (count, item) in (&mut entries).enumerate() {
        if count >= MAX_ENTRIES {
            return fail("staging entry count limit");
        }
        let item = item?;
        let raw = item.path_bytes();
        let text = std::str::from_utf8(&raw)?;
        let path = text
            .strip_prefix("./")
            .unwrap_or(text)
            .trim_end_matches('/');
        if (path.is_empty() || path == ".") && item.header().entry_type().is_dir() {
            continue;
        }
        if !safe_path(path) || !seen.insert(path.to_owned()) || seen.len() > MAX_ENTRIES {
            return fail("unsafe/duplicate staging path or staging count limit");
        }
        let path = path.to_owned();
        let ty = item.header().entry_type();
        if ty.is_dir() {
            continue;
        }
        if !ty.is_file() || item.header().link_name()?.is_some() {
            return fail("staging links, special files and tar extensions are forbidden");
        }
        let size = item.header().size()?;
        if size > MAX_FILE || total.checked_add(size).is_none_or(|v| v > MAX_TOTAL) {
            return fail("oversized staging file/total");
        }
        total += size;
        let mut data = Vec::new();
        item.take(MAX_FILE + 1).read_to_end(&mut data)?;
        if data.len() as u64 != size {
            return fail("truncated staging file");
        }
        let output = destination.join(&path);
        // All directory creation is derived from canonical paths in this new private temp directory.
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output)?;
        file.write_all(&data)?;
        hashes.insert(path, hash(&data));
    }
    Ok(hashes)
}
pub fn stage_hashes(stage: &Path) -> Result<BTreeMap<String, String>> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                return fail("staged symlink forbidden");
            }
            if metadata.is_dir() {
                visit(root, &entry.path(), out)?;
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF8 stage path")?
                    .replace('\\', "/");
                if !safe_path(&relative) || out.len() >= MAX_ENTRIES {
                    return fail("unsafe/oversized staging tree");
                }
                out.insert(relative, hash(&read_limited(&entry.path(), MAX_FILE)?));
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(stage, stage, &mut out)?;
    Ok(out)
}
