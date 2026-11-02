//! Shared fail-closed inspection. No destructive executor exists in 0.6.1.
use serde::Serialize;
use std::{collections::HashSet, fs, path::Path};
use tokio::{
    process::Command,
    time::{timeout, Duration},
};

pub const NETWORK_CONTRACT: &str = "QEMU-USERNET-LOCALHOST-V1";
pub const SERIAL_PREFIX: &str = "zl-";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Signature {
    Blank,
    Ext4,
    Other,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct Facts {
    pub vendor: String,
    pub expected_serial: String,
    pub serial: String,
    pub virtio: bool,
    pub device: (u32, u32),
    pub node: (u32, u32),
    pub sectors: u64,
    pub ro: String,
    pub removable: String,
    pub partitions: Vec<(u32, u32)>,
    pub holders: Vec<String>,
    pub slaves: Vec<String>,
    pub mounts: Vec<(u32, u32)>,
    pub swaps: Vec<(u32, u32)>,
}

pub fn valid_serial(s: &str) -> bool {
    s.strip_prefix(SERIAL_PREFIX)
        .is_some_and(|id| id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn dev_number(s: &str) -> Result<(u32, u32), String> {
    let (major, minor) = s.split_once(':').ok_or("invalid device number")?;
    if major.is_empty()
        || minor.is_empty()
        || !major
            .bytes()
            .chain(minor.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err("invalid device number".into());
    }
    Ok((
        major.parse().map_err(|_| "invalid major")?,
        minor.parse().map_err(|_| "invalid minor")?,
    ))
}

pub fn mounted_devices(data: &str) -> Result<Vec<(u32, u32)>, String> {
    let mut out = Vec::new();
    for line in data.lines() {
        let (head, tail) = line.split_once(" - ").ok_or("malformed mountinfo")?;
        let fields: Vec<_> = head.split_whitespace().collect();
        if fields.len() < 6 || tail.split_whitespace().count() < 3 {
            return Err("malformed mountinfo".into());
        }
        out.push(dev_number(fields[2])?);
    }
    if out.is_empty() {
        return Err("empty mountinfo".into());
    }
    Ok(out)
}

fn decode_path(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

pub fn swap_devices(
    data: &str,
    resolve: impl Fn(&str) -> Result<(u32, u32), String>,
) -> Result<Vec<(u32, u32)>, String> {
    let mut lines = data.lines();
    let header = lines.next().ok_or("empty swaps")?;
    let names: Vec<_> = header.split_whitespace().collect();
    if names != ["Filename", "Type", "Size", "Used", "Priority"] {
        return Err("invalid swaps header".into());
    }
    lines
        .map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() != 5
                || !["file", "partition"].contains(&f[1])
                || f[2].parse::<u64>().is_err()
                || f[3].parse::<u64>().is_err()
                || f[4].parse::<i64>().is_err()
            {
                return Err("malformed swap entry".into());
            }
            resolve(&decode_path(f[0]))
        })
        .collect()
}

pub fn validate(f: &Facts) -> Result<(), String> {
    if f.vendor != "QEMU" {
        return Err("not QEMU hardware".into());
    }
    if !valid_serial(&f.expected_serial) || f.serial != f.expected_serial {
        return Err("missing or mismatched launch-specific disk identity".into());
    }
    if !f.virtio || f.device != f.node {
        return Err("block node/sysfs identity mismatch".into());
    }
    if !(262_144..=16_777_216).contains(&f.sectors) {
        return Err("capacity outside 128 MiB..8 GiB".into());
    }
    if f.ro != "0" || f.removable != "0" {
        return Err("unknown/read-only/removable target".into());
    }
    if !f.partitions.is_empty() || !f.holders.is_empty() || !f.slaves.is_empty() {
        return Err("partitions, holders or slaves detected".into());
    }
    let mut devices: HashSet<_> = f.partitions.iter().copied().collect();
    devices.insert(f.device);
    if f.mounts.iter().chain(&f.swaps).any(|n| devices.contains(n)) {
        return Err("target belongs to a mounted filesystem or swap (including aliases)".into());
    }
    Ok(())
}

fn required(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    fs::read_to_string(path)
        .map(|s| s.trim_matches('\0').trim().to_owned())
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn names(path: impl AsRef<Path>) -> Result<Vec<String>, String> {
    fs::read_dir(path)
        .map_err(|e| format!("cannot enumerate topology: {e}"))?
        .map(|e| {
            e.map(|v| v.file_name().to_string_lossy().into_owned())
                .map_err(|e| format!("topology entry unavailable: {e}"))
        })
        .collect()
}

fn partition_numbers(root: &Path) -> Result<Vec<(u32, u32)>, String> {
    let mut partitions = Vec::new();
    for name in names(root)? {
        let child = root.join(name);
        let metadata = fs::metadata(&child)
            .map_err(|e| format!("cannot inspect topology entry {}: {e}", child.display()))?;
        // sysfs also contains regular attributes such as size/dev/ro. Only directories can be partitions.
        if !metadata.is_dir() {
            continue;
        }
        if child.join("partition").try_exists().map_err(|e| {
            format!(
                "cannot inspect partition attribute {}: {e}",
                child.display()
            )
        })? {
            partitions.push(dev_number(&required(child.join("dev"))?)?);
        }
    }
    Ok(partitions)
}

#[cfg(target_os = "linux")]
fn node_number(path: &Path, block_only: bool) -> Result<(u32, u32), String> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let m = fs::metadata(path).map_err(|e| format!("cannot resolve device/swap: {e}"))?;
    if block_only && !m.file_type().is_block_device() {
        return Err("target is not a block device".into());
    }
    let number = if m.file_type().is_block_device() {
        m.rdev()
    } else {
        m.dev()
    };
    Ok((libc::major(number), libc::minor(number)))
}
#[cfg(not(target_os = "linux"))]
fn node_number(_: &Path, _: bool) -> Result<(u32, u32), String> {
    Err("Linux required".into())
}

/// Root prepares these public, non-secret launch declarations from QEMU fw_cfg.
/// They are provenance for this lab launcher, not authentication against guest root.
pub fn trusted_runtime_file(name: &str) -> Result<String, String> {
    let path = Path::new("/run/zionos").join(name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(&path).map_err(|_| "launch declaration missing")?;
        if !m.is_file() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err("launch declaration must be a root-owned regular file".into());
        }
    }
    required(path)
}

pub fn collect() -> Result<Facts, String> {
    let vendor = required("/sys/class/dmi/id/sys_vendor")?;
    if vendor != "QEMU" {
        return Err("not QEMU hardware".into());
    }
    if required("/etc/zionos-lab")? != "ZIONOS-LAB-0.6.1" {
        return Err("not this laboratory image".into());
    }
    if trusted_runtime_file("network-contract")? != NETWORK_CONTRACT {
        return Err("not the isolated user-network lab".into());
    }
    let expected_serial = trusted_runtime_file("lab-serial")?;
    let root = Path::new("/sys/block/vdb");
    let canonical = fs::canonicalize(root).map_err(|_| "missing vdb sysfs")?;
    let partitions = partition_numbers(root)?;
    let facts = Facts {
        vendor,
        expected_serial,
        serial: required(root.join("serial"))?,
        virtio: canonical
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with("virtio")),
        device: dev_number(&required(root.join("dev"))?)?,
        node: node_number(Path::new("/dev/vdb"), true)?,
        sectors: required(root.join("size"))?
            .parse()
            .map_err(|_| "invalid capacity")?,
        ro: required(root.join("ro"))?,
        removable: required(root.join("removable"))?,
        partitions,
        holders: names(root.join("holders"))?,
        slaves: names(root.join("slaves"))?,
        mounts: mounted_devices(&required("/proc/self/mountinfo")?)?,
        swaps: swap_devices(&required("/proc/swaps")?, |s| {
            node_number(Path::new(s), false)
        })?,
    };
    validate(&facts)?;
    Ok(facts)
}

