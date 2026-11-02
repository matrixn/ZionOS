//! Owned administrator test fixture. Runs only inside the guarded persistent QEMU guest.
use std::{fs, os::unix::fs::MetadataExt, path::Path, process::Command};
const RUST: &str = "org.zionos.zion-hello-rust";
const C: &str = "org.zionos.zion-hello-c";
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
fn change(verb: &str, file: &str) -> serde_json::Value {
    run(&[
        verb,
        file,
        "--trust-local-development",
        "--permissions",
        "background-service,persistent-data",
        "--start",
        "--unix-health",
    ])
}
fn starts(id: &str) -> u64 {
    fs::read_to_string(format!("/var/lib/zionos/apps/{id}/state/starts"))
        .unwrap()
        .parse()
        .unwrap()
}
fn check(id: &str) {
    let status = run(&["status", id]);
    assert_eq!(status["state"], "running");
    let pid = status["pid"].as_u64().unwrap();
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
    assert_eq!(
        fs::metadata(format!("/var/lib/zionos/apps/{id}/state/starts"))
            .unwrap()
            .uid(),
        uid
    );
    assert!(run(&["logs", id])["data"]
        .as_str()
        .unwrap()
        .contains("hello"));
    run(&["verify", id]);
}
fn main() {
    assert!(Path::new("/mnt/zpk-manager-fixtures").is_dir());
    let phase = std::env::args().nth(1).unwrap();
    if phase == "first" {
        change("install", "/mnt/zpk-manager-fixtures/rust-1.0.0.zpk");
        change("install", "/mnt/zpk-manager-fixtures/c-1.0.0.zpk");
        check(RUST);
        check(C);
        run(&["stop", RUST]);
        assert_eq!(run(&["status", RUST])["state"], "stopped");
        run(&["start", RUST]);
        run(&["restart", RUST]);
        check(RUST);
        let previous = starts(RUST);
        change("upgrade", "/mnt/zpk-manager-fixtures/rust-1.1.0.zpk");
        assert_eq!(run(&["info", RUST])["active"], "1.1.0");
        assert!(starts(RUST) > previous);
        run(&["rollback", RUST]);
        assert_eq!(run(&["info", RUST])["active"], "1.0.0");
        check(RUST);
        let fail = Command::new("/usr/local/bin/zpk")
            .args([
                "upgrade",
                "/mnt/zpk-manager-fixtures/rust-1.2.0.zpk",
                "--trust-local-development",
                "--permissions",
                "background-service,persistent-data",
                "--start",
                "--unix-health",
            ])
            .output()
            .unwrap();
        assert!(!fail.status.success());
        assert_eq!(run(&["info", RUST])["active"], "1.0.0");
        check(RUST);
        fs::write(
            "/mnt/zpk-manager-persistence/starts-before-reboot",
            starts(RUST).to_string(),
        )
        .unwrap();
        fs::write("/mnt/zpk-manager-persistence/phase-one-done", "verified").unwrap();
        run(&["stop", RUST]);
        run(&["stop", C]);
    } else if phase == "second" {
        assert_eq!(run(&["list"]).as_object().unwrap().len(), 2);
        run(&["verify", RUST]);
        run(&["verify", C]);
        let previous = fs::read_to_string("/mnt/zpk-manager-persistence/starts-before-reboot")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        run(&["start", RUST]);
        run(&["start", C]);
        check(RUST);
        check(C);
        assert!(starts(RUST) > previous);
        for id in [RUST, C] {
            let retained = starts(id);
            assert_eq!(run(&["repair", "--plan", id])["changes_applied"], false);
            run(&["uninstall", id]);
            assert_eq!(starts(id), retained);
            assert!(!Path::new(&format!("/opt/zionos/apps/{id}/versions")).exists());
        }
        assert!(run(&["list"]).as_object().unwrap().is_empty());
        fs::write("/mnt/zpk-manager-persistence/phase-two-done", "verified").unwrap();
    } else {
        assert!(run(&["list"]).as_object().unwrap().is_empty());
        let prior_rust = starts(RUST);
        let prior_c = starts(C);
        // Reverse installation order after volatile NSS disappears. Retained UID metadata
        // must win over the first free account number, or the app cannot read its 0700 slot.
        change("install", "/mnt/zpk-manager-fixtures/c-1.0.0.zpk");
        change("install", "/mnt/zpk-manager-fixtures/rust-1.0.0.zpk");
        check(C);
        check(RUST);
        assert!(starts(C) > prior_c);
        assert!(starts(RUST) > prior_rust);
        run(&["uninstall", C]);
        run(&["uninstall", RUST]);
        assert!(run(&["list"]).as_object().unwrap().is_empty());
    }
    println!("ZPK_MANAGER_GUEST_PASS_{phase}");
}
