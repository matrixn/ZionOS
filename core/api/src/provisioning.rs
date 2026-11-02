//! Shared read-only laboratory readiness; execution is deliberately disabled in 0.6.1.
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct LabSnapshot {
    pub version: &'static str,
    pub mode: &'static str,
    pub backend: &'static str,
    pub target: &'static str,
    pub qemu_detected: bool,
    pub target_exists: bool,
    pub target_capacity_bytes: Option<u64>,
    pub eligible_for_lab: bool,
    pub inspection_ready: bool,
    pub execution_enabled: bool,
    pub reasons: Vec<String>,
    pub procedure: Vec<&'static str>,
    pub recovery: RecoveryPlan,
}
#[derive(Clone, Serialize)]
pub struct RecoveryPlan {
    pub mode: &'static str,
    pub automatic_repairs: bool,
    pub recovery_point_available: bool,
    pub steps: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
}
pub fn snapshot() -> LabSnapshot {
    let inspection = crate::lab::inspect();
    from_inspection(
        inspection,
        std::fs::read_to_string("/sys/class/dmi/id/sys_vendor").is_ok_and(|v| v.trim() == "QEMU"),
        std::path::Path::new("/sys/block/vdb").exists(),
    )
}
fn from_inspection(
    inspection: crate::lab::Inspection,
    qemu_detected: bool,
    target_exists: bool,
) -> LabSnapshot {
    LabSnapshot {
        version: crate::VERSION, mode: "read-only", backend: "QEMU virtual-disk lab", target: "/dev/vdb",
        qemu_detected,target_exists,
        target_capacity_bytes: inspection.capacity_bytes,
        eligible_for_lab: false, inspection_ready: inspection.inspection_ready, execution_enabled: false,
        reasons: vec![inspection.reason, "Genesis 0.6.1 blochează --apply: nu există încă un executor de formatare verificat și exclusiv.".into()],
        procedure: vec!["Pornește scripts/run-qemu-lab.sh: creează un QCOW2 nou cu identitate de sesiune.",
            "Consola VM: zion-lab-provision --plan sau --validate, fără scrieri.",
            "Pentru un ext4 deja disponibil în lab: zion-lab-recover --check, numai diagnostic."],
        recovery: RecoveryPlan {
            mode:"diagnostic-only", automatic_repairs:false, recovery_point_available:false,
            steps:vec!["Verifică identitatea lab, mount-uri, swap și holders înainte de diagnostic.",
                "Rulează zion-lab-recover --check; e2fsck -fn citește un descriptor fixat și exclusiv.",
                "Salvează rezultatul în afara VM; nu există reparare sau restaurare automată."],
            limitations:vec!["Datele și logurile live în RAM nu supraviețuiesc reboot-ului.",
                "Fără autentificare/TLS: acces numai local sau prin contractul QEMU izolat.",
                "Preflight-ul API nu autorizează scrieri și nu înlocuiește proba privilegiată de semnături."],
        },
    }
}

static CACHE: crate::cache::Cache<LabSnapshot> = crate::cache::Cache::new();
static WORKER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
pub async fn snapshot_cached() -> LabSnapshot {
    CACHE.get(std::time::Duration::from_secs(3),||async {
        let fallback=||from_inspection(crate::lab::Inspection {
            target:"/dev/vdb",execution_enabled:false,inspection_ready:false,serial:None,capacity_bytes:None,
            reason:"Lab inspection unavailable or timed out; no additional blocking worker was started".into()
        },false,false);
        let Ok(permit)=WORKER.try_acquire() else {return fallback();};
        let job=tokio::task::spawn_blocking(move|| {let _permit=permit;snapshot()});
        match tokio::time::timeout(std::time::Duration::from_secs(2),job).await {
            Ok(Ok(snapshot))=>snapshot,_=>fallback(),
        }
    }).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contract_is_diagnostic_only() {
        let s = snapshot();
        assert_eq!(s.mode, "read-only");
        assert!(!s.execution_enabled && !s.eligible_for_lab);
        assert!(!s.recovery.automatic_repairs && !s.recovery.recovery_point_available);
    }
}
