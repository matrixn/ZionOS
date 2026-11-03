//! Malicious archives are constructed in memory, never extracted or executed.
use semver::Version;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::Path,
    process::Command,
};
use zpk_schema::{
    inspect,
    manifest::{FileKind, Linkage, Permission},
    Error, Limits, Manifest,
};

const HELLO: &str = include_str!("../../examples/zpk-hello/zpk.toml");
const DESKTOP: &str = include_str!("fixtures/valid/desktop.toml");
fn manifest() -> Manifest {
    Manifest::parse(HELLO).unwrap()
}
fn fixture_elf() -> Vec<u8> {
    let mut out = vec![0u8; 128];
    out[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    out[16..18].copy_from_slice(&2u16.to_le_bytes());
    out[18..20].copy_from_slice(&62u16.to_le_bytes());
    out[20] = 1;
    out[24..32].copy_from_slice(&0x400078u64.to_le_bytes());
    out[32..40].copy_from_slice(&64u64.to_le_bytes());
    out[52..54].copy_from_slice(&64u16.to_le_bytes());
    out[54..56].copy_from_slice(&56u16.to_le_bytes());
    out[56] = 1;
    out[64] = 1;
    out[68] = 5;
    out[80..88].copy_from_slice(&0x400000u64.to_le_bytes());
    out[96..104].copy_from_slice(&128u64.to_le_bytes());
    out[104..112].copy_from_slice(&128u64.to_le_bytes());
    out[120] = 0xc3;
    out
}
fn png_icon() -> Vec<u8> {
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0, 180, 140, 255])
            .unwrap();
    }
    data
}
#[derive(Clone)]
struct Entry {
    path: String,
    data: Vec<u8>,
    kind: u8,
    mode: u32,
    link: String,
}
fn entry(path: &str, data: Vec<u8>, mode: u32) -> Entry {
    Entry {
        path: path.into(),
        data,
        kind: b'0',
        mode,
        link: String::new(),
    }
}
fn entries(m: &Manifest) -> Vec<Entry> {
    let mut out = vec![
        entry(
            "manifest.toml",
            toml::to_string(m).unwrap().into_bytes(),
            0o644,
        ),
        entry("LICENSE", b"MIT fixture license\n".to_vec(), 0o644),
    ];
    for file in &m.files {
        let data = match file.kind {
            FileKind::Executable => fixture_elf(),
            FileKind::Library => {
                let mut data = fixture_elf();
                data[16] = 3;
                data
            }
            FileKind::Ui => b"<!doctype html><html><body>Hello</body></html>".to_vec(),
            FileKind::Icon => png_icon(),
            FileKind::Asset => b"asset".to_vec(),
        };
        out.push(entry(
            &file.path,
            data,
            if file.kind == FileKind::Executable {
                0o755
            } else {
                0o644
            },
        ));
    }
    checksums(&mut out);
    out
}
fn checksums(entries: &mut Vec<Entry>) {
    entries.retain(|e| e.path != "checksums.json");
    let hashes: BTreeMap<_, _> = entries
        .iter()
        .filter(|e| e.kind == b'0')
        .map(|e| (&e.path, format!("{:x}", Sha256::digest(&e.data))))
        .collect();
    let data = serde_json::to_vec(
        &serde_json::json!({"schema_version":1,"algorithm":"sha256","files":hashes}),
    )
    .unwrap();
    entries.push(entry("checksums.json", data, 0o644));
}
fn raw(entries: &[Entry]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for e in entries {
        let mut header = tar::Header::new_ustar();
        header.set_size(e.data.len() as u64);
        header.set_mode(e.mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::new(e.kind));
        // Directly populate name bytes to exercise traversal paths rejected by normal tar writers.
        assert!(e.path.len() < 100);
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..e.path.len()].copy_from_slice(e.path.as_bytes());
        if !e.link.is_empty() {
            header.set_link_name(&e.link).unwrap();
        }
        header.set_cksum();
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(&e.data);
        bytes.resize(bytes.len() + (512 - e.data.len() % 512) % 512, 0);
    }
    bytes.resize(bytes.len() + 1024, 0);
    bytes
}
fn compress(bytes: &[u8]) -> Vec<u8> {
    zstd::stream::encode_all(Cursor::new(bytes), 3).unwrap()
}
fn inspect_entries(entries: &[Entry]) -> zpk_schema::Result<zpk_schema::Inspection> {
    inspect(Cursor::new(compress(&raw(entries))), Limits::default())
}
fn reject(entries: &[Entry], message: &str) {
    let error = inspect_entries(entries).unwrap_err().to_string();
    assert!(error.contains(message), "expected {message}, got {error}");
}
fn fix_header_checksum(header: &mut [u8]) {
    header[148..156].fill(b' ');
    let sum: u64 = header.iter().map(|b| u64::from(*b)).sum();
    let value = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(value.as_bytes());
}