pub fn signature_result(code: Option<i32>, output: &[u8], errors: &[u8]) -> Signature {
    if !errors.is_empty() {
        return Signature::Unavailable;
    }
    match code {
        Some(2) if output.is_empty() => Signature::Blank,
        Some(0) if output.split(|b| *b == b'\n').any(|s| s == b"TYPE=ext4") => Signature::Ext4,
        Some(0) if !output.is_empty() => Signature::Other,
        _ => Signature::Unavailable,
    }
}

fn verified_signature(code: Option<i32>, output: &[u8], errors: &[u8], header: &[u8]) -> Signature {
    if header.len() != 4096 {
        return Signature::Unavailable;
    }
    let signature = signature_result(code, output, errors);
    if signature == Signature::Blank && header.iter().any(|b| *b != 0) {
        Signature::Other
    } else {
        signature
    }
}

pub async fn signature(expected: &Facts) -> Signature {
    #[cfg(target_os = "linux")]
    {
        use std::{
            fs::OpenOptions,
            io::Read,
            os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
            process::Stdio,
        };
        let device = expected.node;
        let job = tokio::task::spawn_blocking(move || -> std::io::Result<_> {
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_EXCL | libc::O_NOFOLLOW)
                .open("/dev/vdb")?;
            let metadata = file.metadata()?;
            if !metadata.file_type().is_block_device()
                || (libc::major(metadata.rdev()), libc::minor(metadata.rdev())) != device
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "target changed",
                ));
            }
            let mut header = [0u8; 4096];
            file.read_exact(&mut header)?;
            Ok((file, header))
        });
        let (file, header) = match timeout(Duration::from_secs(2), job).await {
            Ok(Ok(Ok(result))) => result,
            _ => return Signature::Unavailable,
        };
        let mut command = Command::new("blkid");
        command
            .args(["-p", "-o", "export", "/proc/self/fd/0"])
            .stdin(Stdio::from(file))
            .kill_on_drop(true);
        match timeout(Duration::from_secs(2), command.output()).await {
            Ok(Ok(output)) => verified_signature(
                output.status.code(),
                &output.stdout,
                &output.stderr,
                &header,
            ),
            _ => Signature::Unavailable,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = expected;
        Signature::Unavailable
    }
}

