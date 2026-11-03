//! Trusted verifier/fault fixture. Never shipped in the ISO or called by HTTP.
use std::{
    fs,
    io::{Read, Write},
    os::unix::{fs::MetadataExt, net::UnixStream},
    path::Path,
    process::Command,
    time::Duration,
};
use zpk_control::{
    ipc,
    protocol::{Action, Credentials, Request},
    Result,
};
use zpk_manager::{
    filesystem::Environment,
    host::Alpine,
    manager::{Faults, Manager},
    model::Review,
};
use zpk_runtime::{backend::State, config::identity_name};
const ID: &str = "org.zionos.zion-hello";
const MEDIA: &str = "/mnt/zion-center-fixtures";
fn guard() {
    assert_eq!(unsafe { libc::geteuid() }, 0);
    assert_eq!(
        fs::read_to_string("/sys/class/dmi/id/sys_vendor")
            .unwrap()
            .trim(),
        "QEMU"
    );
    assert_eq!(
        fs::read_to_string("/sys/block/vda/serial").unwrap().trim(),
        "ZIONCENTER01"
    );
    assert!(Path::new("/run/zionos/platform-ready").is_file());
    assert!(Path::new(MEDIA).is_dir());
}
fn message(request: &str) -> String {
    let mut s = UnixStream::connect(format!(
        "/run/zionos/apps/{}/service.sock",
        identity_name(ID)
    ))
    .unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    s.take(2049).read_to_string(&mut out).unwrap();
    assert!(out.len() < 2049);
    out
}
fn check(version: &str) {
    let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine).unwrap();
    assert!(m.reconcile().unwrap().is_empty());
    let r = m.info(ID).unwrap();
    assert_eq!(r.active, version);
    assert!(r.current().configuration.autostart);
    assert!(r.current().configuration.identity.uid >= 10000);
    let p = m.verify(ID).unwrap();
    let state = m.lifecycle(ID, "status").unwrap();
    assert_eq!(state.state, State::Running);
    let pid = state.pid.unwrap();
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    assert!(status.contains("NoNewPrivs:\t1"));
    assert!(status.contains("CapEff:\t0000000000000000"));
    assert!(status.contains("Seccomp:\t2"));
    let uid = status
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert_eq!(uid, r.current().configuration.identity.uid);
    assert_eq!(message("ZPK_INFO_V1\n"), format!("VERSION {version}\n"));
    assert!(fs::read_link(format!("/proc/{pid}/exe"))
        .unwrap()
        .starts_with(&p.layout.version));
    let state = fs::metadata(p.layout.data.join("state/greeting.txt")).unwrap();
    assert_eq!(state.uid(), uid);
    assert_eq!(state.mode() & 0o777, 0o600);
    let core_pid = fs::read_to_string("/run/zion-core.pid")
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let core = fs::metadata(format!("/proc/{core_pid}")).unwrap();
    assert_ne!(core.uid(), 0);
    assert_ne!(core.uid(), 65534);
    println!("Verified real native service, UID/capabilities, immutable version and persistent state; Core is dedicated non-root");
}
struct Crash;
impl Faults for Crash {
    fn checkpoint(&self, p: &str) -> Result<()> {
        if p == "reference-switched" {
            unsafe { libc::_exit(86) }
        }
        Ok(())
    }
}
fn fixtures(folder: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("build")
        .canonicalize()
        .unwrap();
    let folder = folder.canonicalize().unwrap();
    assert!(folder.starts_with(root));
    let path = folder.join("media/hello-1.0.0.zpk");
    let original = zstd::stream::decode_all(fs::File::open(path).unwrap()).unwrap();
    for attack in ["traversal", "architecture", "hash"] {
        let mut tar = original.clone();
        let mut pos = 0;
        while tar[pos] != 0 {
            let name = std::str::from_utf8(&tar[pos..pos + 100])
                .unwrap()
                .trim_end_matches('\0')
                .to_owned();
            let size = usize::from_str_radix(
                std::str::from_utf8(&tar[pos + 124..pos + 136])
                    .unwrap()
                    .trim_matches('\0')
                    .trim(),
                8,
            )
            .unwrap();
            if (attack == "architecture" && name == "manifest.toml")
                || (attack != "architecture" && name == "payload/bin/zion-hello")
            {
                if attack == "traversal" {
                    tar[pos..pos + 100].fill(0);
                    tar[pos..pos + 9].copy_from_slice(b"../escape");
                    tar[pos + 148..pos + 156].fill(b' ');
                    let sum: u32 = tar[pos..pos + 512].iter().map(|b| *b as u32).sum();
                    tar[pos + 148..pos + 156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
                } else if attack == "hash" {
                    tar[pos + 512 + 120] ^= 1;
                } else {
                    let data = &mut tar[pos + 512..pos + 512 + size];
                    let start = data.windows(6).position(|w| w == b"x86_64").unwrap();
                    data[start..start + 6].copy_from_slice(b"i686__");
                }
                break;
            }
            pos += 512 + size.div_ceil(512) * 512;
        }
        let bytes = zstd::stream::encode_all(std::io::Cursor::new(tar), 3).unwrap();
        let target = folder.join(format!("media/bad-{attack}.zpk"));
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .unwrap();
        f.write_all(&bytes).unwrap();
    }
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("fixtures") {
        fixtures(Path::new(args.get(1).unwrap()));
        return;
    }
    guard();
    match args[0].as_str() {
        "first" => {
            check("1.0.0");
            assert_eq!(
                message("ZPK_SETTINGS_V1 SET Genesis 0.7 persistent greeting\n"),
                "OK saved\n"
            );
        }
        "second" => {
            check("1.0.0");
            assert_eq!(
                message("ZPK_SETTINGS_V1 GET\n"),
                "OK Genesis 0.7 persistent greeting\n"
            );
        }
        "upgraded" => {
            check("1.1.0");
            assert_eq!(
                message("ZPK_SETTINGS_V1 GET\n"),
                "OK Genesis 0.7 persistent greeting\n"
            );
        }
        "third" => {
            check("1.1.0");
            assert_eq!(
                message("ZPK_SETTINGS_V1 GET\n"),
                "OK Genesis 0.7 persistent greeting\n"
            );
        }
        "rolled-back" => {
            check("1.0.0");
        }
        "corrupt-registry" => {
            let path = Path::new("/var/lib/zionos/packages/registry.sqlite3");
            let m = fs::symlink_metadata(path).unwrap();
            assert!(m.is_file() && m.uid() == 0 && m.nlink() == 1);
            fs::write(path, b"intentional disposable-VM corruption").unwrap();
            fs::File::open(path).unwrap().sync_all().unwrap();
        }
        "uninstalled" => {
            let m = Manager::open(Path::new("/"), Environment::Persistent, Alpine).unwrap();
            assert!(m.list().unwrap().is_empty());
            assert_eq!(
                fs::read_to_string("/var/lib/zionos/apps/org.zionos.zion-hello/state/greeting.txt")
                    .unwrap(),
                "Genesis 0.7 persistent greeting"
            );
            assert!(!Path::new("/opt/zionos/apps/org.zionos.zion-hello/versions").exists());
        }
        "ipc-negative" => {
            assert_eq!(
                ipc::Client::default()
                    .call(&Request::Status {}, None)
                    .unwrap()
                    .code,
                "IPC_DENIED"
            );
            let child = unsafe { libc::fork() };
            assert!(child >= 0);
            if child == 0 {
                let n = std::ffi::CString::new("zion-core").unwrap();
                unsafe {
                    let p = libc::getpwnam(n.as_ptr());
                    assert!(!p.is_null());
                    assert_eq!(libc::setgroups(0, std::ptr::null()), 0);
                    assert_eq!(libc::setgid((*p).pw_gid), 0);
                    assert_eq!(libc::setuid((*p).pw_uid), 0);
                }
                let r = ipc::Client::default()
                    .call(
                        &Request::Lifecycle {
                            auth: Credentials {
                                session: "a".repeat(64),
                                csrf: "b".repeat(64),
                            },
                            id: ID.into(),
                            action: Action::Start,
                        },
                        None,
                    )
                    .unwrap();
                assert_eq!(r.code, "UNAUTHENTICATED");
                let mut s = UnixStream::connect(zpk_control::protocol::SOCKET).unwrap();
                ipc::send(&mut s, br#"{"operation":"status","authorized":true}"#, None).unwrap();
                let (b, _) = ipc::receive(&mut s, 65536).unwrap();
                let r: zpk_control::protocol::Reply = serde_json::from_slice(&b).unwrap();
                assert!(!r.ok);
                unsafe { libc::_exit(0) }
            }
            let mut status = 0;
            unsafe { libc::waitpid(child, &mut status, 0) };
            assert_eq!(status, 0);
        }
        "interrupt" => {
            let out = Command::new(std::env::current_exe().unwrap())
                .arg("crash")
                .status()
                .unwrap();
            assert_eq!(out.code(), Some(86));
        }
        "crash" => {
            let p = Path::new(MEDIA).join("hello-1.2.0.zpk");
            let report =
                zpk_schema::inspect(fs::File::open(&p).unwrap(), zpk_schema::Limits::default())
                    .unwrap();
            let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine).unwrap();
            m.install(
                &p,
                true,
                &Review {
                    desktop_entry: None,
                    trusted_local_development: true,
                    approved_permissions: report.manifest.permissions,
                    start: true,
                    autostart: true,
                    unix_health: true,
                },
                &Crash,
            )
            .unwrap();
            panic!("checkpoint not reached");
        }
        _ => panic!("unknown verifier phase"),
    };
    println!("ZION_CENTER_PASS_{}", args[0]);
}
