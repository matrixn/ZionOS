//! ZionOS Genesis 0.5: observational pool/volume topology. No write operations.
use crate::storage::{DiskDetail, StorageSnapshot};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_POOLS: usize = 64;

#[derive(Serialize)]
pub struct PoolSnapshot {
    pub version: &'static str,
    pub mode: &'static str,
    pub collected_at_ms: u128,
    pub pools: Vec<Pool>,
    pub volumes: Vec<Volume>,
    pub candidates: Vec<DiskCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct Pool {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub layout: String,
    pub state: String,
    pub members: Vec<String>,
    pub capacity_bytes: Option<u64>,
    pub degraded_devices: Option<u64>,
    pub sync_action: Option<String>,
    pub evidence: &'static str,
}

#[derive(Serialize)]
pub struct Volume {
    pub id: String,
    pub device: String,
    pub filesystem: Option<String>,
    pub label: Option<String>,
    pub size_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub capacity_basis: &'static str,
    pub mountpoints: Vec<String>,
    pub read_only: Option<bool>,
    pub pool_id: Option<String>,
}

#[derive(Serialize)]
pub struct DiskCandidate {
    pub name: String,
    pub size_bytes: u64,
    pub model: String,
    pub preview_only: bool,
    pub warning: Option<&'static str>,
}

fn text(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn uuid_name(name: &str) -> bool {
    name.len() == 36
        && name.chars().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn members(path: impl AsRef<Path>) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|s| safe_name(s))
        .take(64)
        .collect();
    names.sort();
    names
}

fn pool_for_device(source: &str, filesystem: &str, pools: &[Pool]) -> Option<String> {
    for pool in pools {
        if pool.kind == "mdraid" {
            let root = format!("/dev/{}", pool.name);
            if source == root
                || source.strip_prefix(&root).is_some_and(|s| {
                    s.strip_prefix('p')
                        .is_some_and(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
                })
            {
                return Some(pool.id.clone());
            }
        }
        if pool.kind == "btrfs"
            && filesystem == "btrfs"
            && pool
                .members
                .iter()
                .any(|name| source == format!("/dev/{name}"))
        {
            return Some(pool.id.clone());
        }
    }
    None
}

fn snapshot_from_parts(data: StorageSnapshot, mut pools: Vec<Pool>) -> PoolSnapshot {
    let mut volumes = Vec::new();
    let mut mounted_devs = HashSet::new();
    for m in &data.mounts {
        mounted_devs.insert(m.major_minor.clone());
        let label = data
            .disks
            .iter()
            .flat_map(|d| &d.partitions)
            .find(|part| part.major_minor == m.major_minor)
            .and_then(|part| part.label.clone());
        volumes.push(Volume {
            id: format!("mount:{}", m.mountpoint),
            device: m.source.clone(),
            filesystem: Some(m.filesystem.clone()),
            label,
            size_bytes: m.total_bytes,
            available_bytes: m.available_bytes,
            capacity_basis: "filesystem",
            mountpoints: vec![m.mountpoint.clone()],
            read_only: Some(m.read_only),
            pool_id: pool_for_device(&m.source, &m.filesystem, &pools),
        });
    }
    // Unmounted partitions are inventory entries; capacity is PARTITION size, not filesystem free space.
    for part in data.disks.iter().flat_map(|d| &d.partitions) {
        if mounted_devs.contains(&part.major_minor) {
            continue;
        }
        let source = format!("/dev/{}", part.name);
        volumes.push(Volume {
            id: format!("partition:{}", part.name),
            device: source.clone(),
            filesystem: part.filesystem.clone(),
            label: part.label.clone(),
            size_bytes: Some(part.size_bytes),
            available_bytes: None,
            capacity_basis: "partition",
            mountpoints: Vec::new(),
            read_only: None,
            pool_id: pool_for_device(&source, part.filesystem.as_deref().unwrap_or(""), &pools),
        });
    }
    volumes.sort_by(|a, b| a.device.cmp(&b.device).then(a.id.cmp(&b.id)));
    pools.sort_by_key(|pool| pool.id.clone());
    let candidates = data
        .disks
        .iter()
        .filter(|d| !["Software RAID", "Device mapper", "Loop"].contains(&d.transport.as_str()))
        .filter(|d| d.size_bytes > 0 && safe_name(&d.name))
        .map(|d| {
            let busy = !d.partitions.is_empty()
                || data
                    .mounts
                    .iter()
                    .any(|m| m.source == format!("/dev/{}", d.name));
            DiskCandidate {
                name: d.name.clone(),
                size_bytes: d.size_bytes,
                model: d.model.clone(),
                preview_only: true,
                warning: if busy {
                    Some("Există partiții sau montări; discul poate conține date")
                } else {
                    Some("Fără partiții detectate; nu înseamnă că discul este liber")
                },
            }
        })
        .collect();
    let mut warnings = data.warnings;
    warnings.push("Topologia este doar observațională. Volumele pot fi afișate de mai multe ori la bind mounts sau subvolume.".into());
    PoolSnapshot {
        version: "0.5.0",
        mode: "read-only",
        collected_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        pools,
        volumes,
        candidates,
        warnings,
    }
}

fn md_pools(disks: &[DiskDetail]) -> Vec<Pool> {
    let mut found = Vec::new();
    for d in disks
        .iter()
        .filter(|d| d.name.starts_with("md") && safe_name(&d.name))
    {
        if found.len() >= MAX_POOLS {
            break;
        }
        let root = Path::new("/sys/block").join(&d.name);
        if !root.join("md").exists() {
            continue;
        }
        let level = text(root.join("md/level"));
        found.push(Pool {
            id: format!("md:{}", d.name),
            name: d.name.clone(),
            kind: "mdraid",
            layout: if level.is_empty() {
                "unknown".into()
            } else {
                level
            },
            state: text(root.join("md/array_state")),
            members: members(root.join("slaves")),
            capacity_bytes: Some(d.size_bytes),
            degraded_devices: text(root.join("md/degraded")).parse().ok(),
            sync_action: {
                let v = text(root.join("md/sync_action"));
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            },
            evidence: "linux-md-sysfs",
        });
    }
    found
}

fn btrfs_pools() -> Vec<Pool> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/fs/btrfs") else {
        return found;
    };
    for entry in entries.flatten() {
        if found.len() >= MAX_POOLS {
            break;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !uuid_name(&id) {
            continue;
        }
        let root = entry.path();
        if !root.is_dir() {
            continue;
        }
        let label = text(root.join("label"));
        found.push(Pool {
            id: format!("btrfs:{id}"),
            name: if label.is_empty() { id } else { label },
            kind: "btrfs",
            layout: "unknown (inspect btrfs profiles separately)".into(),
            state: "detected".into(),
            members: members(root.join("devices")),
            capacity_bytes: None,
            degraded_devices: None,
            sync_action: None,
            evidence: "btrfs-sysfs",
        });
    }
    found
}

pub async fn snapshot() -> PoolSnapshot {
    // Uses fast inventory: skips SMART probing, which can be slow or wake drives.
    let data = crate::storage::snapshot_without_smart().await;
    let mut pools = md_pools(&data.disks);
    pools.extend(btrfs_pools());
    snapshot_from_parts(data, pools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MountInfo, Partition, SmartInfo};

    fn disk(partitions: Vec<Partition>) -> DiskDetail {
        DiskDetail {
            name: "vda".into(),
            model: "virtual".into(),
            size_bytes: 10_000,
            rotational: false,
            removable: false,
            read_only: false,
            transport: "VirtIO".into(),
            logical_sector_bytes: Some(512),
            physical_sector_bytes: Some(512),
            partitions,
            smart: SmartInfo {
                status: "unavailable".into(),
                temperature_c: None,
                power_on_hours: None,
                percentage_used: None,
                reason: None,
            },
        }
    }
    fn sample(mount: bool) -> StorageSnapshot {
        let p = Partition {
            name: "vda1".into(),
            size_bytes: 9_000,
            start_bytes: 512,
            major_minor: "254:1".into(),
            filesystem: Some("ext4".into()),
            uuid: None,
            label: None,
            mounts: if mount { vec!["/data".into()] } else { vec![] },
        };
        StorageSnapshot {
            version: "0.4.0",
            mode: "read-only",
            collected_at_ms: 0,
            disks: vec![disk(vec![p])],
            mounts: if mount {
                vec![MountInfo {
                    major_minor: "254:1".into(),
                    mountpoint: "/data".into(),
                    source: "/dev/vda1".into(),
                    filesystem: "ext4".into(),
                    read_only: false,
                    total_bytes: Some(8_000),
                    available_bytes: Some(2_000),
                }]
            } else {
                vec![]
            },
            warnings: vec![],
        }
    }
    #[test]
    fn mounted_volume_has_filesystem_capacity() {
        let result = snapshot_from_parts(sample(true), vec![]);
        assert_eq!(result.volumes.len(), 1);
        assert_eq!(result.volumes[0].size_bytes, Some(8_000));
        assert_eq!(result.volumes[0].capacity_basis, "filesystem");
    }
    #[test]
    fn unmounted_partition_is_not_free_space() {
        let result = snapshot_from_parts(sample(false), vec![]);
        assert_eq!(result.volumes[0].available_bytes, None);
        assert_eq!(result.volumes[0].capacity_basis, "partition");
    }
    #[test]
    fn device_name_validation() {
        assert!(safe_name("nvme0n1"));
        assert!(!safe_name("../../etc"));
        assert!(uuid_name("01b7aee4-a11c-432a-b807-cba64ab01111"));
        assert!(!uuid_name("broken-id"));
    }
    #[test]
    fn md_device_mapping() {
        let pools = vec![Pool {
            id: "md:md0".into(),
            name: "md0".into(),
            kind: "mdraid",
            layout: "raid1".into(),
            state: "clean".into(),
            members: vec!["sda1".into(), "sdb1".into()],
            capacity_bytes: Some(123),
            degraded_devices: Some(0),
            sync_action: None,
            evidence: "linux-md-sysfs",
        }];
        assert_eq!(
            pool_for_device("/dev/md0", "ext4", &pools),
            Some("md:md0".into())
        );
        assert_eq!(
            pool_for_device("/dev/md0p1", "ext4", &pools),
            Some("md:md0".into())
        );
        assert_eq!(pool_for_device("/dev/md01", "ext4", &pools), None);
    }
}