#[derive(Serialize)]
pub struct Inspection {
    pub target: &'static str,
    pub execution_enabled: bool,
    pub inspection_ready: bool,
    pub serial: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub reason: String,
}

pub fn inspect() -> Inspection {
    match collect() {
        Ok(f) => Inspection { target: "/dev/vdb", execution_enabled: false, inspection_ready: true,
            serial: Some(f.serial), capacity_bytes: f.sectors.checked_mul(512),
            reason: "Identity/topology inspected; signatures require the privileged CLI. Apply is disabled in 0.6.1.".into() },
        Err(e) => Inspection { target: "/dev/vdb", execution_enabled: false, inspection_ready: false,
            serial: None, capacity_bytes: None, reason: e },
    }
}

pub async fn console(mode: &str) -> i32 {
    let facts = match collect() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ZionOS LAB DENIED: {e}");
            return 2;
        }
    };
    let sig = signature(&facts).await;
    if mode == "--lab-plan" {
        let ready = sig == Signature::Blank;
        println!(
            "{}",
            serde_json::json!({"version":crate::VERSION,"mode":"read-only",
            "target":"/dev/vdb","serial":facts.serial,"capacity_bytes":facts.sectors*512,
            "signature":sig,"inspection_ready":ready,"execution_enabled":false,
            "warning":"No formatting executor in 0.6.1. A blank signature does not prove every byte is empty."})
        );
        return if ready { 0 } else { 2 };
    }
    if sig != Signature::Ext4 {
        eprintln!("ZionOS LAB DENIED: confirmed ext4 signature required");
        return 2;
    }
    // Keep an exclusive kernel claim and a fixed read-only descriptor through the child.
    #[cfg(target_os = "linux")]
    {
        use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_EXCL | libc::O_NOFOLLOW)
            .open("/dev/vdb")
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("ZionOS LAB DENIED: cannot claim target: {e}");
                return 2;
            }
        };
        use std::os::unix::fs::MetadataExt;
        let same_fd = file
            .metadata()
            .ok()
            .is_some_and(|m| (libc::major(m.rdev()), libc::minor(m.rdev())) == facts.node);
        if !same_fd || collect().is_err() {
            eprintln!("ZionOS LAB DENIED: identity changed");
            return 2;
        }
        check_pinned(file, std::ffi::OsStr::new("e2fsck")).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        2
    }
}