#[test]
fn valid_manifest_modes_roundtrip_and_packages() {
    for source in [
        HELLO,
        DESKTOP,
        include_str!("fixtures/valid/service.toml"),
        include_str!("fixtures/valid/multi-desktop.toml"),
    ] {
        let m = Manifest::parse(source).unwrap();
        Manifest::parse(&toml::to_string(&m).unwrap()).unwrap();
        let report = inspect_entries(&entries(&m)).unwrap();
        assert!(!report.authenticated);
        assert!(report.warning.contains("not publisher identity"));
        assert_eq!(report.files.len(), m.files.len() + 2);
    }
}
#[test]
fn invalid_manifest_fixtures_are_rejected_with_explicit_target_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/zpk/fixtures/invalid");
    for path in fs::read_dir(root).unwrap() {
        let path = path.unwrap().path();
        let error = Manifest::parse(&fs::read_to_string(&path).unwrap()).unwrap_err();
        if path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .starts_with("unsupported-")
        {
            assert!(matches!(error, Error::UnsupportedTarget(_)));
        }
        if path.file_stem().unwrap() == "unknown-schema" {
            assert!(matches!(error, Error::UnsupportedSchema(2)));
        }
    }
}
#[test]
fn package_identifiers_and_semver_are_strict() {
    for id in [
        "hello",
        "org.hello",
        "Org.zion.hello",
        "org..hello",
        "org.zion.-hello",
        "org.zion.hello-",
        "org.zion.hello_world",
        "org.zion.hello/other",
        "org.zion.é",
    ] {
        assert!(
            Manifest::parse(&HELLO.replace("org.zionos.hello", id)).is_err(),
            "{id}"
        );
    }
    for version in ["1", "1.0", "01.0.0", "v1.0.0", "1.0.0-", "1.0.0.0"] {
        assert!(Manifest::parse(
            &HELLO.replace("version = \"1.0.0\"", &format!("version = \"{version}\""))
        )
        .is_err());
    }
    Manifest::parse(&HELLO.replace("version = \"1.0.0\"", "version = \"1.0.0-beta.1+build.3\""))
        .unwrap();
}
#[test]
fn path_aliases_and_ancestor_collisions_are_rejected() {
    assert!(zpk_schema::manifest::safe_path(
        "payload/lib/libstdc++.so.6"
    ));
    for path in [
        "/payload/bin/hello",
        "../hello",
        "payload/../hello",
        "payload//bin/hello",
        "./payload/bin/hello",
        "payload/bin/hello/",
        "C:/hello",
        r"payload\bin\hello",
        "payload/bin/hi\n",
        "payload/bin/é",
    ] {
        let mut m = manifest();
        m.files[0].path = path.into();
        assert!(m.validate().is_err(), "{path}");
    }
    let mut m = manifest();
    let mut nested = m.files[0].clone();
    nested.path.push_str("/nested");
    m.files.push(nested);
    assert!(m.validate().is_err());
    m.files.reverse();
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.files.push(m.files[0].clone());
    assert!(m.validate().is_err());
}
#[test]
fn metadata_unknown_fields_duplicates_and_resource_bounds() {
    for source in [
        HELLO.replace("[target]", "[target]\nunknown = true"),
        HELLO.replace("[build]", "[build]\nunknown = true"),
        HELLO.replace(
            "schema_version = 1",
            "schema_version = 1\nschema_version = 1",
        ),
        HELLO.replace("[runtime]", "[runtime]\nunknown = true"),
    ] {
        assert!(Manifest::parse(&source).is_err());
    }
    assert!(Manifest::parse(&" ".repeat(65537)).is_err());
    let mut m = manifest();
    m.package.name = "\n".into();
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.files[0].linkage = None;
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.files[0].kind = FileKind::Asset;
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.files.clear();
    assert!(m.validate().is_err());
}
#[test]
fn permission_and_feature_reference_checks() {
    let mut m = manifest();
    m.files[0].permissions.push(Permission::NetworkClient);
    assert!(m.validate().is_err());
    m.permissions.push(Permission::NetworkClient);
    m.validate().unwrap();
    m.permissions.push(Permission::NetworkClient);
    assert!(m.validate().is_err());
    for field in ["entry", "icon", "executable"] {
        let mut m = Manifest::parse(DESKTOP).unwrap();
        match field {
            "entry" => m.desktop[0].entry = "https://example.org/app.html".into(),
            "icon" => m.desktop[0].icon = "ui/index.html".into(),
            _ => m.desktop[0].executable = Some("payload/bin/missing".into()),
        };
        assert!(m.validate().is_err());
    }
    let mut m = Manifest::parse(DESKTOP).unwrap();
    m.desktop[0].window.width = 10;
    assert!(m.validate().is_err());
    let mut m = Manifest::parse(DESKTOP).unwrap();
    m.desktop.push(m.desktop[0].clone());
    assert!(m.validate().is_err());
}
#[test]
fn service_storage_and_dependency_requirements() {
    let service = include_str!("fixtures/valid/service.toml");
    for bad in [
        service.replace("run_as = \"package\"", "run_as = \"root\""),
        service.replace("max_bytes = 65536", "max_bytes = 0"),
        service.replace("name = \"state\"", "name = \"../state\""),
        service.replace("schema = 1", "schema = 0"),
        service.replace("\"configuration-write\", ", ""),
        service.replace("\"background-service\", ", ""),
    ] {
        assert!(Manifest::parse(&bad).is_err());
    }
    let mut m = manifest();
    m.runtime.dependencies = vec![zpk_schema::manifest::Dependency {
        id: "org.zionos.library".into(),
        version: "^1.0".parse().unwrap(),
    }];
    m.validate().unwrap();
    m.compatibility.conflicts.push("org.zionos.library".into());
    assert!(m.validate().is_err());
    m.compatibility.conflicts.clear();
    m.runtime.dependencies[0].id = m.package.id.clone();
    assert!(m.validate().is_err());
    assert!(Manifest::parse(&HELLO.replace(
        "dependencies = []",
        "dependencies = [{ id = 'org.zionos.library', version = 'not-semver' }] "
    ))
    .is_err());
}
#[test]
fn installation_and_upgrade_compatibility_are_explicit() {
    let m = manifest();
    m.check_compatibility(&Version::new(0, 6, 1), None).unwrap();
    assert!(m.check_compatibility(&Version::new(0, 6, 0), None).is_err());
    m.check_compatibility(&Version::new(0, 6, 1), Some(&Version::new(0, 9, 0)))
        .unwrap();
    for previous in [
        Version::new(0, 0, 1),
        Version::new(1, 0, 0),
        Version::new(2, 0, 0),
    ] {
        assert!(m
            .check_compatibility(&Version::new(0, 7, 0), Some(&previous))
            .is_err());
    }
}
#[test]
fn archive_paths_traversal_absolute_windows_and_duplicates() {
    for path in [
        "../escape",
        "/etc/passwd",
        "payload/bin/../hello",
        "payload//bin/hello",
        "C:/escape",
        r"payload\bin\hello",
        "./manifest.toml",
    ] {
        let mut e = entries(&manifest());
        e[2].path = path.into();
        reject(&e, "unsafe/noncanonical");
    }
    let mut e = entries(&manifest());
    e.insert(3, e[2].clone());
    reject(&e, "duplicate archive path");
    let mut e = entries(&manifest());
    e[2].path = "assets/undeclared".into();
    reject(&e, "undeclared file");
    let mut e = entries(&manifest());
    e.remove(2);
    checksums(&mut e);
    reject(&e, "missing LICENSE or declared");
}
#[test]
fn all_links_special_files_extensions_and_privileged_modes_are_denied() {
    for kind in *b"12346xgLKS" {
        let mut e = entries(&manifest());
        e[2].kind = kind;
        e[2].link = "../../outside".into();
        reject(&e, "forbidden");
    }
    for mode in [0o4755, 0o2755, 0o777, 0o666, 0o600] {
        let mut e = entries(&manifest());
        e[2].mode = mode;
        reject(&e, "mode");
    }
    let mut e = entries(&manifest());
    e[2].link = "assets/other".into();
    reject(&e, "link targets");
}
#[test]
fn canonical_optional_directories_and_prefixes() {
    let mut e = entries(&manifest());
    let mut dir = entry("payload/bin/", Vec::new(), 0o755);
    dir.kind = b'5';
    e.insert(2, dir.clone());
    inspect_entries(&e).unwrap();
    e.insert(3, dir);
    reject(&e, "duplicate archive path");
    let mut e = entries(&manifest());
    let mut dir = entry("outside/", Vec::new(), 0o755);
    dir.kind = b'5';
    e.insert(2, dir);
    reject(&e, "directory outside");
    let mut e = entries(&manifest());
    e[2].kind = b'5';
    e[2].data.clear();
    reject(&e, "directory outside");
    // USTAR prefix/name spelling is accepted if its combined path is canonical.
    let mut bytes = raw(&entries(&manifest()));
    let at = (0..bytes.len() - 512)
        .step_by(512)
        .find(|at| &bytes[*at..*at + 17] == b"payload/bin/hello")
        .unwrap();
    let h = &mut bytes[at..at + 512];
    h[..100].fill(0);
    h[..5].copy_from_slice(b"hello");
    h[345..356].copy_from_slice(b"payload/bin");
    fix_header_checksum(h);
    inspect(Cursor::new(compress(&bytes)), Limits::default()).unwrap();
}
#[test]
fn header_checksums_ownership_magic_and_numeric_overflow() {
    let original = raw(&entries(&manifest()));
    let mut bytes = original.clone();
    bytes[1] ^= 1;
    assert!(inspect(Cursor::new(compress(&bytes)), Limits::default())
        .unwrap_err()
        .to_string()
        .contains("header checksum"));
    for (range, value) in [
        (108..116, b"0000001\0".as_slice()),
        (257..263, b"ustar ".as_slice()),
        (124..136, b"\x80\0\0\0\0\0\0\0\0\0\0\0".as_slice()),
    ] {
        let mut bytes = original.clone();
        bytes[range].copy_from_slice(value);
        fix_header_checksum(&mut bytes[..512]);
        assert!(inspect(Cursor::new(compress(&bytes)), Limits::default()).is_err());
    }
}
#[test]
fn hash_mismatch_checksum_inventory_and_duplicate_json_keys() {
    let mut e = entries(&manifest());
    e[1].data.push(b'!');
    reject(&e, "SHA-256");
    let mut e = entries(&manifest());
    e.last_mut().unwrap().data = b"{broken".to_vec();
    reject(&e, "checksums.json");
    for change in [
        "remove",
        "extra",
        "self",
        "wrong-algorithm",
        "unknown-schema",
        "unknown-field",
    ] {
        let mut e = entries(&manifest());
        let mut value: serde_json::Value = serde_json::from_slice(&e.last().unwrap().data).unwrap();
        match change {
            "remove" => {
                value["files"].as_object_mut().unwrap().remove("LICENSE");
            }
            "extra" => value["files"]["assets/unknown"] = serde_json::json!("0".repeat(64)),
            "self" => value["files"]["checksums.json"] = serde_json::json!("0".repeat(64)),
            "wrong-algorithm" => value["algorithm"] = serde_json::json!("sha1"),
            "unknown-schema" => value["schema_version"] = serde_json::json!(2),
            _ => value["unknown"] = serde_json::json!(true),
        };
        e.last_mut().unwrap().data = serde_json::to_vec(&value).unwrap();
        assert!(inspect_entries(&e).is_err());
    }
    for hashes in [
        format!(
            "\"LICENSE\":\"{}\",\"LICENSE\":\"{}\"",
            "0".repeat(64),
            "1".repeat(64)
        ),
        "\"../outside\":\"abc\"".into(),
        format!("\"LICENSE\":\"{}\"", "A".repeat(64)),
    ] {
        let mut e = entries(&manifest());
        e.last_mut().unwrap().data =
            format!("{{\"schema_version\":1,\"algorithm\":\"sha256\",\"files\":{{{hashes}}}}}")
                .into_bytes();
        reject(&e, "checksums.json");
    }
}
#[test]
fn manifest_order_checksums_last_end_blocks_and_padding() {
    let mut e = entries(&manifest());
    e.swap(0, 1);
    reject(&e, "first entry");
    let mut e = entries(&manifest());
    e.swap(2, 3);
    reject(&e, "last archive entry");
    let e = entries(&manifest());
    let bytes = raw(&e);
    for bad in [
        bytes[..bytes.len() - 512].to_vec(),
        {
            let mut b = bytes.clone();
            *b.last_mut().unwrap() = 1;
            b
        },
        {
            let mut b = bytes.clone();
            b.push(0);
            b
        },
        {
            let mut b = bytes.clone();
            b.extend_from_slice(b"junk");
            b
        },
    ] {
        assert!(inspect(Cursor::new(compress(&bad)), Limits::default()).is_err());
    }
    let mut padding = bytes;
    let length = e[0].data.len();
    padding[512 + length] = 1;
    assert!(inspect(Cursor::new(compress(&padding)), Limits::default()).is_err());
}
#[test]
fn truncated_malformed_concatenated_frames_and_zstd_window_limit() {
    let bytes = compress(&raw(&entries(&manifest())));
    for bad in [
        Vec::new(),
        b"not zstd".to_vec(),
        bytes[..bytes.len() - 1].to_vec(),
        {
            let mut b = bytes.clone();
            b.extend_from_slice(&bytes);
            b
        },
        {
            let mut b = bytes.clone();
            b.extend_from_slice(b"junk");
            b
        },
    ] {
        assert!(inspect(Cursor::new(bad), Limits::default()).is_err());
    }
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
    encoder.window_log(24).unwrap();
    use std::io::Write;
    encoder.write_all(&raw(&entries(&manifest()))).unwrap();
    assert!(inspect(Cursor::new(encoder.finish().unwrap()), Limits::default()).is_err());
}
#[test]
fn expansion_compressed_file_and_entry_limits() {
    let bytes = compress(&raw(&entries(&manifest())));
    for limits in [
        Limits {
            compressed_bytes: bytes.len() as u64 - 1,
            ..Limits::default()
        },
        Limits {
            expanded_bytes: 1024,
            ..Limits::default()
        },
        Limits {
            file_bytes: 100,
            ..Limits::default()
        },
        Limits {
            entries: 2,
            ..Limits::default()
        },
        Limits {
            entries: 0,
            ..Limits::default()
        },
    ] {
        assert!(inspect(Cursor::new(&bytes), limits).is_err());
    }
    let mut bomb = raw(&entries(&manifest()));
    bomb[124..136].copy_from_slice(b"77777777777\0");
    fix_header_checksum(&mut bomb[..512]);
    assert!(matches!(
        inspect(Cursor::new(compress(&bomb)), Limits::default()),
        Err(Error::Limit(_))
    ));
    let mut bomb = raw(&entries(&manifest()));
    bomb.resize(bomb.len() + 2 * 1024 * 1024, 0);
    assert!(inspect(
        Cursor::new(compress(&bomb)),
        Limits {
            expanded_bytes: 1024 * 1024,
            ..Limits::default()
        }
    )
    .is_err());
}
#[test]
fn fragmented_reader_is_supported() {
    struct Fragmented(Cursor<Vec<u8>>);
    impl Read for Fragmented {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = out.len().min(1);
            self.0.read(&mut out[..n])
        }
    }
    inspect(
        Fragmented(Cursor::new(compress(&raw(&entries(&manifest()))))),
        Limits::default(),
    )
    .unwrap();
}
#[test]
fn native_elf_machine_headers_and_linkage_are_validated() {
    for bad in [
        b"#!/bin/sh\necho do-not-execute".to_vec(),
        b"\x7fELF".to_vec(),
        {
            let mut b = fixture_elf();
            b[18] = 183;
            b
        },
        {
            let mut b = fixture_elf();
            b[4] = 1;
            b
        },
        {
            let mut b = fixture_elf();
            b[32..40].fill(0xff);
            b
        },
        {
            let mut b = fixture_elf();
            b[56] = 0;
            b
        },
        {
            let mut b = fixture_elf();
            b[68] = 4;
            b
        },
    ] {
        let mut e = entries(&manifest());
        e[2].data = bad;
        checksums(&mut e);
        reject(&e, "ELF");
    }
    let mut m = manifest();
    m.files[0].linkage = Some(Linkage::MuslDynamic);
    reject(&entries(&m), "interpreter/linkage");
    let mut m = manifest();
    let mut library = m.files[0].clone();
    library.path = "payload/lib/libhello.so".into();
    library.kind = FileKind::Library;
    library.linkage = None;
    m.files.push(library);
    inspect_entries(&entries(&m)).unwrap();
}
#[test]
fn invalid_icons_html_and_missing_desktop_assets_are_rejected() {
    let m = Manifest::parse(DESKTOP).unwrap();
    for (name, data) in [
        ("assets/icon.png", b"<svg onload='evil()'/>".to_vec()),
        ("assets/icon.png", {
            let mut b = png_icon();
            b[40] ^= 1;
            b
        }),
        ("ui/index.html", b"not HTML".to_vec()),
        ("ui/index.html", vec![0xff]),
    ] {
        let mut e = entries(&m);
        e.iter_mut().find(|e| e.path == name).unwrap().data = data;
        checksums(&mut e);
        assert!(inspect_entries(&e).is_err());
    }
    let mut e = entries(&m);
    e.retain(|e| e.path != "assets/icon.png");
    checksums(&mut e);
    reject(&e, "missing LICENSE or declared");
}
#[test]
fn inspector_cli_reports_unsigned_and_compatibility_without_execution() {
    let path = std::env::temp_dir().join(format!(
        "zion-zpk-{}-{}.zpk",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, compress(&raw(&entries(&manifest())))).unwrap();
    let good = Command::new(env!("CARGO_BIN_EXE_zpk-inspect"))
        .arg(&path)
        .arg("0.6.1")
        .output()
        .unwrap();
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    assert!(String::from_utf8_lossy(&good.stdout).contains("\"authenticated\": false"));
    let bad = Command::new(env!("CARGO_BIN_EXE_zpk-inspect"))
        .arg(&path)
        .arg("0.6.0")
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("requires ZionOS"));
    fs::remove_file(path).unwrap();
}

