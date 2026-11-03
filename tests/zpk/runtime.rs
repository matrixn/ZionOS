use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    fs,
    io::{Read, Write},
    path::Path,
};
use zpk_runtime::{
    backend::{self, OpenRc, Operations, Output, ServiceBackend, State},
    config::{self, Activation, DesktopAction, Layout, Plan, RuntimeConfig},
    digest, installed,
};
const MANIFEST: &str = include_str!("../../examples/zpk-runtime-demo/manifest.toml");
const CONFIG: &str = include_str!("../../examples/zpk-runtime-demo/runtime.toml");
fn config() -> RuntimeConfig {
    RuntimeConfig::parse(CONFIG).unwrap()
}
fn manifest() -> zpk_schema::Manifest {
    zpk_schema::Manifest::parse(MANIFEST).unwrap()
}
fn plan(root: &Path) -> Plan {
    config().plan(manifest(), root).unwrap()
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
fn sums(version: &Path) {
    let mut hashes = BTreeMap::new();
    for p in ["manifest.toml", "LICENSE", "payload/bin/runtime-demo"] {
        hashes.insert(p, digest(&fs::read(version.join(p)).unwrap()));
    }
    fs::write(
        version.join("checksums.json"),
        serde_json::to_vec(
            &serde_json::json!({"schema_version":1,"algorithm":"sha256","files":hashes}),
        )
        .unwrap(),
    )
    .unwrap();
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let p = plan(root.path());
    fs::create_dir_all(p.layout.version.join("payload/bin")).unwrap();
    fs::create_dir_all(root.path().join("etc")).unwrap();
    fs::write(
        root.path().join("etc/zionos-release"),
        "ZionOS Genesis 0.6.1 (AL 3.23)\n",
    )
    .unwrap();
    fs::write(p.layout.version.join("manifest.toml"), MANIFEST).unwrap();
    fs::write(p.layout.version.join("LICENSE"), "MIT fixture\n").unwrap();
    fs::write(p.layout.version.join("payload/bin/runtime-demo"), elf()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        fs::set_permissions(
            p.layout.version.join("payload/bin/runtime-demo"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        symlink("versions/1.0.0", p.layout.current).unwrap();
    }
    sums(&p.layout.version);
    root
}
struct Mock {
    calls: RefCell<Vec<(String, Vec<String>)>>,
    outputs: RefCell<VecDeque<(i32, String)>>,
    child: bool,
}
impl Mock {
    fn new(out: Vec<(i32, &str)>) -> Self {
        Self {
            calls: RefCell::new(vec![]),
            outputs: RefCell::new(
                out.into_iter()
                    .map(|(code, text)| (code, text.into()))
                    .collect(),
            ),
            child: true,
        }
    }
}
impl Operations for Mock {
    fn run(&self, tool: &str, args: &[&str]) -> zpk_runtime::Result<Output> {
        self.calls
            .borrow_mut()
            .push((tool.into(), args.iter().map(|s| s.to_string()).collect()));
        let (code, stdout) = self
            .outputs
            .borrow_mut()
            .pop_front()
            .unwrap_or((0, String::new()));
        Ok(Output { code, stdout })
    }
    fn observes_child(&self, _: u32, _: &Plan) -> zpk_runtime::Result<bool> {
        Ok(self.child)
    }
}
#[test]
fn config_roundtrip_and_non_root_identity_contract() {
    let c = config();
    let p = c.clone().plan(manifest(), Path::new("/")).unwrap();
    assert_eq!(
        p.layout.version.to_str().unwrap(),
        "/opt/zionos/apps/org.zionos.runtime-demo/versions/1.0.0"
    );
    assert!(p.layout.run.to_str().unwrap().len() < 90);
    RuntimeConfig::parse(&toml::to_string(&c).unwrap()).unwrap();
    for field in ["uid", "gid"] {
        let mut c = config();
        if field == "uid" {
            c.identity.uid = 0;
        } else {
            c.identity.gid = 0;
        }
        assert!(c.plan(manifest(), Path::new("/")).is_err());
    }
    let mut c = config();
    c.identity.name = "nobody".into();
    assert!(c.plan(manifest(), Path::new("/")).is_err());
    assert_ne!(
        config::identity_name("org.zionos.a"),
        config::identity_name("org.zionos.b")
    );
}
#[test]
fn invalid_service_references_and_runtime_fields_fail_closed() {
    for source in [
        MANIFEST.replace(
            "executable = \"payload/bin/runtime-demo\"",
            "executable = \"/bin/sh\"",
        ),
        MANIFEST.replace("run_as = \"package\"", "run_as = \"root\""),
        MANIFEST.replace("\"background-service\", ", ""),
    ] {
        assert!(zpk_schema::Manifest::parse(&source).is_err());
    }
    assert!(RuntimeConfig::parse(&CONFIG.replace(
        "runtime_version = 1",
        "runtime_version = 1\nnas_access = true"
    ))
    .is_err());
    let mut c = config();
    c.runtime_version = 2;
    assert!(c.plan(manifest(), Path::new("/")).is_err());
    let mut c = config();
    c.version = semver::Version::new(9, 0, 0);
    assert!(c.plan(manifest(), Path::new("/")).is_err());
}
#[test]
fn resource_and_permission_declarations_are_not_arbitrary_grants() {
    let mut c = config();
    c.limits.address_space_bytes = 0;
    assert!(c.plan(manifest(), Path::new("/")).is_err());
    let mut c = config();
    c.limits.open_files = 100000;
    assert!(c.plan(manifest(), Path::new("/")).is_err());
    let mut c = config();
    c.restart.max = 0;
    assert!(c.plan(manifest(), Path::new("/")).is_err());
    let mut c = config();
    c.grants.data.push("../../nas".into());
    assert!(c.plan(manifest(), Path::new("/")).is_err());
    let mut c = config();
    c.grants.data.push("state".into());
    assert!(c.plan(manifest(), Path::new("/")).is_err());
    let mut m = manifest();
    m.permissions
        .push(zpk_schema::manifest::Permission::NetworkListen);
    let p = config().plan(m, Path::new("/")).unwrap();
    assert!(zpk_runtime::policy::ENFORCED.contains("denying IP sockets"));
    assert!(zpk_runtime::policy::LIMITS.contains("quotas"));
    assert!(p.program.is_some());
}
#[test]
fn shared_backend_and_static_or_on_demand_desktop_models() {
    let mut m =
        zpk_schema::Manifest::parse(include_str!("fixtures/valid/multi-desktop.toml")).unwrap();
    let c = config();
    m.package.id = c.package_id.clone();
    m.service = Some(manifest().service.unwrap());
    m.service.as_mut().unwrap().executable = m.files[0].path.clone();
    m.permissions
        .push(zpk_schema::manifest::Permission::BackgroundService);
    let mut c = config();
    c.grants.data.clear();
    c.grants.configuration_write.clear();
    let shared = c.clone().plan(m.clone(), Path::new("/")).unwrap();
    assert_eq!(
        shared.desktop_action("hello").unwrap(),
        DesktopAction::EnsureSharedBackend
    );
    assert_eq!(
        shared.desktop_action("status").unwrap(),
        DesktopAction::EnsureSharedBackend
    );
    m.service = None;
    c.activation = Activation::OnDemand;
    c.desktop_entry = Some("hello".into());
    let on_demand = c.clone().plan(m.clone(), Path::new("/")).unwrap();
    assert!(on_demand.program.is_some());
    assert_eq!(
        on_demand.desktop_action("hello").unwrap(),
        DesktopAction::StartOnDemand
    );
    m.desktop[0].executable = None;
    let static_ui = c.clone().plan(m.clone(), Path::new("/")).unwrap();
    assert!(static_ui.program.is_none());
    assert_eq!(
        static_ui.desktop_action("hello").unwrap(),
        DesktopAction::StaticAssets
    );
    let backend = OpenRc {
        operations: Mock::new(vec![]),
    };
    backend.start(&static_ui).unwrap();
    assert_eq!(
        backend.status(&static_ui).unwrap().state,
        State::NotRequired
    );
    assert!(backend.operations.calls.borrow().is_empty());
    c.autostart = true;
    assert!(c.plan(m, Path::new("/")).is_err());
}
#[test]
fn openrc_lifecycle_and_autostart_are_delegated_without_shell_arguments() {
    let p = plan(Path::new("/"));
    let backend = OpenRc {
        operations: Mock::new(vec![]),
    };
    backend.start(&p).unwrap();
    backend.stop(&p).unwrap();
    backend.restart(&p).unwrap();
    backend.autostart(&p, true).unwrap();
    backend.autostart(&p, false).unwrap();
    let calls = backend.operations.calls.borrow();
    assert_eq!(calls[0].1, [p.layout.service.clone(), "start".into()]);
    assert_eq!(calls[1].1[1], "stop");
    assert_eq!(calls[2].1[1], "restart");
    assert_eq!(calls[3].0, "/sbin/rc-update");
    assert_eq!(calls[4].1[0], "del");
}
#[test]
fn status_includes_crashes_transition_and_verified_child() {
    let p = plan(Path::new("/"));
    for (code, state) in [
        (0, State::Running),
        (3, State::Stopped),
        (4, State::Stopping),
        (8, State::Starting),
        (16, State::Inactive),
        (32, State::Crashed),
        (64, State::Crashed),
        (99, State::Unknown),
    ] {
        let backend = OpenRc {
            operations: Mock::new(vec![(code, ""), (0, "123\n2\n")]),
        };
        let status = backend.status(&p).unwrap();
        assert_eq!(status.state, state);
        assert_eq!(status.restart_count, Some(2));
    }
    let mut mock = Mock::new(vec![(0, ""), (0, "123\n0\n")]);
    mock.child = false;
    assert_eq!(
        OpenRc { operations: mock }.status(&p).unwrap().state,
        State::Starting
    );
    let backend = OpenRc {
        operations: Mock::new(vec![(0, ""), (0, "\n\n")]),
    };
    assert_eq!(backend.status(&p).unwrap().state, State::Starting);
    assert!(OpenRc {
        operations: Mock::new(vec![(1, "failed")])
    }
    .start(&p)
    .is_err());
    let backend = OpenRc {
        operations: Mock::new(vec![(3, ""), (0, "\n4\n1\n")]),
    };
    assert_eq!(backend.status(&p).unwrap().state, State::Crashed);
}
#[test]
fn generated_service_contains_only_trusted_launcher_and_bounded_supervision() {
    let mut p = plan(Path::new("/"));
    p.manifest.service.as_mut().unwrap().arguments = vec!["';touch /should-not-exist;#".into()];
    let script = backend::render(&p).unwrap();
    assert!(script.contains("supervisor=supervise-daemon"));
    assert!(script.contains("/usr/local/libexec/zpk-runtime"));
    assert!(!script.contains("touch"));
    assert!(!script.contains("command_user=root"));
    assert!(script.contains("retry=\"TERM/3/KILL/2\""));
    assert!(script.contains("respawn_max=3"));
    let r = tempfile::tempdir().unwrap();
    assert!(backend::render(&plan(r.path())).is_err());
}
#[test]
fn installed_inventory_current_reference_and_content_are_verified() {
    let r = fixture();
    installed::load(r.path(), &config()).unwrap();
    let p = plan(r.path());
    fs::write(p.layout.version.join("extra"), "unlisted").unwrap();
    assert!(installed::load(r.path(), &config()).is_err());
    fs::remove_file(p.layout.version.join("extra")).unwrap();
    fs::write(
        p.layout.version.join("payload/bin/runtime-demo"),
        b"mutated",
    )
    .unwrap();
    assert!(installed::load(r.path(), &config())
        .unwrap_err()
        .to_string()
        .contains("SHA-256"));
}
#[test]
fn invalid_elf_and_runtime_dependency_fail_before_launch() {
    let r = fixture();
    let p = plan(r.path());
    let mut wrong = elf();
    wrong[18] = 183;
    fs::write(p.layout.version.join("payload/bin/runtime-demo"), wrong).unwrap();
    sums(&p.layout.version);
    assert!(installed::load(r.path(), &config()).is_err());
    let r = fixture();
    let p = plan(r.path());
    let mut m = manifest();
    m.runtime
        .dependencies
        .push(zpk_schema::manifest::Dependency {
            id: "org.zionos.missing".into(),
            version: "^1.0".parse().unwrap(),
        });
    fs::write(
        p.layout.version.join("manifest.toml"),
        toml::to_string(&m).unwrap(),
    )
    .unwrap();
    sums(&p.layout.version);
    assert!(installed::load(r.path(), &config()).is_err());
}
#[cfg(unix)]
#[test]
fn installed_paths_and_permissions_refuse_escape_and_app_writes() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let r = fixture();
    let p = plan(r.path());
    fs::remove_file(&p.layout.current).unwrap();
    symlink("../../host", &p.layout.current).unwrap();
    assert!(installed::load(r.path(), &config()).is_err());
    fs::remove_file(&p.layout.current).unwrap();
    symlink("versions/1.0.0", &p.layout.current).unwrap();
    let binary = p.layout.version.join("payload/bin/runtime-demo");
    fs::remove_file(&binary).unwrap();
    symlink("/etc/passwd", &binary).unwrap();
    assert!(installed::load(r.path(), &config()).is_err());
    fs::remove_file(&binary).unwrap();
    fs::write(&binary, elf()).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(installed::load(r.path(), &config()).is_err());
}
#[test]
fn log_reads_are_bounded_and_not_package_path_controlled() {
    let r = tempfile::tempdir().unwrap();
    let p = plan(r.path());
    fs::create_dir_all(&p.layout.logs).unwrap();
    fs::write(p.layout.logs.join("stdout.log"), "0123456789").unwrap();
    assert_eq!(backend::logs(&p, false, 4).unwrap(), "6789");
    assert!(backend::logs(&p, false, 0).is_err());
    assert!(backend::logs(&p, true, 4).is_err());
}
#[cfg(target_os = "linux")]
#[test]
fn health_checks_unix_peer_identity_and_uses_no_application_hook() {
    use std::os::unix::net::UnixListener;
    let r = tempfile::tempdir().unwrap();
    let mut p = plan(r.path());
    fs::create_dir_all(&p.layout.run).unwrap();
    let listener = UnixListener::bind(p.layout.run.join("service.sock")).unwrap();
    let thread = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut bytes = [0; 14];
        s.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ZPK_HEALTH_V1\n");
        s.write_all(b"OK\n\n").unwrap();
    });
    p.config.identity.uid = unsafe { libc::geteuid() };
    p.config.identity.gid = unsafe { libc::getegid() };
    let status = backend::Status {
        state: State::Running,
        pid: Some(123),
        restart_count: Some(0),
        backend_code: 0,
    };
    assert!(backend::health(&p, &status).unwrap().healthy);
    thread.join().unwrap();
    fs::remove_file(p.layout.run.join("service.sock")).unwrap();
    let wrong_peer = UnixListener::bind(p.layout.run.join("service.sock")).unwrap();
    p.config.identity.uid = 23101;
    assert!(!backend::health(&p, &status).unwrap().healthy);
    drop(wrong_peer);
    let listener = UnixListener::bind(p.layout.run.join("wrong.sock")).unwrap();
    drop(listener);
    p.config.identity.uid = 23101;
    assert!(
        !backend::health(
            &p,
            &backend::Status {
                state: State::Stopped,
                pid: None,
                restart_count: None,
                backend_code: 3
            }
        )
        .unwrap()
        .healthy
    );
}
#[test]
fn administration_rejects_temp_roots_and_unknown_control_paths() {
    let r = fixture();
    let p = installed::load(r.path(), &config()).unwrap();
    assert!(zpk_runtime::launch::prepare(&p).is_err());
    assert!(Layout::new(Path::new("relative"), &config()).is_err());
    let cfg = r.path().join("runtime.toml");
    fs::write(&cfg, CONFIG).unwrap();
    let planned = std::process::Command::new(env!("CARGO_BIN_EXE_zpk-runtime"))
        .args(["plan", "--config"])
        .arg(cfg)
        .arg("--root")
        .arg(r.path())
        .output()
        .unwrap();
    assert!(planned.status.success());
    let report: serde_json::Value = serde_json::from_slice(&planned.stdout).unwrap();
    assert_eq!(report["policy_applied"], false);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_zpk-runtime"))
        .args(["start", "--config", "not-config", "--root"])
        .arg(r.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
}