#[cfg(target_os = "linux")]
async fn check_pinned(file: fs::File, program: &std::ffi::OsStr) -> i32 {
    let mut command = Command::new(program);
    command
        .args(["-fn", "/proc/self/fd/0"])
        .stdin(std::process::Stdio::from(file))
        .kill_on_drop(true);
    match timeout(Duration::from_secs(60), command.status()).await {
        Ok(Ok(status)) => status.code().unwrap_or(8),
        _ => {
            eprintln!("Read-only diagnostic unavailable or timed out; no repair performed");
            8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partition_enumeration_skips_attributes_but_refuses_missing_topology() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "zion-sysfs-fixture-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("size"), b"2097152").unwrap();
        fs::create_dir(root.join("holders")).unwrap();
        fs::create_dir(root.join("slaves")).unwrap();
        assert!(partition_numbers(&root).unwrap().is_empty());
        fs::create_dir(root.join("vdb1")).unwrap();
        fs::write(root.join("vdb1/partition"), b"1").unwrap();
        fs::write(root.join("vdb1/dev"), b"253:17").unwrap();
        assert_eq!(partition_numbers(&root).unwrap(), vec![(253, 17)]);
        fs::remove_file(root.join("vdb1/dev")).unwrap();
        assert!(partition_numbers(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
        assert!(partition_numbers(&root).is_err());
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn recovery_reads_pinned_descriptor_even_after_path_replacement() {
        use std::os::unix::fs::PermissionsExt;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("zion-fd-test-{}-{unique}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("target");
        fs::write(&target, b"original fixture").unwrap();
        let file = fs::File::open(&target).unwrap();
        fs::rename(&target, directory.join("moved")).unwrap();
        fs::write(&target, b"replacement fixture").unwrap();
        let output = directory.join("read-result");
        let program = directory.join("mock-check");
        fs::write(&program,format!("#!/bin/sh\n[ \"$1\" = -fn ] || exit 99\n[ \"$2\" = /proc/self/fd/0 ] || exit 99\ncat \"$2\" > '{}'\n",output.display())).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(check_pinned(file, program.as_os_str()).await, 0);
        assert_eq!(fs::read(&output).unwrap(), b"original fixture");
        fs::remove_dir_all(directory).unwrap();
    }
    fn good() -> Facts {
        Facts {
            vendor: "QEMU".into(),
            expected_serial: "zl-0123456789abcdef".into(),
            serial: "zl-0123456789abcdef".into(),
            virtio: true,
            device: (254, 16),
            node: (254, 16),
            sectors: 2_097_152,
            ro: "0".into(),
            removable: "0".into(),
            partitions: vec![],
            holders: vec![],
            slaves: vec![],
            mounts: vec![(0, 1)],
            swaps: vec![],
        }
    }
    #[test]
    fn valid_lab_identity_and_empty_topology() {
        assert!(validate(&good()).is_ok());
    }
    #[test]
    fn denies_unsafe_facts() {
        let mut cases = Vec::new();
        let mut f = good();
        f.serial.push('x');
        cases.push(f);
        let mut f = good();
        f.expected_serial = "".into();
        cases.push(f);
        let mut f = good();
        f.vendor = "real hardware".into();
        cases.push(f);
        let mut f = good();
        f.virtio = false;
        cases.push(f);
        let mut f = good();
        f.node = (8, 0);
        cases.push(f);
        let mut f = good();
        f.sectors = u64::MAX;
        cases.push(f);
        let mut f = good();
        f.ro = "".into();
        cases.push(f);
        let mut f = good();
        f.removable = "1".into();
        cases.push(f);
        let mut f = good();
        f.partitions.push((254, 17));
        cases.push(f);
        let mut f = good();
        f.holders.push("dm-0".into());
        cases.push(f);
        let mut f = good();
        f.slaves.push("sda".into());
        cases.push(f);
        let mut f = good();
        f.mounts.push(f.device);
        cases.push(f);
        let mut f = good();
        f.swaps.push(f.device);
        cases.push(f);
        for (index, f) in cases.iter().enumerate() {
            assert!(validate(f).is_err(), "case {index}");
        }
    }
    #[test]
    fn mount_aliases_and_system_mounts_use_device_numbers() {
        for source in ["UUID=xxx", "/dev/root", "/dev/strange-alias"] {
            let mut f = good();
            f.mounts = mounted_devices(&format!("1 0 254:16 / / rw - ext4 {source} rw\n")).unwrap();
            assert!(validate(&f).is_err());
        }
        assert!(mounted_devices("").is_err());
        assert!(mounted_devices("incomplete").is_err());
        assert!(dev_number("8:0:1").is_err());
        assert!(dev_number("+8:1").is_err());
    }
    #[test]
    fn swap_alias_and_file_resolve_to_their_real_device() {
        let header = "Filename Type Size Used Priority\n";
        for kind in ["file", "partition"] {
            let data = format!("{header}/swap\\040file {kind} 1024 0 -2\n");
            let mut f = good();
            f.swaps = swap_devices(&data, |s| {
                assert_eq!(s, "/swap file");
                Ok((254, 16))
            })
            .unwrap();
            assert!(validate(&f).is_err());
        }
        assert!(swap_devices("", |_| Ok((0, 0))).is_err());
        assert!(
            swap_devices(&format!("{header}/gone partition 1 0 -1\n"), |_| Err(
                "missing".into()
            ))
            .is_err()
        );
    }
    #[test]
    fn blkid_failures_and_ambiguity_are_never_blank() {
        assert_eq!(signature_result(Some(2), b"", b""), Signature::Blank);
        for code in [None, Some(1), Some(4), Some(8)] {
            assert_eq!(signature_result(code, b"", b""), Signature::Unavailable);
        }
        assert_eq!(
            signature_result(Some(2), b"TYPE=ext4", b""),
            Signature::Unavailable
        );
        assert_eq!(
            signature_result(Some(8), b"TYPE=ext4", b""),
            Signature::Unavailable
        );
        assert_eq!(
            signature_result(Some(0), b"TYPE=ext4\n", b""),
            Signature::Ext4
        );
        assert_eq!(
            signature_result(Some(2), b"", b"Permission denied"),
            Signature::Unavailable
        );
        assert_eq!(
            verified_signature(Some(2), b"", b"", &[0; 4096]),
            Signature::Blank
        );
        assert_eq!(
            verified_signature(Some(2), b"", b"", &[0; 4095]),
            Signature::Unavailable
        );
        assert_eq!(
            verified_signature(Some(2), b"", b"", &[1; 4096]),
            Signature::Other
        );
    }
}
