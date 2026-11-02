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

#[derive(Clone, Serialize)]
pub struct StorageSnapshot {
    pub version: &'static str,
    pub mode: &'static str,
    pub collected_at_ms: u128,
    pub disks: Vec<DiskDetail>,
    pub mounts: Vec<MountInfo>,
    pub warnings: Vec<String>,
    pub complete: bool,
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
    pub signature: SignatureInfo,
}

#[derive(Clone, Serialize)]
pub struct SignatureInfo {
    pub status: String,
    pub filesystem: Option<String>,
    pub uuid: Option<String>,
    pub label: Option<String>,
}
impl SignatureInfo {
    pub fn unavailable(status: &str) -> Self {
        Self {
            status: status.into(),
            filesystem: None,
            uuid: None,
            label: None,
        }
    }
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
    pub signature_status: String,
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
        // Device identity is field 3, not the textual source (which may be UUID= or /dev/root).
        let block = crate::lab::dev_number(a[2]).is_ok_and(|(major, _)| major != 0);
        if !block && b[0] != "btrfs" && !b[1].starts_with("/dev/") {
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

fn scan_partitions(root: &Path, mounts: &[MountInfo]) -> std::io::Result<Vec<Partition>> {
    let mut out = Vec::new();
    let entries = fs::read_dir(root)?;
    for entry in entries {
        let entry = entry?;
        if out.len() >= MAX_PARTITIONS {
            break;
        }
        let path = entry.path();
        if !path.join("partition").try_exists()? {
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
            signature_status: "not-probed".into(),
        });
    }
    out.sort_by_key(|a| a.start_bytes);
    Ok(out)
}

fn parse_blkid(code: Option<i32>, output: &[u8], errors: &[u8]) -> SignatureInfo {
    if !errors.is_empty() {
        return SignatureInfo::unavailable("unavailable");
    }
    match code {
        Some(2) if output.is_empty() => return SignatureInfo::unavailable("none"),
        Some(8) => return SignatureInfo::unavailable("ambiguous"),
        Some(0) if !output.is_empty() && output.len() <= 65_536 => {}
        _ => return SignatureInfo::unavailable("unavailable"),
    }
    let mut info = SignatureInfo::unavailable("detected");
    for line in String::from_utf8_lossy(output).lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "TYPE" => info.filesystem = Some(value.into()),
                "UUID" => info.uuid = Some(value.into()),
                "LABEL" => info.label = Some(value.into()),
                _ => {}
            }
        }
    }
    info
}

async fn probe_blkid(name: &str) -> SignatureInfo {
    // blkid reads superblocks for inventory; no filesystem changes are made.
    let path = format!("/dev/{name}");
    probe_blkid_path(Path::new(&path)).await
}
async fn probe_blkid_path(path: &Path) -> SignatureInfo {
    if !path.exists() {
        return SignatureInfo::unavailable("unavailable");
    }
    let mut command = Command::new("blkid");
    command
        .args(["-p", "-o", "export"])
        .arg(path)
        .kill_on_drop(true);
    match timeout(Duration::from_secs(2), command.output()).await {
        Ok(Ok(out)) => parse_blkid(out.status.code(), &out.stdout, &out.stderr),
        Err(_) => SignatureInfo::unavailable("timeout"),
        _ => SignatureInfo::unavailable("unavailable"),
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

fn inventory_files() -> StorageSnapshot {
    let mut warnings = Vec::new();
    let mount_text = match fs::read_to_string("/proc/self/mountinfo") {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("Mount table unavailable: {error}"));
            String::new()
        }
    };
    if crate::lab::mounted_devices(&mount_text).is_err() {
        warnings.push("Mount table is incomplete".into());
    }
    let mounts = parse_mountinfo(&mount_text);
    let mut disks = Vec::new();
    match fs::read_dir("/sys/block") {
        Err(_) => warnings.push("/sys/block nu este disponibil".into()),
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        warnings.push(format!("Device entry unavailable: {error}"));
                        continue;
                    }
                };
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
                let mut partitions = match scan_partitions(&path, &mounts) {
                    Ok(parts) => parts,
                    Err(error) => {
                        warnings.push(format!(
                            "Partition topology unavailable for {name}: {error}"
                        ));
                        Vec::new()
                    }
                };
                let already: usize = disks.iter().map(|d: &DiskDetail| d.partitions.len()).sum();
                if partitions.len() >= MAX_PARTITIONS || already + partitions.len() > MAX_PARTITIONS
                {
                    warnings.push("Global partition inventory limit reached (128)".into());
                }
                partitions.truncate(MAX_PARTITIONS.saturating_sub(already));
                let smart = SmartInfo::unavailable("SMART is not requested for pool topology");
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
                    signature: SignatureInfo::unavailable("not-probed"),
                });
            }
        }
    }
    disks.sort_by(|a, b| a.name.cmp(&b.name));
    let complete = warnings.is_empty();
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
        complete,
    }
}

