//! Genesis 0.6: read-only provisioning readiness and recovery diagnostics.
//! This HTTP API deliberately NEVER invokes mkfs/mount/partition/repair operations.
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Serialize)]
pub struct LabSnapshot {
    pub version: &'static str,
    pub mode: &'static str,
    pub backend: &'static str,
    pub target: &'static str,
    pub qemu_detected: bool,
    pub target_exists: bool,
    pub target_capacity_bytes: Option<u64>,
    pub eligible_for_lab: bool,
    pub reasons: Vec<String>,
    pub procedure: Vec<&'static str>,
    pub recovery: RecoveryPlan,
}

#[derive(Serialize)]
pub struct RecoveryPlan {
    pub mode: &'static str,
    pub automatic_repairs: bool,
    pub recovery_point_available: bool,
    pub steps: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
}

fn trimmed(path: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn qemu_vendor(vendor: &str) -> bool {
    vendor.eq_ignore_ascii_case("qemu")
}

fn valid_size(sectors: u64) -> bool {
    let bytes = sectors.saturating_mul(512);
    (128 * 1024 * 1024..=8 * 1024 * 1024 * 1024).contains(&bytes)
}

pub fn snapshot() -> LabSnapshot {
    let base = Path::new("/sys/block/vdb");
    let target_exists = base.exists() && Path::new("/dev/vdb").exists();
    let qemu_detected = qemu_vendor(&trimmed("/sys/class/dmi/id/sys_vendor"));
    let size: Option<u64> = trimmed("/sys/block/vdb/size").parse().ok();
    let target_capacity_bytes = size.map(|n| n.saturating_mul(512));
    let mut reasons = Vec::new();
    if !qemu_detected {
        reasons
            .push("Nu este detectat un sistem QEMU; operația de laborator este interzisă.".into());
    }
    if !target_exists {
        reasons.push(
            "Nu există discul separat /dev/vdb. Pornește VM cu scripts/run-qemu-lab.sh.".into(),
        );
    }
    if !size.is_some_and(valid_size) {
        reasons.push("Discul vdb trebuie să aibă între 128 MiB și 8 GiB.".into());
    }
    if trimmed("/sys/block/vdb/ro") != "0" {
        reasons.push("Discul vdb nu este confirmat ca writable.".into());
    }
    let children = fs::read_dir(base)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .any(|n| n.starts_with("vdb") && n.len() > 3 && n[3..].bytes().all(|b| b.is_ascii_digit()));
    if children {
        reasons.push("vdb are partiții; nu este un disc gol pentru laborator.".into());
    }
    // /sys and /proc cannot establish device emptiness; the CLI performs further checks.
    reasons.push("Eligibilitatea finală necesită verificarea semnăturilor în consola VM; API-ul nu execută write.".into());
    let eligible_for_lab = qemu_detected
        && target_exists
        && size.is_some_and(valid_size)
        && trimmed("/sys/block/vdb/ro") == "0"
        && !children;
    LabSnapshot {
        version: "0.6.0", mode: "read-only", backend: "QEMU virtual-disk lab", target: "/dev/vdb",
        qemu_detected, target_exists, target_capacity_bytes, eligible_for_lab,
        reasons,
        procedure: vec![
            "Pornește VM numai cu scripts/run-qemu-lab.sh și un fișier QCOW2 dedicat.",
            "În consola QEMU: zion-lab-provision --plan (doar inspecție).",
            "Dacă accepți distrugerea discului virtual separat: zion-lab-provision --apply; confirmare interactivă.",
            "Verifică filesystemul nemontat: zion-lab-recover --check (read-only).",
        ],
        recovery: RecoveryPlan {
            mode: "diagnostic-only", automatic_repairs: false, recovery_point_available: false,
            steps: vec![
                "Oprește aplicațiile care folosesc discul și verifică că /dev/vdb nu este montat.",
                "Rulează zion-lab-recover --check în consola VM pentru un e2fsck fără modificări.",
                "Salvează diagnosticul în afara VM înainte de orice intervenție.",
                "Pentru recuperarea datelor reale: restaurare din backup independent; nu există rollback automat în Genesis 0.6.",
            ],
            limitations: vec![
                "Nicio reparare automată, snapshot sau restaurare implementată.",
                "Fișierele din RAM ale sistemului live se pierd la reboot.",
                "API-ul nu autentifică utilizatorii; operațiile privilegiate nu sunt expuse prin HTTP.",
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn size_limits() {
        assert!(!valid_size(100));
        assert!(valid_size(1_048_576)); // 512 MiB
        assert!(!valid_size(20_000_000));
    }
    #[test]
    fn vendor_validation() {
        assert!(qemu_vendor("QEMU"));
        assert!(!qemu_vendor("Apple Inc."));
        assert!(!qemu_vendor("Microsoft Corporation"));
    }
    #[test]
    fn contract_is_diagnostic_only() {
        let s = snapshot();
        assert_eq!(s.mode, "read-only");
        assert!(!s.recovery.automatic_repairs);
        assert!(!s.recovery.recovery_point_available);
    }
}
