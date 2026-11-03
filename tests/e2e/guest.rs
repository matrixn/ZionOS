//! Trusted test coordinator; never an installer hook or a production CLI extension.
use std::{
    fs,
    io::{Read, Write},
    os::unix::{fs::MetadataExt, net::UnixStream},
    path::Path,
    process::Command,
    time::Duration,
};
use zpk_manager::{
    filesystem::Environment,
    host::Alpine,
    manager::{Faults, Manager},
    model::Review,
    Result,
};
use zpk_runtime::{backend::ServiceBackend, config::identity_name};
const ID: &str = "org.zionos.zion-hello";
const MEDIA: &str = "/mnt/zion-e2e-fixtures";
const DATA: &str = "/var/lib/zionos/apps/org.zionos.zion-hello/state";
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
        "ZPKE2ETEST01"
    );
    assert_eq!(
        fs::read_to_string("/etc/zionos-lab").unwrap().trim(),
        "ZIONOS-LAB-0.6.1"
    );
    assert!(Path::new(MEDIA).is_dir());
    let mounts = fs::read_to_string("/proc/mounts").unwrap();
    assert!(mounts
        .lines()
        .any(|l| l.starts_with("/dev/vda /mnt/zion-e2e-persistence ext4 rw,")));
}
fn run(args: &[&str]) -> serde_json::Value {
    let out = Command::new("/usr/local/bin/zpk")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn change(verb: &str, version: &str) -> std::process::Output {
    Command::new("/usr/local/bin/zpk")
        .args([
            verb,
            &format!("{MEDIA}/hello-{version}.zpk"),
            "--trust-local-development",
            "--permissions",
            "desktop-ui,notifications,background-service,persistent-data",
            "--start",
            "--unix-health",
        ])
        .output()
        .unwrap()
}
fn message(request: &str) -> String {
    let socket = format!("/run/zionos/apps/{}/service.sock", identity_name(ID));
    let mut s = UnixStream::connect(socket).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut result = String::new();
    s.take(2049).read_to_string(&mut result).unwrap();
    assert!(result.len() <= 2048);
    result
}
fn check(version: &str) -> u32 {
    assert_eq!(run(&["info", ID])["active"], version);
    let state = run(&["status", ID]);
    assert_eq!(state["state"], "running");
    let pid = state["pid"].as_u64().unwrap();
    let proc = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let uid = proc
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert!(uid >= 10000);
    assert!(proc.lines().any(|l| l == "NoNewPrivs:\t1"));
    assert!(proc.lines().any(|l| l == "Seccomp:\t2"));
    assert!(proc.lines().any(|l| l == "CapEff:\t0000000000000000"));
    let info = run(&["info", ID]);
    let conf = &info["versions"][version]["configuration"];
    assert_eq!(conf["identity"]["uid"], uid);
    for name in ["starts", "greeting.txt"] {
        let m = fs::metadata(format!("{DATA}/{name}")).unwrap();
        assert_eq!(m.uid(), uid);
        assert_eq!(m.mode() & 0o777, 0o600);
    }
    let exe = fs::read_link(format!("/proc/{pid}/exe")).unwrap();
    assert!(exe
        .to_string_lossy()
        .contains(&format!("/versions/{version}/payload/bin/zion-hello")));
    assert_eq!(fs::metadata(&exe).unwrap().uid(), 0);
    assert_eq!(fs::metadata(&exe).unwrap().mode() & 0o022, 0);
    assert_eq!(message("ZPK_INFO_V1\n"), format!("VERSION {version}\n"));
    assert!(run(&["logs", ID])["data"]
        .as_str()
        .unwrap()
        .contains("native backend ready"));
    run(&["verify", ID]);
    let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine).unwrap();
    m.reconcile().unwrap();
    let record = m.info(ID).unwrap();
    let p = record
        .current()
        .configuration
        .clone()
        .plan(record.current().parsed().unwrap(), Path::new("/"))
        .unwrap();
    assert!(
        zpk_runtime::backend::health(&p, &Alpine.status(&p).unwrap())
            .unwrap()
            .healthy
    );
    uid
}
struct Crash(&'static str);
impl Faults for Crash {
    fn checkpoint(&self, point: &str) -> Result<()> {
        if point == self.0 {
            unsafe {
                libc::_exit(86);
            }
        }
        Ok(())
    }
}
fn interrupt(version: &str, point: &str) {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["crash", version, point])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
}
fn main() {
    guard();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let phase = args.first().expect("phase required");
    if phase == "crash" {
        let point = match args[2].as_str() {
            "staged" => "staged",
            "reference-switched" => "reference-switched",
            _ => panic!("invalid fixture checkpoint"),
        };
        assert!(["1.0.0", "1.2.0"].contains(&args[1].as_str()));
        let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine).unwrap();
        let p = Path::new(MEDIA).join(format!("hello-{}.zpk", args[1]));
        let manifest =
            zpk_schema::inspect(fs::File::open(&p).unwrap(), zpk_schema::Limits::default())
                .unwrap()
                .manifest;
        let review = Review {
            trusted_local_development: true,
            approved_permissions: manifest.permissions,
            start: true,
            autostart: false,
            unix_health: true,
            desktop_entry: None,
        };
        m.install(&p, args[1] != "1.0.0", &review, &Crash(point))
            .unwrap();
        panic!("checkpoint not reached");
    }
    match phase.as_str() {
        "settings-get" => {
            print!("{}", message("ZPK_SETTINGS_V1 GET\n"));
        }
        "settings-set" => {
            let value = args.get(1).expect("greeting required");
            assert!(
                !value.trim().is_empty()
                    && value.len() <= 160
                    && !value.chars().any(char::is_control)
            );
            assert_eq!(
                message(&format!("ZPK_SETTINGS_V1 SET {value}\n")),
                "OK saved\n"
            );
        }
        "first" => {
            assert!(run(&["list"]).as_object().unwrap().is_empty());
            interrupt("1.0.0", "staged");
            assert!(run(&["list"]).as_object().unwrap().is_empty());
            let out = change("install", "1.0.0");
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let uid = check("1.0.0");
            assert_eq!(message("ZPK_SETTINGS_V1 GET\n"), "OK Welcome to ZionOS\n");
            assert_eq!(
                message("ZPK_SETTINGS_V1 SET Persistent hello from the first boot\n"),
                "OK saved\n"
            );
            assert_eq!(message("ZPK_SETTINGS_V1 SET \n"), "ERROR invalid request\n");
            assert_eq!(
                message("ZPK_SETTINGS_V1 DELETE /etc/passwd\n"),
                "ERROR invalid request\n"
            );
            assert_eq!(
                message(&format!("ZPK_SETTINGS_V1 SET {}\n", "x".repeat(161))),
                "ERROR invalid request\n"
            );
            fs::write("/mnt/zion-e2e-persistence/identity", uid.to_string()).unwrap();
            fs::write(
                "/mnt/zion-e2e-persistence/starts",
                fs::read(format!("{DATA}/starts")).unwrap(),
            )
            .unwrap();
        }
        "second" => {
            assert_eq!(run(&["list"]).as_object().unwrap().len(), 1);
            assert_eq!(
                fs::read_to_string(format!("{DATA}/greeting.txt")).unwrap(),
                "Persistent hello from the first boot"
            );
            run(&["start", ID]);
            let uid = check("1.0.0");
            assert_eq!(
                uid.to_string(),
                fs::read_to_string("/mnt/zion-e2e-persistence/identity").unwrap()
            );
            let before = fs::read_to_string("/mnt/zion-e2e-persistence/starts")
                .unwrap()
                .parse::<u64>()
                .unwrap();
            assert!(
                fs::read_to_string(format!("{DATA}/starts"))
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
                    > before
            );
            assert_eq!(
                message("ZPK_SETTINGS_V1 GET\n"),
                "OK Persistent hello from the first boot\n"
            );
            run(&["stop", ID]);
            assert_eq!(run(&["status", ID])["state"], "stopped");
            run(&["start", ID]);
            run(&["restart", ID]);
            check("1.0.0");
        }
        "upgrade" => {
            let out = change("upgrade", "1.1.0");
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            check("1.1.0");
            assert_eq!(
                message("ZPK_SETTINGS_V1 GET\n"),
                "OK Persistent hello from the first boot\n"
            );
        }
        "failure" => {
            let out = change("upgrade", "1.2.0");
            assert!(!out.status.success());
            check("1.1.0");
            assert_eq!(
                message("ZPK_SETTINGS_V1 GET\n"),
                "OK Persistent hello from the first boot\n"
            );
            let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine).unwrap();
            m.reconcile().unwrap();
            assert_eq!(
                m.database.operation_states().unwrap().last().unwrap().1,
                "FAILED"
            );
        }
        "interrupt" => {
            interrupt("1.2.0", "reference-switched");
        }
        "third" => {
            // CLI startup reconciliation restores the executable reference and native service.
            assert_eq!(run(&["list"]).as_object().unwrap().len(), 1);
            check("1.1.0");
            assert_eq!(
                message("ZPK_SETTINGS_V1 GET\n"),
                "OK Persistent hello from the first boot\n"
            );
            let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine).unwrap();
            m.reconcile().unwrap();
            assert!(m.database.pending(Path::new("/")).unwrap().is_empty());
            assert_eq!(
                m.database
                    .operation_states()
                    .unwrap()
                    .iter()
                    .filter(|(_, s)| s == "FAILED")
                    .count(),
                3
            );
        }
        "uninstall" => {
            let before = fs::read(format!("{DATA}/greeting.txt")).unwrap();
            let pid = run(&["status", ID])["pid"].as_u64().unwrap();
            run(&["uninstall", ID]);
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
            assert!(run(&["list"]).as_object().unwrap().is_empty());
            assert_eq!(fs::read(format!("{DATA}/greeting.txt")).unwrap(), before);
            assert!(!Path::new("/opt/zionos/apps/org.zionos.zion-hello/versions").exists());
            assert!(!Path::new(&format!("/etc/init.d/zpk-{}", &identity_name(ID)[3..])).exists());
        }
        _ => panic!("unknown fixture phase"),
    }
    println!("ZION_E2E_PASS_{phase}");
}