#[test]
fn dynamic_musl_interpreter_is_checked_without_loading_it() {
    let mut m = manifest();
    m.files[0].linkage = Some(Linkage::MuslDynamic);
    for interpreter in [
        b"/lib/ld-musl-x86_64.so.1\0".as_slice(),
        b"/lib64/ld-linux-x86-64.so.2\0".as_slice(),
    ] {
        let mut elf = fixture_elf();
        elf.resize(256, 0);
        elf[56] = 2;
        elf[96..104].copy_from_slice(&256u64.to_le_bytes());
        elf[104..112].copy_from_slice(&256u64.to_le_bytes());
        elf[120..176].fill(0);
        elf[120] = 3;
        elf[124] = 4;
        elf[128..136].copy_from_slice(&200u64.to_le_bytes());
        elf[152..160].copy_from_slice(&(interpreter.len() as u64).to_le_bytes());
        elf[160..168].copy_from_slice(&(interpreter.len() as u64).to_le_bytes());
        elf[200..200 + interpreter.len()].copy_from_slice(interpreter);
        let mut e = entries(&m);
        e[2].data = elf;
        checksums(&mut e);
        if interpreter.starts_with(b"/lib/ld-musl") {
            inspect_entries(&e).unwrap();
        } else {
            reject(&e, "interpreter/linkage");
        }
    }
}