static BASE_CACHE: crate::cache::Cache<StorageSnapshot> = crate::cache::Cache::new();
type SmartRecords = Vec<(String, u64, String, SmartInfo)>;
static SMART_CACHE: crate::cache::Cache<SmartRecords> = crate::cache::Cache::new();
static FILE_WORKER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

fn unavailable_inventory(reason: &str) -> StorageSnapshot {
    StorageSnapshot {
        version: "0.4.0",
        mode: "read-only",
        collected_at_ms: 0,
        disks: vec![],
        mounts: vec![],
        warnings: vec![reason.into()],
        complete: false,
    }
}

async fn collect_files(
    collect: impl FnOnce() -> StorageSnapshot + Send + 'static,
    budget: Duration,
) -> StorageSnapshot {
    let Ok(permit) = FILE_WORKER.try_acquire() else {
        return unavailable_inventory(
            "Previous filesystem inspection is still blocked; no additional worker was started",
        );
    };
    let job = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        collect()
    });
    match timeout(budget, job).await {
        Ok(Ok(data)) => data,
        _ => {
            unavailable_inventory("Filesystem inventory timed out or failed; result is incomplete")
        }
    }
}

async fn collect_base() -> StorageSnapshot {
    let mut data = collect_files(inventory_files, Duration::from_secs(2)).await;
    if data.disks.is_empty() {
        return data;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    let mut budget_exhausted = false;
    'disks: for disk in &mut data.disks {
        match tokio::time::timeout_at(deadline, probe_blkid(&disk.name)).await {
            Ok(sig) => disk.signature = sig,
            Err(_) => {
                data.complete = false;
                budget_exhausted = true;
                break 'disks;
            }
        }
        for part in &mut disk.partitions {
            match tokio::time::timeout_at(deadline, probe_blkid(&part.name)).await {
                Ok(sig) => {
                    part.signature_status = sig.status;
                    if part.filesystem.is_none() {
                        part.filesystem = sig.filesystem;
                    }
                    part.uuid = sig.uuid;
                    part.label = sig.label;
                }
                Err(_) => {
                    data.complete = false;
                    budget_exhausted = true;
                    break 'disks;
                }
            }
        }
    }
    if budget_exhausted {
        data.warnings
            .push("Global signature-probe budget exhausted; inventory is incomplete".into());
    }
    summarize_signatures(&mut data);
    data
}

fn summarize_signatures(data: &mut StorageSnapshot) {
    let unknown = |status: &str| !["detected", "none"].contains(&status);
    if data.disks.iter().any(|d| {
        unknown(&d.signature.status) || d.partitions.iter().any(|p| unknown(&p.signature_status))
    }) {
        data.complete = false;
        data.warnings.push("Disk or partition signatures unavailable, ambiguous or unprobed; inventory never authorizes erasure".into());
    }
}

