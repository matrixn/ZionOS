//! Host tests are filesystem-only. Every service operation below is a mock.
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};
use zpk_manager::{
    database::Database,
    digest,
    filesystem::Environment,
    host::Host,
    manager::{Faults, Manager, NoFaults},
    model::Review,
    Result,
};
use zpk_runtime::{
    backend::{ServiceBackend, State, Status},
    config::{identity_name, Identity, Plan},
};
use zpk_schema::{manifest::Permission, Manifest};
const BASE: &str = include_str!("../../examples/zpk-runtime-demo/manifest.toml");
struct Mock {
    root: PathBuf,
}
impl Mock {
    fn states(&self) -> BTreeMap<String, bool> {
        fs::read(self.root.join(".mock-services"))
            .ok()
            .map(|b| serde_json::from_slice(&b).unwrap())
            .unwrap_or_default()
    }
    fn set(&self, p: &Plan, on: bool) -> Result<()> {
        let mut m = self.states();
        m.insert(p.config.package_id.clone(), on);
        fs::write(self.root.join(".mock-services"), serde_json::to_vec(&m)?)?;
        Ok(())
    }
}
impl ServiceBackend for Mock {
    fn start(&self, p: &Plan) -> Result<()> {
        if p.program.is_none() {
            return Ok(());
        }
        if self
            .root
            .join(format!(".fail-start-{}", p.config.version))
            .exists()
        {
            return Err("injected service start failure".into());
        }
        self.set(p, true)
    }
    fn stop(&self, p: &Plan) -> Result<()> {
        if self.root.join(".fail-stop").exists() {
            return Err("injected stop failure".into());
        }
        self.set(p, false)
    }
    fn restart(&self, p: &Plan) -> Result<()> {
        self.stop(p)?;
        self.start(p)
    }
    fn status(&self, p: &Plan) -> Result<Status> {
        if p.program.is_none() {
            return Ok(Status {
                state: State::NotRequired,
                pid: None,
                restart_count: None,
                backend_code: 0,
            });
        }
        Ok(Status {
            state: if self.states().get(&p.config.package_id) == Some(&true) {
                State::Running
            } else {
                State::Stopped
            },
            pid: None,
            restart_count: Some(0),
            backend_code: 0,
        })
    }
    fn autostart(&self, _: &Plan, _: bool) -> Result<()> {
        Ok(())
    }
}
impl Host for Mock {
    fn identity(&self, id: &str, reserved: &std::collections::BTreeSet<u32>) -> Result<Identity> {
        if self.root.join(".forbid-allocation").exists() {
            return Err("allocator must not run during retained-data reinstall".into());
        }
        let d = digest(id.as_bytes());
        let uid = 10000 + u32::from_str_radix(&d[..6], 16)? % 40000;
        if reserved.contains(&uid) {
            return Err("mock identity collision".into());
        }
        Ok(Identity {
            name: identity_name(id),
            uid,
            gid: uid,
        })
    }
    fn register(&self, p: &Plan) -> Result<()> {
        for path in [&p.layout.data, &p.layout.logs] {
            fs::create_dir_all(path)?;
        }
        for s in &p.manifest.storage {
            let base = if matches!(s.kind, zpk_schema::manifest::StorageKind::Data) {
                &p.layout.data
            } else {
                &p.layout.configuration
            };
            fs::create_dir_all(base.join(&s.name))?;
        }
        Ok(())
    }
    fn unregister(&self, _: &Plan) -> Result<()> {
        Ok(())
    }
    fn healthy(&self, p: &Plan) -> Result<bool> {
        Ok(matches!(
            self.status(p)?.state,
            State::Running | State::NotRequired
        ))
    }
    fn logs(&self, _: &Plan, _: bool) -> Result<String> {
        Ok("mock log".into())
    }
}
fn open(root: &Path) -> Manager<Mock> {
    Manager::open(root, Environment::TemporaryTest, Mock { root: root.into() }).unwrap()
}
fn root() -> tempfile::TempDir {
    let r = tempfile::tempdir().unwrap();
    fs::create_dir(r.path().join("etc")).unwrap();
    fs::write(
        r.path().join("etc/zionos-release"),
        "ZionOS Genesis 0.6.1\n",
    )
    .unwrap();
    r
}
fn review() -> Review {
    Review {
        desktop_entry: None,
        trusted_local_development: true,
        approved_permissions: vec![
            Permission::BackgroundService,
            Permission::PersistentData,
            Permission::ConfigurationWrite,
        ],
        start: true,
        autostart: false,
        unix_health: false,
    }
}
fn manifest(version: &str) -> Manifest {
    Manifest::parse(
        &BASE
            .replace("version = \"1.0.0\"", &format!("version = \"{version}\""))
            .replace(
                "upgrades_from = \">=0.1.0, <1.0.0\"",
                "upgrades_from = \">=1.0.0, <3.0.0\"",
            ),
    )
    .unwrap()
}
fn elf() -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    b[16] = 2;
    b[18] = 62;
    b[20] = 1;
    b[24..32].copy_from_slice(&0x400078u64.to_le_bytes());
    b[32] = 64;
    b[52] = 64;
    b[54] = 56;
    b[56] = 1;
    b[64] = 1;
    b[68] = 5;
    b[80..88].copy_from_slice(&0x400000u64.to_le_bytes());
    b[96] = 128;
    b[104] = 128;
    b[120] = 0xc3;
    b
}
fn append(t: &mut tar::Builder<Vec<u8>>, name: &str, data: &[u8], mode: u32) {
    let mut h = tar::Header::new_ustar();
    h.set_size(data.len() as u64);
    h.set_mode(mode);
    h.set_uid(0);
    h.set_gid(0);
    h.set_mtime(0);
    h.set_cksum();
    t.append_data(&mut h, name, Cursor::new(data)).unwrap();
}
fn package(root: &Path, m: &Manifest) -> PathBuf {
    let bytes = toml::to_string(m).unwrap().into_bytes();
    let mut t = tar::Builder::new(Vec::new());
    let mut sums = BTreeMap::new();
    let mut entries = vec![
        ("manifest.toml", bytes, 0o644),
        ("LICENSE", b"MIT fixture".to_vec(), 0o644),
    ];
    for file in &m.files {
        use zpk_schema::manifest::FileKind;
        let b = match file.kind {
            FileKind::Executable | FileKind::Library => elf(),
            FileKind::Ui => b"<!doctype html><html><body>hello</body></html>".to_vec(),
            FileKind::Asset => b"owned fixture".to_vec(),
            FileKind::Icon => {
                let mut b = vec![];
                {
                    let mut e = png::Encoder::new(&mut b, 1, 1);
                    e.set_color(png::ColorType::Rgba);
                    e.set_depth(png::BitDepth::Eight);
                    let mut w = e.write_header().unwrap();
                    w.write_image_data(&[0, 0, 0, 255]).unwrap();
                }
                b
            }
        };
        entries.push((
            &file.path,
            b,
            if file.kind == FileKind::Executable {
                0o755
            } else {
                0o644
            },
        ));
    }
    for (name, b, mode) in entries {
        sums.insert(name, digest(&b));
        append(&mut t, name, &b, mode);
    }
    let checksums = serde_json::to_vec(
        &serde_json::json!({"schema_version":1,"algorithm":"sha256","files":sums}),
    )
    .unwrap();
    append(&mut t, "checksums.json", &checksums, 0o644);
    let file = root.join(format!("{}-{}.zpk", m.package.id, m.package.version));
    fs::write(
        &file,
        zstd::stream::encode_all(Cursor::new(t.into_inner().unwrap()), 3).unwrap(),
    )
    .unwrap();
    file
}
#[test]
fn installation_lifecycle_upgrade_rollback_uninstall_preserve_data() {
    let r = root();
    let one = package(r.path(), &manifest("1.0.0"));
    let two = package(r.path(), &manifest("2.0.0"));
    let mut m = open(r.path());
    let report = m.install(&one, false, &review(), &NoFaults).unwrap();
    assert_eq!(report.state, "COMMITTED");
    let id = &report.package_id;
    assert_eq!(m.list().unwrap().len(), 1);
    m.verify(id).unwrap();
    assert_eq!(m.lifecycle(id, "status").unwrap().state, State::Running);
    m.lifecycle(id, "stop").unwrap();
    assert_eq!(m.lifecycle(id, "status").unwrap().state, State::Stopped);
    m.lifecycle(id, "start").unwrap();
    m.lifecycle(id, "restart").unwrap();
    assert_eq!(m.logs(id, false).unwrap(), "mock log");
    let data = r
        .path()
        .join("var/lib/zionos/apps")
        .join(id)
        .join("state/user-data");
    fs::write(&data, "preserve").unwrap();
    m.install(&two, true, &review(), &NoFaults).unwrap();
    let record = m.info(id).unwrap();
    assert_eq!(record.active, "2.0.0");
    assert_eq!(record.previous.as_deref(), Some("1.0.0"));
    assert_eq!(record.versions.len(), 2);
    m.rollback(id, &NoFaults).unwrap();
    assert_eq!(m.info(id).unwrap().active, "1.0.0");
    assert_eq!(fs::read_to_string(&data).unwrap(), "preserve");
    m.uninstall(id, &NoFaults).unwrap();
    assert!(m.list().unwrap().is_empty());
    assert_eq!(fs::read_to_string(&data).unwrap(), "preserve");
    assert!(!r
        .path()
        .join("opt/zionos/apps")
        .join(id)
        .join("versions")
        .exists());
    m.install(&one, false, &review(), &NoFaults).unwrap();
    assert_eq!(fs::read_to_string(data).unwrap(), "preserve");
}
#[test]
fn unsigned_trust_permission_review_and_unsupported_network_are_explicit() {
    let r = root();
    let p = package(r.path(), &manifest("1.0.0"));
    let mut m = open(r.path());
    let mut v = review();
    v.trusted_local_development = false;
    assert!(m
        .install(&p, false, &v, &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("authenticate"));
    v = review();
    v.approved_permissions.clear();
    assert!(m.install(&p, false, &v, &NoFaults).is_err());
    let mut n = manifest("1.0.0");
    n.permissions.push(Permission::NetworkListen);
    let p = package(r.path(), &n);
    v = review();
    v.approved_permissions.push(Permission::NetworkListen);
    assert!(m
        .install(&p, false, &v, &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("network"));
    assert!(m.list().unwrap().is_empty());
}
#[test]
fn desktop_selection_static_ui_and_shared_backend_are_recorded_without_fake_daemons() {
    for case in ["single", "multiple", "static", "shared"] {
        let r = root();
        let mut manager = open(r.path());
        let mut m = Manifest::parse(include_str!("fixtures/valid/multi-desktop.toml")).unwrap();
        let mut policy = review();
        policy.approved_permissions = vec![Permission::DesktopUi];
        policy.start = false;
        if case == "multiple" {
            m.desktop[1].executable = Some(m.files[0].path.clone());
            let p = package(r.path(), &m);
            assert!(manager
                .install(&p, false, &policy, &NoFaults)
                .unwrap_err()
                .to_string()
                .contains("--desktop-entry"));
            policy.desktop_entry = Some("status".into());
        }
        if case == "static" {
            for d in &mut m.desktop {
                d.executable = None;
            }
        }
        if case == "shared" {
            let mut service = manifest("1.0.0").service.unwrap();
            service.executable = m.files[0].path.clone();
            m.service = Some(service);
            m.permissions.push(Permission::BackgroundService);
            policy.approved_permissions = m.permissions.clone();
        }
        let p = package(r.path(), &m);
        manager.install(&p, false, &policy, &NoFaults).unwrap();
        let record = manager.info(&m.package.id).unwrap();
        assert_eq!(record.current().parsed().unwrap().desktop.len(), 2);
        let expected = match case {
            "single" => Some("hello"),
            "multiple" => Some("status"),
            _ => None,
        };
        assert_eq!(
            record.current().configuration.desktop_entry.as_deref(),
            expected
        );
        assert_eq!(
            record.current().configuration.activation,
            if case == "shared" {
                zpk_runtime::config::Activation::Background
            } else {
                zpk_runtime::config::Activation::OnDemand
            }
        );
        if case == "static" {
            assert_eq!(
                manager.lifecycle(&m.package.id, "status").unwrap().state,
                State::NotRequired
            );
            assert_eq!(record.service_state, "not-required");
        }
    }
}
#[test]
fn malformed_archive_cpu_hash_and_path_fail_before_activation() {
    let r = root();
    let p = package(r.path(), &manifest("1.0.0"));
    let mut b = fs::read(&p).unwrap();
    b[0] = 0;
    fs::write(&p, b).unwrap();
    let mut m = open(r.path());
    assert!(m.install(&p, false, &review(), &NoFaults).is_err());
    let mut bad = manifest("1.0.0");
    bad.target.cpu = "x86-64-v3".into();
    let p = package(r.path(), &bad);
    assert!(m
        .install(&p, false, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("unsupported target"));
    assert!(m.database.operation_states().unwrap().is_empty());
}
#[test]
fn missing_dependencies_conflicts_reverse_constraints_and_cycles() {
    let r = root();
    let mut m = open(r.path());
    let mut a = manifest("1.0.0");
    a.runtime
        .dependencies
        .push(zpk_schema::manifest::Dependency {
            id: "org.example.dep".into(),
            version: "^1".parse().unwrap(),
        });
    let p = package(r.path(), &a);
    assert!(m
        .install(&p, false, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("missing ZPK"));
    let mut dep = manifest("1.0.0");
    dep.package.id = "org.example.dep".into();
    let p = package(r.path(), &dep);
    m.install(&p, false, &review(), &NoFaults).unwrap();
    let p = package(r.path(), &a);
    m.install(&p, false, &review(), &NoFaults).unwrap();
    assert!(m.uninstall("org.example.dep", &NoFaults).is_err());
    dep.package.version = "2.0.0".parse().unwrap();
    let p = package(r.path(), &dep);
    assert!(m
        .install(&p, true, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("requires"));
    dep.package.version = "1.1.0".parse().unwrap();
    dep.runtime
        .dependencies
        .push(zpk_schema::manifest::Dependency {
            id: a.package.id.clone(),
            version: "^1".parse().unwrap(),
        });
    let p = package(r.path(), &dep);
    assert!(m
        .install(&p, true, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("cycle"));
    dep.runtime.dependencies.clear();
    dep.compatibility.conflicts.push(a.package.id.clone());
    let p = package(r.path(), &dep);
    assert!(m
        .install(&p, true, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("conflict"));
    dep.compatibility.conflicts.clear();
    let p = package(r.path(), &dep);
    assert!(m
        .install(&p, true, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("stop dependent"));
    m.lifecycle(&a.package.id, "stop").unwrap();
    m.install(&p, true, &review(), &NoFaults).unwrap();
}
#[test]
fn failed_upgrade_restores_binaries_service_and_user_data() {
    let r = root();
    let a = package(r.path(), &manifest("1.0.0"));
    let b = package(r.path(), &manifest("2.0.0"));
    let mut m = open(r.path());
    let id = manifest("1.0.0").package.id;
    m.install(&a, false, &review(), &NoFaults).unwrap();
    fs::write(r.path().join(".fail-start-2.0.0"), "").unwrap();
    assert!(m.install(&b, true, &review(), &NoFaults).is_err());
    assert_eq!(m.info(&id).unwrap().active, "1.0.0");
    m.verify(&id).unwrap();
    assert_eq!(m.lifecycle(&id, "status").unwrap().state, State::Running);
    assert_eq!(
        m.database.operation_states().unwrap().last().unwrap().1,
        "FAILED"
    );
}
#[test]
fn recovery_required_blocks_mutation_and_plan_does_not_repair() {
    let r = root();
    let a = package(r.path(), &manifest("1.0.0"));
    let b = package(r.path(), &manifest("2.0.0"));
    let mut m = open(r.path());
    let id = manifest("1.0.0").package.id;
    m.install(&a, false, &review(), &NoFaults).unwrap();
    fs::write(r.path().join(".fail-stop"), "").unwrap();
    assert!(m
        .install(&b, true, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("RECOVERY_REQUIRED"));
    assert_eq!(m.repair_plan(&id).unwrap()["changes_applied"], false);
    assert!(m.lifecycle(&id, "start").is_err());
    fs::remove_file(r.path().join(".fail-stop")).unwrap();
    m.reconcile().unwrap();
    m.verify(&id).unwrap();
}
#[test]
fn concurrent_manager_is_refused_and_registry_corruption_fails_closed() {
    let r = root();
    let mut m = open(r.path());
    assert!(Manager::open(
        r.path(),
        Environment::TemporaryTest,
        Mock {
            root: r.path().into()
        }
    )
    .is_err());
    let p = package(r.path(), &manifest("1.0.0"));
    m.install(&p, false, &review(), &NoFaults).unwrap();
    drop(m);
    let db = r.path().join("var/lib/zionos/packages/registry.sqlite3");
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute("UPDATE packages SET record='{}'", []).unwrap();
    drop(c);
    assert!(Manager::open(
        r.path(),
        Environment::TemporaryTest,
        Mock {
            root: r.path().into()
        }
    )
    .is_err());
}
#[test]
fn symlink_destination_and_foreign_directory_collisions_are_refused() {
    use std::os::unix::fs::symlink;
    let r = root();
    let mut m = open(r.path());
    let id = manifest("1.0.0").package.id;
    let app = r.path().join("opt/zionos/apps").join(&id);
    fs::create_dir(&app).unwrap();
    fs::write(app.join("foreign"), "keep").unwrap();
    let p = package(r.path(), &manifest("1.0.0"));
    assert!(m.install(&p, false, &review(), &NoFaults).is_err());
    m.reconcile().unwrap();
    assert_eq!(fs::read_to_string(app.join("foreign")).unwrap(), "keep");
    drop(m);
    let r = root();
    fs::create_dir_all(r.path().join("opt/zionos")).unwrap();
    symlink("/tmp", r.path().join("opt/zionos/apps")).unwrap();
    assert!(Manager::open(
        r.path(),
        Environment::TemporaryTest,
        Mock {
            root: r.path().into()
        }
    )
    .is_err());
}
#[test]
fn installed_mutation_and_data_schema_migration_are_refused() {
    let r = root();
    let mut m = open(r.path());
    let a = package(r.path(), &manifest("1.0.0"));
    let id = manifest("1.0.0").package.id;
    m.install(&a, false, &review(), &NoFaults).unwrap();
    let mut v = manifest("2.0.0");
    v.compatibility.data_schema = 2;
    let p = package(r.path(), &v);
    assert!(m
        .install(&p, true, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("migration"));
    let p = m.verify(&id).unwrap();
    fs::write(p.layout.version.join("LICENSE"), "changed").unwrap();
    assert!(m.verify(&id).is_err());
    let plan = m.repair_plan(&id).unwrap();
    assert_eq!(plan["changes_applied"], false);
    assert_ne!(plan["verification"], "ok");
    assert_eq!(m.lifecycle(&id, "stop").unwrap().state, State::Stopped);
}

#[test]
fn malicious_tar_links_traversal_duplicates_and_digest_mutation_are_rejected() {
    let r = root();
    let mut m = open(r.path());
    for attack in [
        "traversal",
        "absolute",
        "symlink",
        "hardlink",
        "device",
        "duplicate",
        "hash",
    ] {
        let p = package(r.path(), &manifest("1.0.0"));
        let mut tar = zstd::stream::decode_all(Cursor::new(fs::read(&p).unwrap())).unwrap();
        let mut offset = 0usize;
        loop {
            let name = std::str::from_utf8(&tar[offset..offset + 100])
                .unwrap()
                .trim_end_matches('\0')
                .to_owned();
            let size = usize::from_str_radix(
                std::str::from_utf8(&tar[offset + 124..offset + 136])
                    .unwrap()
                    .trim_matches('\0')
                    .trim(),
                8,
            )
            .unwrap();
            if name == "payload/bin/runtime-demo" {
                match attack {
                    "hash" => tar[offset + 512 + 120] ^= 1,
                    "traversal" | "absolute" | "duplicate" => {
                        let path = match attack {
                            "traversal" => "../escape",
                            "absolute" => "/etc/escape",
                            _ => "LICENSE",
                        };
                        tar[offset..offset + 100].fill(0);
                        tar[offset..offset + path.len()].copy_from_slice(path.as_bytes());
                    }
                    "symlink" => tar[offset + 156] = b'2',
                    "hardlink" => tar[offset + 156] = b'1',
                    "device" => tar[offset + 156] = b'3',
                    _ => unreachable!(),
                }
                tar[offset + 148..offset + 156].fill(b' ');
                let sum: u32 = tar[offset..offset + 512].iter().map(|b| *b as u32).sum();
                tar[offset + 148..offset + 156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
                break;
            }
            offset += 512 + size.div_ceil(512) * 512;
        }
        fs::write(&p, zstd::stream::encode_all(Cursor::new(tar), 3).unwrap()).unwrap();
        assert!(
            m.install(&p, false, &review(), &NoFaults).is_err(),
            "{attack}"
        );
    }
    assert!(m.database.operation_states().unwrap().is_empty());
    assert!(m.list().unwrap().is_empty());
    assert!(!r.path().join("escape").exists());
}
#[test]
fn oversized_sources_and_writable_destination_permissions_fail_closed() {
    use std::os::unix::fs::PermissionsExt;
    let r = root();
    let mut m = open(r.path());
    let p = r.path().join("oversized.zpk");
    fs::File::create(&p)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(m.install(&p, false, &review(), &NoFaults).is_err());
    let p = package(r.path(), &manifest("1.0.0"));
    let apps = r.path().join("opt/zionos/apps");
    fs::set_permissions(&apps, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(m.install(&p, false, &review(), &NoFaults).is_err());
    fs::set_permissions(&apps, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(m.list().unwrap().is_empty());
}
#[test]
fn retained_version_tampering_is_not_activated_and_untracked_trees_are_preserved() {
    let r = root();
    let mut m = open(r.path());
    let a = package(r.path(), &manifest("1.0.0"));
    let b = package(r.path(), &manifest("2.0.0"));
    let id = manifest("1.0.0").package.id;
    m.install(&a, false, &review(), &NoFaults).unwrap();
    m.install(&b, true, &review(), &NoFaults).unwrap();
    let old = r
        .path()
        .join("opt/zionos/apps")
        .join(&id)
        .join("versions/1.0.0");
    fs::write(old.join("LICENSE"), "MIT altered").unwrap();
    let mut hashes = BTreeMap::new();
    for path in ["manifest.toml", "LICENSE", "payload/bin/runtime-demo"] {
        hashes.insert(path, digest(&fs::read(old.join(path)).unwrap()));
    }
    fs::write(
        old.join("checksums.json"),
        serde_json::to_vec(
            &serde_json::json!({"schema_version":1,"algorithm":"sha256","files":hashes}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(m.rollback(&id, &NoFaults).is_err());
    assert_eq!(m.info(&id).unwrap().active, "2.0.0");
    m.verify(&id).unwrap();
    let foreign = old.parent().unwrap().join("untracked");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("keep"), "keep").unwrap();
    assert!(m.uninstall(&id, &NoFaults).is_err());
    assert_eq!(fs::read_to_string(foreign.join("keep")).unwrap(), "keep");
    m.verify(&id).unwrap();
}
#[test]
fn registry_semantic_corruption_and_journal_corruption_are_reported() {
    let r = root();
    let mut m = open(r.path());
    let a = package(r.path(), &manifest("1.0.0"));
    m.install(&a, false, &review(), &NoFaults).unwrap();
    drop(m);
    let db = r.path().join("var/lib/zionos/packages/registry.sqlite3");
    let c = rusqlite::Connection::open(&db).unwrap();
    let text: String = c
        .query_row("SELECT record FROM packages", [], |r| r.get(0))
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["active"] = "../../bad".into();
    let text = serde_json::to_string(&value).unwrap();
    c.execute(
        "UPDATE packages SET record=?1,hash=?2",
        rusqlite::params![text, digest(text.as_bytes())],
    )
    .unwrap();
    drop(c);
    assert!(Manager::open(
        r.path(),
        Environment::TemporaryTest,
        Mock {
            root: r.path().into()
        }
    )
    .is_err());
    let r = root();
    let mut m = open(r.path());
    let a = package(r.path(), &manifest("1.0.0"));
    m.install(&a, false, &review(), &NoFaults).unwrap();
    drop(m);
    let c = rusqlite::Connection::open(r.path().join("var/lib/zionos/packages/registry.sqlite3"))
        .unwrap();
    c.execute(
        "UPDATE operations SET state='RECOVERY_REQUIRED',record='{}'",
        [],
    )
    .unwrap();
    drop(c);
    assert!(Manager::open(
        r.path(),
        Environment::TemporaryTest,
        Mock {
            root: r.path().into()
        }
    )
    .is_err());
}
struct Crash {
    point: String,
}
impl Faults for Crash {
    fn checkpoint(&self, p: &str) -> Result<()> {
        if p == self.point {
            unsafe { libc::_exit(77) }
        }
        Ok(())
    }
}
#[test]
fn crash_worker() {
    let Ok(path) = std::env::var("ZPK_MANAGER_CRASH_ROOT") else {
        return;
    };
    let mode = std::env::var("ZPK_MANAGER_CRASH_MODE").unwrap();
    let point = std::env::var("ZPK_MANAGER_CRASH_POINT").unwrap();
    let path = Path::new(&path);
    let mut m = open(path);
    let id = manifest("1.0.0").package.id;
    let crash = Crash { point };
    match mode.as_str() {
        "install" | "reinstall" => {
            let p = package(path, &manifest("1.0.0"));
            m.install(&p, false, &review(), &crash).unwrap();
        }
        "upgrade" => {
            let p = package(path, &manifest("2.0.0"));
            m.install(&p, true, &review(), &crash).unwrap();
        }
        "rollback" => {
            m.rollback(&id, &crash).unwrap();
        }
        "uninstall" => {
            m.uninstall(&id, &crash).unwrap();
        }
        _ => panic!("unknown mode"),
    };
    panic!("checkpoint not reached");
}
#[test]
fn actual_process_crashes_at_transaction_boundaries_reconcile_consistently() {
    for mode in ["install", "reinstall", "upgrade", "rollback", "uninstall"] {
        for point in [
            "prepared",
            "stage-written",
            "staged",
            "activating",
            "old-stopped",
            "version-published",
            "reference-switched",
            "service-registered",
            "health-verified",
            "committed",
        ] {
            if point == "stage-written" && ["rollback", "uninstall"].contains(&mode) {
                continue;
            }
            if mode == "uninstall" && ["reference-switched", "service-registered"].contains(&point)
            {
                continue;
            }
            let r = root();
            let id = manifest("1.0.0").package.id;
            if mode != "install" {
                let mut m = open(r.path());
                let a = package(r.path(), &manifest("1.0.0"));
                m.install(&a, false, &review(), &NoFaults).unwrap();
                if mode == "rollback" {
                    let b = package(r.path(), &manifest("2.0.0"));
                    m.install(&b, true, &review(), &NoFaults).unwrap();
                }
                if mode == "reinstall" {
                    m.uninstall(&id, &NoFaults).unwrap();
                }
            }
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "crash_worker", "--nocapture"])
                .env("ZPK_MANAGER_CRASH_ROOT", r.path())
                .env("ZPK_MANAGER_CRASH_MODE", mode)
                .env("ZPK_MANAGER_CRASH_POINT", point)
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(77),
                "{mode}/{point}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let mut m = open(r.path());
            m.reconcile().unwrap();
            assert!(
                m.database.pending(r.path()).unwrap().is_empty(),
                "{mode}/{point}"
            );
            let installed = m.list().unwrap();
            let exists = if mode == "install" || mode == "reinstall" {
                point == "committed"
            } else {
                !(mode == "uninstall" && point == "committed")
            };
            assert_eq!(installed.contains_key(&id), exists, "{mode}/{point}");
            if mode == "reinstall" {
                assert_eq!(
                    m.database.retained(r.path()).unwrap().contains_key(&id),
                    point != "committed"
                );
            }
            if exists {
                m.verify(&id).unwrap();
                let expected = if mode == "upgrade" && point == "committed"
                    || mode == "rollback" && point != "committed"
                {
                    "2.0.0"
                } else {
                    "1.0.0"
                };
                assert_eq!(m.info(&id).unwrap().active, expected, "{mode}/{point}");
            }
        }
    }
}
#[test]
fn cli_rejects_unknown_or_unsafe_operations_before_host_mutation() {
    for args in [
        vec!["install", "a.zpk"],
        vec!["uninstall", "org.example.app", "--delete-data"],
        vec!["repair", "org.example.app"],
        vec!["--root", "/tmp"],
        vec!["unknown"],
    ] {
        assert!(!Command::new(env!("CARGO_BIN_EXE_zpk"))
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }
    assert!(Command::new(env!("CARGO_BIN_EXE_zpk"))
        .arg("--help")
        .output()
        .unwrap()
        .status
        .success());
}
#[test]
fn unsafe_test_roots_and_registry_symlinks_are_refused() {
    assert!(Manager::open(
        Path::new("/"),
        Environment::TemporaryTest,
        Mock { root: "/".into() }
    )
    .is_err());
    let r = root();
    let m = open(r.path());
    drop(m);
    let db = r.path().join("var/lib/zionos/packages/registry.sqlite3");
    fs::remove_file(&db).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", &db).unwrap();
    assert!(Database::open(
        &zpk_manager::filesystem::Root::new(r.path(), Environment::TemporaryTest).unwrap(),
        &db
    )
    .is_err());
}

#[test]
fn native_identity_selection_reserves_registry_ids_without_host_account_changes() {
    let reserved = std::collections::BTreeSet::from([10000, 10001, 10002]);
    let id = format!("org.zionos.manager-identity-test-{}", std::process::id());
    let identity = zpk_manager::host::Alpine.identity(&id, &reserved).unwrap();
    assert!(!reserved.contains(&identity.uid));
    assert!(!reserved.contains(&identity.gid));
    assert!((10000..=60000).contains(&identity.uid));
    // identity() reads NSS only; registration is never invoked against the developer host.
}
#[test]
fn retained_data_identity_and_schema_survive_uninstall_and_manager_restart() {
    let r = root();
    let a = package(r.path(), &manifest("1.0.0"));
    let mut m = open(r.path());
    let id = manifest("1.0.0").package.id;
    m.install(&a, false, &review(), &NoFaults).unwrap();
    let uid = m.info(&id).unwrap().current().configuration.identity.uid;
    m.uninstall(&id, &NoFaults).unwrap();
    assert_eq!(
        m.database.retained(r.path()).unwrap()[&id]
            .current()
            .configuration
            .identity
            .uid,
        uid
    );
    drop(m);
    fs::write(r.path().join(".forbid-allocation"), "").unwrap();
    let mut m = open(r.path());
    let mut incompatible = manifest("2.0.0");
    incompatible.compatibility.data_schema = 2;
    let p = package(r.path(), &incompatible);
    assert!(m
        .install(&p, false, &review(), &NoFaults)
        .unwrap_err()
        .to_string()
        .contains("migration"));
    assert_eq!(m.repair_plan(&id).unwrap()["retained_data"], true);
    m.install(&a, false, &review(), &NoFaults).unwrap();
    assert_eq!(
        m.info(&id).unwrap().current().configuration.identity.uid,
        uid
    );
    assert!(m.database.retained(r.path()).unwrap().is_empty());
}