#[test]
fn oversized_ui_icon_and_deep_metadata_are_rejected() {
    let m = Manifest::parse(DESKTOP).unwrap();
    for name in ["ui/index.html", "assets/icon.png"] {
        let mut e = entries(&m);
        e.iter_mut().find(|f| f.path == name).unwrap().data = vec![0; 1024 * 1024 + 1];
        checksums(&mut e);
        reject(&e, "size");
    }
    let mut image = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut image, 1025, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![0; 1025])
            .unwrap();
    }
    let mut e = entries(&m);
    e.iter_mut()
        .find(|f| f.path == "assets/icon.png")
        .unwrap()
        .data = image;
    checksums(&mut e);
    reject(&e, "dimensions");
    let deep = format!(
        "schema_version = 1\na = {}0{}\n",
        "[".repeat(1000),
        "]".repeat(1000)
    );
    assert!(Manifest::parse(&deep).is_err());
}

#[test]
fn deterministic_mutation_corpus_never_panics() {
    let original = raw(&entries(&manifest()));
    // Mutate every header-byte position, repairing tar checksum so deeper field checks run.
    for at in 0..512 {
        let mut candidate = original.clone();
        candidate[at] ^= 0x80;
        fix_header_checksum(&mut candidate[..512]);
        let _ = inspect(Cursor::new(compress(&candidate)), Limits::default());
    }
    // Every compressed truncation of this small archive must fail, including frame footer truncation.
    let compressed = compress(&original);
    for cut in 0..compressed.len() {
        assert!(
            inspect(Cursor::new(&compressed[..cut]), Limits::default()).is_err(),
            "accepted truncation at {cut}"
        );
    }
}