/// Shared topology/signature cache, including disk-level signatures. Never probes SMART.
pub async fn snapshot_without_smart() -> StorageSnapshot {
    BASE_CACHE.get(Duration::from_secs(5), collect_base).await
}
pub async fn snapshot() -> StorageSnapshot {
    let mut data = snapshot_without_smart().await;
    let records = SMART_CACHE
        .get(Duration::from_secs(30), || async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
            let mut records = Vec::new();
            for disk in &data.disks {
                match tokio::time::timeout_at(deadline, probe_smart(&disk.name)).await {
                    Ok(smart) => records.push((
                        disk.name.clone(),
                        disk.size_bytes,
                        disk.model.clone(),
                        smart,
                    )),
                    Err(_) => break,
                }
            }
            records
        })
        .await;
    for disk in &mut data.disks {
        disk.smart = records
            .iter()
            .find(|(name, size, model, _)| {
                name == &disk.name && *size == disk.size_bytes && model == &disk.model
            })
            .map(|(_, _, _, smart)| smart.clone())
            .unwrap_or_else(|| {
                SmartInfo::unavailable("SMART collection budget exhausted or device changed")
            });
    }
    data.warnings.push(
        "Topology/signatures cached for 5s; SMART cached for 30s and bounded to 4s collection time"
            .into(),
    );
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn timed_out_filesystem_worker_is_not_duplicated_and_can_recover() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let count = Arc::new(AtomicUsize::new(0));
        let worker_count = count.clone();
        let (release, waiting) = std::sync::mpsc::channel::<()>();
        let (finished, done) = tokio::sync::oneshot::channel();
        let result = collect_files(
            move || {
                worker_count.fetch_add(1, Ordering::SeqCst);
                let _ = waiting.recv();
                let _ = finished.send(());
                unavailable_inventory("fixture")
            },
            Duration::from_millis(5),
        )
        .await;
        assert!(!result.complete);
        let second_count = count.clone();
        let second = collect_files(
            move || {
                second_count.fetch_add(1, Ordering::SeqCst);
                unavailable_inventory("unexpected duplicate")
            },
            Duration::from_millis(5),
        )
        .await;
        assert!(second.warnings[0].contains("Previous filesystem inspection"));
        assert!(count.load(Ordering::SeqCst) <= 1);
        release.send(()).unwrap();
        timeout(Duration::from_secs(2), done)
            .await
            .unwrap()
            .unwrap();
        let permit = timeout(Duration::from_secs(2), FILE_WORKER.acquire())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        let third = collect_files(
            || unavailable_inventory("recovered collector"),
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(third.warnings, ["recovered collector"]);
    }
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
    fn mount_aliases_are_inventoried_by_device_number() {
        let found = parse_mountinfo("35 24 8:1 / /mnt/alias rw - ext4 UUID=example rw\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].major_minor, "8:1");
    }
    #[test]
    fn signature_output_requires_successful_status() {
        assert_eq!(parse_blkid(Some(8), b"TYPE=ext4", b"").status, "ambiguous");
        assert!(parse_blkid(Some(1), b"TYPE=ext4", b"").filesystem.is_none());
        assert_eq!(parse_blkid(Some(2), b"", b"").status, "none");
        assert_eq!(
            parse_blkid(Some(2), b"", b"Permission denied").status,
            "unavailable"
        );
        assert_eq!(
            parse_blkid(Some(0), b"TYPE=ext4\nLABEL=x\n", b"")
                .filesystem
                .as_deref(),
            Some("ext4")
        );
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn real_regular_fixture_read_error_is_not_an_empty_signature() {
        use std::os::unix::fs::PermissionsExt;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "zion-blkid-fixture-{}-{unique}",
            std::process::id()
        ));
        fs::write(&path, [0u8; 4096]).unwrap();
        assert_eq!(probe_blkid_path(&path).await.status, "none");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(probe_blkid_path(&path).await.status, "unavailable");
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(probe_blkid_path(&path).await.status, "unavailable");
    }
    #[test]
    fn failed_partition_probe_marks_otherwise_successful_inventory_incomplete() {
        let part = Partition {
            name: "vda1".into(),
            size_bytes: 1024,
            start_bytes: 512,
            major_minor: "254:1".into(),
            filesystem: None,
            uuid: None,
            label: None,
            mounts: vec![],
            signature_status: "none".into(),
        };
        let mut original = unavailable_inventory("fixture");
        original.warnings.clear();
        original.complete = true;
        original.disks.push(DiskDetail {
            name: "vda".into(),
            model: "fixture".into(),
            size_bytes: 4096,
            rotational: false,
            removable: false,
            read_only: false,
            transport: "VirtIO".into(),
            logical_sector_bytes: Some(512),
            physical_sector_bytes: Some(512),
            partitions: vec![part],
            smart: SmartInfo::unavailable("fixture"),
            signature: parse_blkid(Some(0), b"PTTYPE=gpt", b""),
        });
        let mut good = original.clone();
        summarize_signatures(&mut good);
        assert!(good.complete);
        for status in ["unavailable", "ambiguous", "timeout", "not-probed"] {
            let mut data = original.clone();
            data.disks[0].partitions[0].signature_status = status.into();
            summarize_signatures(&mut data);
            assert!(!data.complete);
            assert!(data
                .warnings
                .iter()
                .any(|s| s.contains("partition signatures")));
        }
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
