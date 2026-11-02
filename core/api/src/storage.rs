//! Read-only discovery. No disk formatting, mount, partitioning or writes.
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{process::Command, time::timeout};

const MAX_DISKS: usize = 32;
const MAX_PARTITIONS: usize = 128;
const SMART_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Serialize)]
pub struct StorageSnapshot {
    pub version: &'static str,
    pub mode: &'static str,
    pub collected_at_ms: u128,
    pub disks: Vec<DiskDetail>,
    pub mounts: Vec<MountInfo>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct DiskDetail {
    pub name: String,
    pub model: String,
    pub size_bytes: u64,
    pub rotational: bool,
    pub removable: bool,
    pub read_only: bool,
    pub transport: String,
    pub logical_sector_bytes: Option<u64>,
    pub physical_sector_bytes: Option<u64>,
    pub partitions: Vec<Partition>,
    pub smart: SmartInfo,
}

#[derive(Clone, Serialize)]
pub struct Partition {
    pub name: String,
    pub size_bytes: u64,
    pub start_bytes: u64,
    pub major_minor: String,
    pub filesystem: Option<String>,
    pub uuid: Option<String>,
    pub label: Option<String>,
    pub mounts: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct SmartInfo {
    pub status: String, // passed, failed, unknown, unavailable, standby
    pub temperature_c: Option<i64>,
    pub power_on_hours: Option<u64>,
    pub percentage_used: Option<u64>,
    pub reason: Option<String>,
}

impl SmartInfo {
    fn unavailable(reason: &str) -> Self {
        Self {
            status: "unavailable".into(),
            temperature_c: None,
            power_on_hours: None,
            percentage_used: None,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct MountInfo {
    pub major_minor: String,
    pub mountpoint: String,
    pub source: String,
    pub filesystem: String,
    pub read_only: bool,
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
}

fn read_text(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}
fn read_num(path: impl AsRef<Path>) -> Option<u64> {
    read_text(path).parse().ok()
}
fn disk_name_is_safe(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn escape_decode(s: &str) -> String {
    // mountinfo escapes literal spaces, tabs, newlines and backslashes.
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn parse_mountinfo(data: &str) -> Vec<MountInfo> {
    let mut mounts = Vec::new();
    for line in data.lines() {
        let Some((head, tail)) = line.split_once(" - ") else {
            continue;
        };
        let a: Vec<_> = head.split_whitespace().collect();
        let b: Vec<_> = tail.split_whitespace().collect();
        if a.len() < 6 || b.len() < 2 {
            continue;
        }
        // We display only mounted block filesystems. Virtual filesystems (proc, sysfs, etc.) are omitted.
        if !b[1].starts_with("/dev/") {
            continue;
        }
        let point = escape_decode(a[4]);
        let (total_bytes, available_bytes) = filesystem_usage(&point).unwrap_or((None, None));
        mounts.push(MountInfo {
            major_minor: a[2].to_string(),
            mountpoint: point,
            source: escape_decode(b[1]),
            filesystem: b[0].to_string(),
            read_only: a[5].split(',').any(|o| o == "ro"),
            total_bytes,
            available_bytes,
        });
    }
    mounts
}

#[cfg(unix)]
fn filesystem_usage(path: &str) -> Option<(Option<u64>, Option<u64>)> {
    use std::ffi::CString;
    let raw = CString::new(path).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // Safe because statvfs initializes the struct on success and raw is NUL-terminated.
    let rc = unsafe { libc::statvfs(raw.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    let total = (stat.f_blocks as u128).saturating_mul(stat.f_frsize as u128);
    let avail = (stat.f_bavail as u128).saturating_mul(stat.f_frsize as u128);
    Some((
        Some(total.min(u64::MAX as u128) as u64),
        Some(avail.min(u64::MAX as u128) as u64),
    ))
}
#[cfg(not(unix))]
fn filesystem_usage(_path: &str) -> Option<(Option<u64>, Option<u64>)> {
    None
}

fn transport(name: &str, root: &Path) -> String {
    if name.starts_with("nvme") {
        return "NVMe".into();
    }
    if name.starts_with("vd") {
        return "VirtIO".into();
    }
    if name.starts_with("md") {
        return "Software RAID".into();
    }
    if name.starts_with("dm-") {
        return "Device mapper".into();
    }
    if name.starts_with("loop") {
        return "Loop".into();
    }
    let p = fs::canonicalize(root.join("device"))
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if p.contains("/usb") {
        "USB".into()
    } else if p.contains("/ata") || p.contains("/host") {
        "SATA / SCSI".into()
    } else {
        "Unknown".into()
    }
}

fn scan_partitions(root: &Path, mounts: &[MountInfo]) -> Vec<Partition> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_PARTITIONS {
            break;
        }
        let path = entry.path();
        if !path.join("partition").exists() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !disk_name_is_safe(&name) {
            continue;
        }
        let major_minor = read_text(path.join("dev"));
        let matching: Vec<_> = mounts
            .iter()
            .filter(|m| m.major_minor == major_minor)
            .collect();
        out.push(Partition {
            name,
            size_bytes: read_num(path.join("size")).unwrap_or(0).saturating_mul(512),
            start_bytes: read_num(path.join("start"))
                .unwrap_or(0)
                .saturating_mul(512),
            major_minor,
            filesystem: matching.first().map(|m| m.filesystem.clone()),
            uuid: None,
            label: None,
            mounts: matching.iter().map(|m| m.mountpoint.clone()).collect(),
        });
    }
    out.sort_by_key(|a| a.start_bytes);
    out
}

async fn probe_blkid(part: &mut Partition) {
    // blkid reads superblocks for inventory; no filesystem changes are made.
    let path = format!("/dev/{}", part.name);
    if !Path::new(&path).exists() {
        return;
    }
    let mut command = Command::new("blkid");
    command
        .args(["-p", "-o", "export", &path])
        .kill_on_drop(true);
    if let Ok(Ok(out)) = timeout(Duration::from_secs(2), command.output()).await {
        for row in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((key, value)) = row.split_once('=') {
                match key {
                    "TYPE" => {
                        if part.filesystem.is_none() {
                            part.filesystem = Some(value.to_string());
                        }
                    }
                    "UUID" => part.uuid = Some(value.to_string()),
                    "LABEL" => part.label = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }
}

fn parse_smart(info: &Value) -> SmartInfo {
    let passed = info
        .pointer("/smart_status/passed")
        .and_then(Value::as_bool);
    let temperature_c = info.pointer("/temperature/current").and_then(Value::as_i64);
    let hours = info
        .pointer("/power_on_time/hours")
        .and_then(Value::as_u64)
        .or_else(|| {
            info.pointer("/nvme_smart_health_information_log/power_on_hours")
                .and_then(Value::as_u64)
        });
    let percentage = info
        .pointer("/nvme_smart_health_information_log/percentage_used")
        .and_then(Value::as_u64);
    SmartInfo {
        status: match passed {
            Some(true) => "passed",
            Some(false) => "failed",
            None => "unknown",
        }
        .into(),
        temperature_c,
        power_on_hours: hours,
        percentage_used: percentage,
        reason: None,
    }
}

fn smart_result(info: &Value) -> SmartInfo {
    let Some(status) = info
        .pointer("/smartctl/exit_status")
        .and_then(Value::as_u64)
    else {
        return SmartInfo::unavailable("SMART exit status is missing");
    };
    // -n standby,3 gives standby a distinct status from device-open failure (2).
    if status == 3 {
        return SmartInfo {
            status: "standby".into(),
            temperature_c: None,
            power_on_hours: None,
            percentage_used: None,
            reason: Some("Discul este în standby".into()),
        };
    }
    if status & 3 != 0 {
        return SmartInfo::unavailable("Dispozitivul nu poate fi accesat prin SMART");
    }
    parse_smart(info)
}

async fn probe_smart(disk: &str) -> SmartInfo {
    // Do not attempt SMART on logical/virtual devices. Unsupported devices are explicitly unknown.
    if disk.starts_with("vd")
        || disk.starts_with("loop")
        || disk.starts_with("ram")
        || disk.starts_with("dm-")
        || disk.starts_with("md")
    {
        return SmartInfo::unavailable("Dispozitiv virtual sau logic; SMART nu este expus aici");
    }
    let path = format!("/dev/{disk}");
    if !Path::new(&path).exists() {
        return SmartInfo::unavailable("Nodul dispozitivului nu este disponibil");
    }
    let mut cmd = Command::new("smartctl");
    // -n standby minimizes wake-ups of sleeping ATA drives.
    cmd.args(["-j", "-H", "-A", "-i", "-n", "standby,3", &path])
        .kill_on_drop(true);
    match timeout(SMART_TIMEOUT, cmd.output()).await {
        Err(_) => SmartInfo::unavailable("SMART nu a răspuns în 3 secunde"),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            SmartInfo::unavailable("smartmontools nu este instalat")
        }
        Ok(Err(_)) => SmartInfo::unavailable("Nu se poate executa smartctl"),
        Ok(Ok(out)) => {
            let Ok(json) = serde_json::from_slice::<Value>(&out.stdout) else {
                return SmartInfo::unavailable("Dispozitivul nu oferă date SMART compatibile");
            };
            smart_result(&json)
        }
    }
}

pub async fn snapshot() -> StorageSnapshot {
    snapshot_inner(true).await
}

/// Fast storage inventory for pool topology, deliberately without SMART probing.
pub async fn snapshot_without_smart() -> StorageSnapshot {
    snapshot_inner(false).await
}

async fn snapshot_inner(with_smart: bool) -> StorageSnapshot {
    let mut warnings = Vec::new();
    let mounts = parse_mountinfo(&read_text("/proc/self/mountinfo"));
    let mut disks = Vec::new();
    match fs::read_dir("/sys/block") {
        Err(_) => warnings.push("/sys/block nu este disponibil".into()),
        Ok(entries) => {
            for entry in entries.flatten() {
                if disks.len() >= MAX_DISKS {
                    warnings.push("Lista a fost limitată la 32 de dispozitive".into());
                    break;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if !disk_name_is_safe(&name)
                    || ["loop", "ram", "zram", "fd"]
                        .iter()
                        .any(|p| name.starts_with(p))
                {
                    continue;
                }
                let path = entry.path();
                let mut partitions = scan_partitions(&path, &mounts);
                for partition in &mut partitions {
                    probe_blkid(partition).await;
                }
                let smart = if with_smart {
                    probe_smart(&name).await
                } else {
                    SmartInfo::unavailable("SMART is not requested for pool topology")
                };
                disks.push(DiskDetail {
                    model: read_text(path.join("device/model")),
                    size_bytes: read_num(path.join("size")).unwrap_or(0).saturating_mul(512),
                    rotational: read_num(path.join("queue/rotational")).unwrap_or(0) != 0,
                    removable: read_num(path.join("removable")).unwrap_or(0) != 0,
                    read_only: read_num(path.join("ro")).unwrap_or(0) != 0,
                    logical_sector_bytes: read_num(path.join("queue/logical_block_size")),
                    physical_sector_bytes: read_num(path.join("queue/physical_block_size")),
                    transport: transport(&name, &path),
                    name,
                    partitions,
                    smart,
                });
            }
        }
    }
    disks.sort_by(|a, b| a.name.cmp(&b.name));
    StorageSnapshot {
        version: "0.4.0",
        mode: "read-only",
        collected_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        disks,
        mounts,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_valid_disk_names() {
        assert!(disk_name_is_safe("nvme0n1"));
        assert!(disk_name_is_safe("dm-0"));
        assert!(!disk_name_is_safe("../etc/passwd"));
        assert!(!disk_name_is_safe("sda;shutdown"));
    }
    #[test]
    fn test_mountinfo_parser() {
        let sample = "35 24 8:1 / /mnt/My\\040Files rw,relatime - ext4 /dev/sda1 rw\n1 0 0:3 / /proc rw - proc proc rw\n";
        let found = parse_mountinfo(sample);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].major_minor, "8:1");
        assert_eq!(found[0].mountpoint, "/mnt/My Files");
        assert_eq!(found[0].filesystem, "ext4");
    }
    #[test]
    fn test_smart_json() {
        let json = serde_json::json!({"smart_status":{"passed":true}, "temperature":{"current":37}, "power_on_time":{"hours":127}});
        let smart = parse_smart(&json);
        assert_eq!(smart.status, "passed");
        assert_eq!(smart.temperature_c, Some(37));
        assert_eq!(smart.power_on_hours, Some(127));
    }
    #[test]
    fn smart_access_errors_are_not_standby() {
        for status in [1, 2] {
            let json = serde_json::json!({"smartctl":{"exit_status":status}});
            assert_eq!(smart_result(&json).status, "unavailable");
        }
        let standby = serde_json::json!({"smartctl":{"exit_status":3}});
        assert_eq!(smart_result(&standby).status, "standby");
        assert_eq!(smart_result(&serde_json::json!({})).status, "unavailable");
    }

    #[test]
    fn smart_health_failure_is_preserved() {
        let failed = serde_json::json!({
            "smartctl":{"exit_status":8}, "smart_status":{"passed":false}
        });
        assert_eq!(smart_result(&failed).status, "failed");
    }
    #[test]
    fn test_sector_conversion() {
        assert_eq!(2048_u64.saturating_mul(512), 1_048_576);
    }
}
