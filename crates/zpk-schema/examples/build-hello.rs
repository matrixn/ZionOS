//! Developer-only deterministic USTAR writer for the fixed hello example; not an installer.
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{Cursor, Write},
    path::Path,
};
use zpk_schema::{inspect, Limits, Manifest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: build-hello COMPILED_HELLO OUTPUT.zpk".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/zpk-hello");
    let manifest = fs::read(root.join("zpk.toml"))?;
    Manifest::parse(std::str::from_utf8(&manifest)?)?;
    let entries = [
        ("manifest.toml", manifest, 0o644),
        ("LICENSE", fs::read(root.join("LICENSE"))?, 0o644),
        ("payload/bin/hello", fs::read(&args[0])?, 0o755),
    ];
    let mut hashes = BTreeMap::new();
    let mut tar = tar::Builder::new(Vec::new());
    for (path, data, mode) in entries {
        hashes.insert(path, format!("{:x}", Sha256::digest(&data)));
        append(&mut tar, path, &data, mode)?;
    }
    let checksums = serde_json::to_vec(
        &serde_json::json!({"schema_version":1,"algorithm":"sha256","files":hashes}),
    )?;
    append(&mut tar, "checksums.json", &checksums, 0o644)?;
    let archive = zstd::stream::encode_all(Cursor::new(tar.into_inner()?), 3)?;
    let report = inspect(Cursor::new(&archive), Limits::default())?;
    // Refuse to overwrite an existing artifact accidentally.
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    output.write_all(&archive)?;
    println!(
        "Created {} ({} bytes, {}): {}",
        args[1],
        archive.len(),
        report.manifest.package.id,
        report.warning
    );
    Ok(())
}
fn append<W: Write>(
    tar: &mut tar::Builder<W>,
    name: &str,
    data: &[u8],
    mode: u32,
) -> std::io::Result<()> {
    let mut header = tar::Header::new_ustar();
    header.set_size(data.len() as u64);
    header.set_mode(mode);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();
    tar.append_data(&mut header, name, Cursor::new(data))
}
