//! Trusted administrator-side VM harness; never run against host services.
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::Command,
    time::Duration,
};
const CONFIG: &str = "/etc/zionos/apps/org.zionos.runtime-demo/runtime.toml";
fn cli(verb: &str) -> String {
    let out = Command::new("/usr/local/libexec/zpk-runtime")
        .args([verb, "--config", CONFIG])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{verb}: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn request(message: &[u8]) -> serde_json::Value {
    let name = zpk_runtime::config::identity_name("org.zionos.runtime-demo");
    let mut stream = UnixStream::connect(format!("/run/zionos/apps/{name}/service.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    stream.write_all(message).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    serde_json::from_str(&response).unwrap()
}
fn wait_ready() {
    for _ in 0..80 {
        let out = Command::new("/usr/local/libexec/zpk-runtime")
            .args(["healthcheck", "--config", CONFIG])
            .output()
            .unwrap();
        if out.status.success() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let root = "/var/log/zionos/apps/org.zionos.runtime-demo";
    panic!(
        "not ready: stdout={:?} stderr={:?}",
        fs::read_to_string(format!("{root}/stdout.log")),
        fs::read_to_string(format!("{root}/stderr.log"))
    );
}
fn main() {
    assert!(
        Path::new("/mnt/runtime-fixtures").is_dir(),
        "VM fixture mount required"
    );
    cli("start");
    wait_ready();
    let status: serde_json::Value = serde_json::from_str(&cli("status")).unwrap();
    assert_eq!(status["state"], "running");
    let pid = status["pid"].as_u64().unwrap();
    let report = request(b"REPORT\n");
    println!("runtime report: {report}");
    assert_eq!(report["uid"], 23101);
    assert_eq!(report["gid"], 23101);
    assert_eq!(report["fd_leak"], false);
    for key in [
        "caps_zero",
        "no_new_privs",
        "seccomp",
        "nas_read_denied",
        "nas_write_denied",
        "passwd_denied",
        "device_denied",
        "shell_denied",
        "ip_denied",
        "outbound_unix_denied",
        "memory_denied",
        "nofile_limited",
        "fsize_denied",
        "nproc_denied",
        "sysvipc_denied",
        "namespace_denied",
    ] {
        assert_eq!(report[key], true, "unenforced {key}");
    }
    assert!(cli("logs").contains("runtime fixture ready"));
    cli("stop");
    let status: serde_json::Value = serde_json::from_str(&cli("status")).unwrap();
    assert_eq!(status["state"], "stopped");
    assert!(fs::read_to_string(
        "/var/lib/zionos/apps/org.zionos.runtime-demo/state/clean-shutdown"
    )
    .unwrap()
    .contains("yes"));
    cli("start");
    wait_ready();
    let report2 = request(b"REPORT\n");
    assert!(report2["starts"].as_u64().unwrap() > report["starts"].as_u64().unwrap());
    cli("restart");
    wait_ready();
    let current: serde_json::Value = serde_json::from_str(&cli("status")).unwrap();
    let before = current["pid"].as_u64().unwrap();
    unsafe {
        libc::kill(before as i32, libc::SIGKILL);
    }
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        let s: serde_json::Value = serde_json::from_str(&cli("status")).unwrap();
        if s["state"] == "running" && s["pid"].as_u64() != Some(before) {
            break;
        }
    }
    wait_ready();
    let restarted: serde_json::Value = serde_json::from_str(&cli("status")).unwrap();
    assert_ne!(restarted["pid"].as_u64(), Some(before));
    assert!(restarted["restart_count"].as_u64().unwrap_or(0) > 0);
    let name = zpk_runtime::config::identity_name("org.zionos.runtime-demo");
    let mut socket = UnixStream::connect(format!("/run/zionos/apps/{name}/service.sock")).unwrap();
    socket.write_all(b"CPU\n").unwrap();
    drop(socket);
    std::thread::sleep(Duration::from_secs(5));
    wait_ready();
    let after: serde_json::Value = serde_json::from_str(&cli("status")).unwrap();
    assert_ne!(after["pid"], restarted["pid"]);
    let current = "/opt/zionos/apps/org.zionos.runtime-demo/current";
    fs::remove_file(current).unwrap();
    cli("stop");
    std::os::unix::fs::symlink("versions/1.0.0", current).unwrap();
    for setting in ["on", "off"] {
        let out = Command::new("/usr/local/libexec/zpk-runtime")
            .args(["autostart", "--config", CONFIG, setting])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "autostart: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let service = format!("zpk-{}", &name[3..]);
        assert_eq!(
            Path::new("/etc/runlevels/default").join(service).exists(),
            setting == "on"
        );
    }
    // Exhaust the configured crash budget; OpenRC must stop respawning and expose failure.
    cli("start");
    wait_ready();
    for attempt in 0..4 {
        let mut stream =
            UnixStream::connect(format!("/run/zionos/apps/{name}/service.sock")).unwrap();
        stream.write_all(b"CRASH\n").unwrap();
        drop(stream);
        std::thread::sleep(Duration::from_millis(1500));
        if attempt < 3 {
            wait_ready();
        }
    }
    std::thread::sleep(Duration::from_secs(1));
    let failed: serde_json::Value = serde_json::from_str(&cli("status")).unwrap();
    assert_eq!(failed["state"], "crashed", "budget exhaustion: {failed}");
    let mut c =
        zpk_runtime::config::RuntimeConfig::parse(&fs::read_to_string(CONFIG).unwrap()).unwrap();
    c.storage_mode = zpk_runtime::config::StorageMode::PersistentRequired;
    zpk_runtime::launch::save_config(Path::new(CONFIG), &c).unwrap();
    let verify = Command::new("/usr/local/libexec/zpk-runtime")
        .args(["verify", "--config", CONFIG])
        .output()
        .unwrap();
    assert!(
        !verify.status.success(),
        "live RAM must not be treated as persistent storage"
    );
    c.storage_mode = zpk_runtime::config::StorageMode::EphemeralDevelopment;
    zpk_runtime::launch::save_config(Path::new(CONFIG), &c).unwrap();
    assert_eq!(
        fs::read_to_string("/var/lib/zionos/nas-fixture/public.txt")
            .unwrap()
            .trim(),
        "fixture-must-remain-untouched"
    );
    println!("PASS: non-root, Landlock/seccomp/rlimits, clean lifecycle, OpenRC crash respawn, CPU kill, state and logs; initial pid={pid}");
}
