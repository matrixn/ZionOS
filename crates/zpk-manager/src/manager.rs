use crate::{
    database::Database,
    digest, fail,
    filesystem::{self, Environment, Lock, Root},
    host::Host,
    model::{Kind, Operation, Record, Review, Version},
    resolver, Result,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zpk_runtime::{
    backend::{State, Status},
    config::{Activation, Grants, HealthMode, Plan, RuntimeConfig, StorageMode},
    installed,
};
use zpk_schema::{
    manifest::{FileKind, StorageKind},
    Manifest,
};
/// Faults are invoked only by tests/library callers; the CLI exposes no fault injection.
pub trait Faults {
    fn checkpoint(&self, point: &str) -> Result<()>;
}
pub struct NoFaults;
impl Faults for NoFaults {
    fn checkpoint(&self, _: &str) -> Result<()> {
        Ok(())
    }
}
pub struct Manager<H: Host> {
    pub root: Root,
    pub database: Database,
    pub host: H,
    _lock: Lock,
}
#[derive(Debug, serde::Serialize)]
pub struct Report {
    pub operation: i64,
    pub state: String,
    pub package_id: String,
    pub active: Option<String>,
    pub data: String,
    pub health: String,
}
impl<H: Host> Manager<H> {
    pub fn open(path: &Path, environment: Environment, host: H) -> Result<Self> {
        let root = Root::new(path, environment)?;
        for path in [
            "opt/zionos/apps",
            "etc/zionos/apps",
            "var/lib/zionos/apps",
            "var/log/zionos/apps",
            "var/lib/zionos/packages",
        ] {
            let p = root.join(path);
            root.mkdir(
                &p,
                if path.ends_with("packages") {
                    0o700
                } else {
                    0o755
                },
            )?;
            root.persistent(&p)?;
        }
        let state = root.join("var/lib/zionos/packages");
        // State is never traversable by application users.
        if fs::metadata(&state)?.permissions().mode() & 0o777 != 0o700 {
            return fail("package registry directory must have mode 0700");
        }
        let lock = Lock::acquire(&root, &state.join("manager.lock"))?;
        let database = Database::open(&root, &state.join("registry.sqlite3"))?;
        database.packages(&root.path)?;
        database.retained(&root.path)?;
        database.pending(&root.path)?;
        Ok(Self {
            root,
            database,
            host,
            _lock: lock,
        })
    }
    fn ready(&self) -> Result<()> {
        if !self.database.pending(&self.root.path)?.is_empty() {
            return fail("incomplete operation: run startup reconciliation; unresolved recovery blocks all package operations");
        }
        Ok(())
    }
    pub fn list(&self) -> Result<BTreeMap<String, Record>> {
        self.database.packages(&self.root.path)
    }
    pub fn info(&self, id: &str) -> Result<Record> {
        if !zpk_schema::manifest::package_id(id) {
            return fail("invalid package ID");
        }
        self.list()?
            .remove(id)
            .ok_or_else(|| format!("package is not installed: {id}").into())
    }
    fn plan(&self, r: &Record) -> Result<Plan> {
        let v = r.current();
        v.configuration.clone().plan(v.parsed()?, &self.root.path)
    }
    fn verify_snapshot(&self, p: &Plan, r: &Record) -> Result<()> {
        let v = r.current();
        if digest(&installed::read_file(
            &p.layout.version.join("manifest.toml"),
            65536,
        )?) != v.manifest_sha256
            || digest(&installed::read_file(
                &p.layout.version.join("checksums.json"),
                1024 * 1024,
            )?) != v.inventory_sha256
        {
            return fail("version snapshot differs from committed manifest/inventory digests");
        }
        Ok(())
    }
    pub fn verify(&self, id: &str) -> Result<Plan> {
        let r = self.info(id)?;
        let v = r.current();
        let p = installed::load(&self.root.path, &v.configuration)?;
        if digest(&installed::read_file(
            &p.layout.version.join("manifest.toml"),
            65536,
        )?) != v.manifest_sha256
            || digest(&installed::read_file(
                &p.layout.version.join("checksums.json"),
                1024 * 1024,
            )?) != v.inventory_sha256
            || installed::read_file(&p.layout.config, 65536)?
                != toml::to_string_pretty(&v.configuration)?.as_bytes()
        {
            return fail("installed files/configuration disagree with committed registry");
        }
        self.root.persistent(&p.layout.version)?;
        Ok(p)
    }
    fn os(&self) -> Result<semver::Version> {
        let b = installed::read_file(&self.root.join("etc/zionos-release"), 4096)?;
        std::str::from_utf8(&b)?
            .split_whitespace()
            .find_map(|s| s.parse().ok())
            .ok_or_else(|| "missing ZionOS version".into())
    }
    fn configuration(
        &self,
        m: &Manifest,
        old: Option<&Record>,
        retained: Option<&Record>,
        review: &Review,
    ) -> Result<RuntimeConfig> {
        let identity = if let Some(old) = old.or(retained) {
            old.current().configuration.identity.clone()
        } else {
            let installed = self.list()?;
            let retained = self.database.retained(&self.root.path)?;
            let reserved = installed
                .values()
                .chain(retained.values())
                .flat_map(|r| {
                    let i = &r.current().configuration.identity;
                    [i.uid, i.gid]
                })
                .collect();
            self.host.identity(&m.package.id, &reserved)?
        };
        let entries: Vec<_> = m
            .desktop
            .iter()
            .filter(|d| d.executable.is_some())
            .collect();
        let desktop_entry = if review.desktop_entry.is_some() {
            review.desktop_entry.clone()
        } else if m.service.is_none() && entries.len() == 1 {
            Some(entries[0].id.clone())
        } else {
            None
        };
        if m.service.is_none() && entries.len() > 1 && desktop_entry.is_none() {
            return fail(
                "multiple desktop executables require explicit --desktop-entry ID selection",
            );
        }
        let c = RuntimeConfig {
            runtime_version: 1,
            package_id: m.package.id.clone(),
            version: m.package.version.clone(),
            identity,
            activation: if m.service.is_some() {
                Activation::Background
            } else {
                Activation::OnDemand
            },
            desktop_entry,
            autostart: review.autostart || old.is_some_and(|r| r.current().configuration.autostart),
            health: if review.unix_health {
                HealthMode::UnixSocket
            } else {
                old.map(|r| r.current().configuration.health.clone())
                    .unwrap_or(HealthMode::Process)
            },
            storage_mode: if self.root.environment == Environment::Persistent {
                StorageMode::PersistentRequired
            } else {
                StorageMode::EphemeralDevelopment
            },
            limits: old
                .map(|r| r.current().configuration.limits.clone())
                .unwrap_or_default(),
            restart: old
                .map(|r| r.current().configuration.restart.clone())
                .unwrap_or_default(),
            grants: Grants {
                data: m
                    .storage
                    .iter()
                    .filter(|s| matches!(s.kind, StorageKind::Data))
                    .map(|s| s.name.clone())
                    .collect(),
                configuration_write: m
                    .storage
                    .iter()
                    .filter(|s| matches!(s.kind, StorageKind::Config))
                    .map(|s| s.name.clone())
                    .collect(),
            },
        };
        c.clone().plan(m.clone(), &self.root.path)?;
        Ok(c)
    }
    fn app(&self, id: &str) -> PathBuf {
        self.root.join("opt/zionos/apps").join(id)
    }
    fn stage(&self, op: &Operation, n: i64) -> PathBuf {
        self.app(&op.package_id)
            .join("versions")
            .join(format!(".stage-{n}"))
    }
    fn trash(&self, op: &Operation, n: i64) -> PathBuf {
        self.app(&op.package_id).join(format!(".removed-{n}"))
    }
    fn live_reference(&self, op: &Operation) -> Result<()> {
        let current = self.app(&op.package_id).join("current");
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                let target = fs::read_link(&current)?;
                if ![&op.old, &op.new]
                    .into_iter()
                    .flatten()
                    .any(|r| target == Path::new("versions").join(&r.active))
                {
                    return fail("current reference is foreign to operation; refusing replacement");
                }
            }
            Ok(_) => return fail("current reference is not a symlink"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.root.check(current.parent().ok_or("missing parent")?)?;
        Ok(())
    }
    fn activate_files(&self, op: &Operation, r: Option<&Record>, n: i64) -> Result<()> {
        self.live_reference(op)?;
        let current = self.app(&op.package_id).join("current");
        if let Some(r) = r {
            let p = self.plan(r)?;
            self.root.check(&p.layout.version)?;
            self.root.mkdir(&p.layout.configuration, 0o755)?;
            filesystem::atomic(
                &self.root,
                &p.layout.config,
                toml::to_string_pretty(&p.config)?.as_bytes(),
                &format!("config-{n}"),
            )?;
            let next = self.app(&r.id).join(format!(".current-{n}"));
            if fs::symlink_metadata(&next).is_ok() {
                if fs::read_link(&next)? != Path::new("versions").join(&r.active) {
                    return fail("foreign activation temporary reference");
                }
                fs::remove_file(&next)?;
            }
            symlink(Path::new("versions").join(&r.active), &next)?;
            fs::rename(next, &current)?;
            filesystem::sync(&self.app(&r.id))?;
            installed::load(&self.root.path, &p.config)?;
            self.verify_snapshot(&p, r)?;
        } else {
            if fs::symlink_metadata(&current).is_ok() {
                fs::remove_file(&current)?;
                filesystem::sync(&self.app(&op.package_id))?;
            }
            let cfg = self
                .root
                .join("etc/zionos/apps")
                .join(&op.package_id)
                .join("runtime.toml");
            if cfg.try_exists()? {
                self.root.check(&cfg)?;
                fs::remove_file(&cfg)?;
                filesystem::sync(cfg.parent().ok_or("missing parent")?)?;
            }
        }
        Ok(())
    }
    fn stage_archive(
        &self,
        op: &Operation,
        n: i64,
        bytes: &[u8],
        fault: &dyn Faults,
    ) -> Result<()> {
        let record = op.new.as_ref().ok_or("missing candidate")?;
        let p = self.plan(record)?;
        let marker = p.layout.app.join(".manager-owner");
        if p.layout.app.try_exists()? {
            self.root.check(&p.layout.app)?;
            if installed::read_file(&marker, 512)?
                != format!("ZPK-MANAGER-1\n{}\n", record.id).as_bytes()
            {
                return fail("foreign application directory collision");
            }
        } else {
            self.root.mkdir(&p.layout.app, 0o755)?;
            filesystem::atomic(
                &self.root,
                &marker,
                format!("ZPK-MANAGER-1\n{}\n", record.id).as_bytes(),
                &format!("owner-{n}"),
            )?;
        }
        self.root
            .mkdir(p.layout.version.parent().ok_or("missing versions")?, 0o755)?;
        self.root
            .persistent(p.layout.version.parent().ok_or("missing versions")?)?;
        if p.layout.version.try_exists()? {
            return fail("immutable version path already exists; refusing collision/overwrite");
        }
        let stage = self.stage(op, n);
        self.root.mkdir(&stage, 0o700)?;
        let mut output: Option<fs::File> = None;
        zpk_schema::archive::inspect_stream(
            Cursor::new(bytes),
            zpk_schema::Limits::default(),
            &mut |name, chunk| {
                if chunk.is_empty() {
                    if let Some(f) = output.take() {
                        f.sync_all()?;
                    }
                    let destination = stage.join(name);
                    self.root
                        .mkdir(
                            destination
                                .parent()
                                .ok_or_else(|| std::io::Error::other("missing parent"))?,
                            0o755,
                        )
                        .map_err(|e| std::io::Error::other(e.to_string()))?;
                    let executable = p
                        .manifest
                        .files
                        .iter()
                        .any(|f| f.path == name && f.kind == FileKind::Executable);
                    output = Some(
                        fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(if executable { 0o755 } else { 0o644 })
                            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                            .open(destination)?,
                    );
                    output
                        .as_ref()
                        .ok_or_else(|| std::io::Error::other("missing output"))?
                        .set_permissions(fs::Permissions::from_mode(if executable {
                            0o755
                        } else {
                            0o644
                        }))?;
                } else {
                    output
                        .as_mut()
                        .ok_or_else(|| std::io::Error::other("missing stage output"))?
                        .write_all(chunk)?;
                }
                Ok(())
            },
        )?;
        if let Some(f) = output {
            f.sync_all()?;
        }
        sync_tree(&stage)?;
        fault.checkpoint("stage-written")?;
        self.database.phase(n, "STAGED")?;
        fault.checkpoint("staged")?;
        Ok(())
    }
    fn wait_health(&self, p: &Plan) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.host.healthy(p)? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return fail("candidate service failed health verification");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    /// Fault errors simulate abrupt interruption: journal is deliberately left pending.
    pub fn install(
        &mut self,
        package: &Path,
        upgrade: bool,
        review: &Review,
        fault: &dyn Faults,
    ) -> Result<Report> {
        self.ready()?;
        let bytes = installed::read_file(package, 64 * 1024 * 1024)?;
        let inspection = zpk_schema::inspect(Cursor::new(&bytes), zpk_schema::Limits::default())?;
        let m = inspection.manifest;
        let id = m.package.id.clone();
        let mut all = self.list()?;
        let old = all.remove(&id);
        if old.is_none() && all.len() >= 1024 {
            return fail("installed package count limit reached");
        }
        if upgrade != old.is_some() {
            return fail(if upgrade {
                "upgrade requires an installed package"
            } else {
                "package already installed; use upgrade"
            });
        }
        m.check_compatibility(
            &self.os()?,
            old.as_ref().map(|r| &r.current().configuration.version),
        )?;
        review.check(&m)?;
        let mut retained = self.database.retained(&self.root.path)?;
        let previous_data = retained.remove(&id);
        if old.is_some() {
            self.verify(&id)?;
        }
        if let Some(old) = old.as_ref().or(previous_data.as_ref()) {
            let previous = old.current().parsed()?;
            if m.compatibility.data_schema != previous.compatibility.data_schema
                || m.storage.iter().any(|s| {
                    previous
                        .storage
                        .iter()
                        .find(|p| p.name == s.name)
                        .is_some_and(|p| {
                            p.schema != s.schema
                                || std::mem::discriminant(&p.kind)
                                    != std::mem::discriminant(&s.kind)
                        })
                })
                || previous
                    .storage
                    .iter()
                    .any(|p| !m.storage.iter().any(|s| s.name == p.name))
            {
                return fail("data/configuration schema migration or storage removal is unsupported; binaries cannot roll back application data");
            }
            if upgrade && old.versions.len() >= 64 {
                return fail("retained version limit reached; explicit maintenance required");
            }
        }
        let c = self.configuration(&m, old.as_ref(), previous_data.as_ref(), review)?;
        for r in all.values().chain(retained.values()) {
            let i = &r.current().configuration.identity;
            if i.uid == c.identity.uid || i.gid == c.identity.gid || i.name == c.identity.name {
                return fail("package identity collision");
            }
        }
        if let Some(old) = &old {
            all.insert(id.clone(), old.clone());
        }
        resolver::resolve(&all, Some(&m), None)?;
        if upgrade {
            self.dependents_stopped(&id)?;
        }
        // Snapshot exact archive manifest/checksum bytes through the shared parser.
        let mut meta = BTreeMap::<String, Vec<u8>>::new();
        zpk_schema::archive::inspect_stream(
            Cursor::new(&bytes),
            zpk_schema::Limits::default(),
            &mut |name, chunk| {
                if name == "manifest.toml" || name == "checksums.json" {
                    meta.entry(name.into())
                        .or_default()
                        .extend_from_slice(chunk);
                }
                Ok(())
            },
        )?;
        let manifest = String::from_utf8(meta.remove("manifest.toml").ok_or("missing manifest")?)?;
        let version = m.package.version.to_string();
        let v = Version {
            approved_declarations: m.permissions.clone(),
            runtime_policy: zpk_runtime::policy::ENFORCED.into(),
            manifest_sha256: digest(manifest.as_bytes()),
            manifest,
            configuration: c,
            source: package.to_string_lossy().into_owned(),
            archive_sha256: digest(&bytes),
            inventory_sha256: digest(&meta["checksums.json"]),
        };
        let mut new = old.clone().unwrap_or_else(|| Record {
            id: id.clone(),
            active: version.clone(),
            previous: None,
            versions: BTreeMap::new(),
            service_state: "stopped".into(),
        });
        if new.versions.contains_key(&version) {
            return fail(
                "version already retained; use rollback rather than replacing immutable content",
            );
        }
        new.previous = old.as_ref().map(|r| r.active.clone());
        new.active = version.clone();
        new.versions.insert(version, v);
        let was_running = old
            .as_ref()
            .map(|r| {
                self.host
                    .status(&self.plan(r)?)
                    .map(|s| s.state == State::Running)
            })
            .transpose()?
            .unwrap_or(false);
        let op = Operation {
            kind: if upgrade {
                Kind::Upgrade
            } else {
                Kind::Install
            },
            package_id: id,
            old,
            new: Some(new),
            was_running,
            start: review.start || was_running,
        };
        op.validate(&self.root.path)?;
        let n = self.database.prepare(&op)?;
        fault.checkpoint("prepared")?;
        if let Err(e) = self.stage_archive(&op, n, &bytes, fault) {
            self.database.diagnostic(n, &e.to_string())?;
            match self.cleanup(&op, n) {
                Ok(()) => self.database.phase(n, "FAILED")?,
                Err(cleanup) => {
                    self.database.phase(n, "RECOVERY_REQUIRED")?;
                    self.database.diagnostic(n, &cleanup.to_string())?;
                }
            }
            return Err(e);
        }
        self.activate(op, n, fault)
    }
    fn activate(&mut self, mut op: Operation, n: i64, fault: &dyn Faults) -> Result<Report> {
        self.database.phase(n, "ACTIVATING")?;
        fault.checkpoint("activating")?;
        let outcome = (|| -> Result<()> {
            if let Some(old) = &op.old {
                self.host.stop(&self.plan(old)?)?;
                self.host.unregister(&self.plan(old)?)?;
            }
            fault.checkpoint("old-stopped")?;
            match op.kind {
                Kind::Install | Kind::Upgrade => {
                    let p = self.plan(op.new.as_ref().ok_or("missing candidate")?)?;
                    let stage = self.stage(&op, n);
                    self.root.check(&stage)?;
                    fs::set_permissions(&stage, fs::Permissions::from_mode(0o755))?;
                    filesystem::sync(&stage)?;
                    filesystem::publish(&stage, &p.layout.version)?;
                    filesystem::sync(p.layout.version.parent().ok_or("missing parent")?)?;
                }
                Kind::Uninstall => {
                    let p = self.plan(op.old.as_ref().ok_or("missing previous")?)?;
                    self.activate_files(&op, None, n)?;
                    let entries = fs::read_dir(p.layout.app.join("versions"))?
                        .take(65)
                        .collect::<std::io::Result<Vec<_>>>()?;
                    if entries.len() != op.old.as_ref().ok_or("missing old record")?.versions.len()
                        || entries.iter().any(|e| {
                            !op.old.as_ref().is_some_and(|r| {
                                e.file_name()
                                    .to_str()
                                    .is_some_and(|n| r.versions.contains_key(n))
                            })
                        })
                    {
                        return fail("untracked version directory; refusing uninstall removal");
                    }
                    filesystem::publish(&p.layout.app.join("versions"), &self.trash(&op, n))?;
                    filesystem::sync(&p.layout.app)?;
                }
                Kind::Rollback => {}
            }
            fault.checkpoint("version-published")?;
            if let Some(new) = &op.new {
                self.activate_files(&op, Some(new), n)?;
                fault.checkpoint("reference-switched")?;
                let p = self.plan(new)?;
                self.host.register(&p)?;
                fault.checkpoint("service-registered")?;
                if op.start {
                    self.host.start(&p)?;
                    self.wait_health(&p)?;
                }
            }
            fault.checkpoint("health-verified")?;
            Ok(())
        })();
        if let Err(e) = outcome {
            self.database.diagnostic(n, &e.to_string())?;
            // A Faults error intentionally models a process crash and is not compensated in-process.
            fault.checkpoint("compensate")?;
            if let Err(recovery) = self.restore(&op, n) {
                self.database.phase(n, "RECOVERY_REQUIRED")?;
                self.database.diagnostic(n, &recovery.to_string())?;
                return fail(format!(
                    "operation failed: {e}; RECOVERY_REQUIRED: {recovery}"
                ));
            }
            self.database.phase(n, "FAILED")?;
            return Err(e);
        }
        if let Some(new) = &mut op.new {
            new.service_state = if self.plan(new)?.program.is_none() {
                "not-required"
            } else if op.start {
                "running"
            } else {
                "stopped"
            }
            .into();
        }
        self.database.commit(n, op.new.as_ref(), &op.package_id)?;
        fault.checkpoint("committed")?;
        match self.cleanup(&op, n) {
            Ok(()) => self.database.cleanup_done(n)?,
            Err(e) => self
                .database
                .diagnostic(n, &format!("committed; cleanup pending: {e}"))?,
        }
        Ok(Report {
            operation: n,
            state: "COMMITTED".into(),
            package_id: op.package_id,
            active: op.new.map(|r| r.active),
            data: "preserved; no migration/data rollback performed".into(),
            health: if op.start {
                "verified through Runtime"
            } else {
                "not run; service startup not requested"
            }
            .into(),
        })
    }
    fn restore(&self, op: &Operation, n: i64) -> Result<()> {
        if let Some(new) = &op.new {
            let p = self.plan(new)?;
            if op.start || p.layout.config.try_exists()? {
                self.host.stop(&p)?;
            }
            // Remove candidate registration before restoring a possibly different old definition.
            if p.layout.config.try_exists()? {
                self.host.unregister(&p)?;
            }
        }
        if matches!(op.kind, Kind::Uninstall) {
            let trash = self.trash(op, n);
            if trash.try_exists()? {
                self.root.check(&trash)?;
                let versions = self.app(&op.package_id).join("versions");
                if versions.try_exists()? {
                    return fail("both live and removed version trees exist");
                }
                fs::rename(trash, versions)?;
                filesystem::sync(&self.app(&op.package_id))?;
            }
        }
        self.activate_files(op, op.old.as_ref(), n)?;
        if let Some(old) = &op.old {
            let p = self.plan(old)?;
            self.host.register(&p)?;
            if op.was_running {
                self.host.start(&p)?;
                self.wait_health(&p)?;
            }
        }
        if matches!(op.kind, Kind::Install | Kind::Upgrade) {
            let new = op.new.as_ref().ok_or("missing candidate")?;
            if !op
                .old
                .as_ref()
                .is_some_and(|r| r.versions.contains_key(&new.active))
            {
                let path = self.plan(new)?.layout.version;
                if path.try_exists()? {
                    self.root.remove_tree(&path)?;
                }
            }
        }
        self.cleanup(op, n)?;
        Ok(())
    }
    fn cleanup(&self, op: &Operation, n: i64) -> Result<()> {
        for p in [self.stage(op, n), self.trash(op, n)] {
            if p.try_exists()? {
                self.root.remove_tree(&p)?;
            }
        }
        Ok(())
    }
    pub fn reconcile(&mut self) -> Result<Vec<String>> {
        let mut reports = vec![];
        for j in self.database.completed(&self.root.path)? {
            if let Err(e) = self.cleanup(&j.operation, j.number) {
                self.database
                    .diagnostic(j.number, &format!("committed cleanup still pending: {e}"))?;
                reports.push(format!("{}: committed cleanup pending: {e}", j.number));
            } else {
                self.database.cleanup_done(j.number)?;
            }
        }
        for j in self.database.pending(&self.root.path)? {
            let op = j.operation;
            let outcome = if j.state == "PREPARED" || j.state == "STAGED" {
                self.cleanup(&op, j.number)
            } else {
                self.restore(&op, j.number)
            };
            match outcome {
                Ok(()) => {
                    self.database.phase(j.number, "FAILED")?;
                    self.database.diagnostic(j.number,"Interrupted operation recovered to last committed version; application data was preserved")?;
                    reports.push(format!("{}: restored last committed state", j.number));
                }
                Err(e) => {
                    self.database.phase(j.number, "RECOVERY_REQUIRED")?;
                    self.database.diagnostic(j.number, &e.to_string())?;
                    reports.push(format!("{}: RECOVERY_REQUIRED: {e}", j.number));
                }
            }
        }
        Ok(reports)
    }
    fn dependents_stopped(&self, id: &str) -> Result<()> {
        let all = self.list()?;
        let mut affected = std::collections::BTreeSet::from([id.to_owned()]);
        loop {
            let count = affected.len();
            for (other, r) in &all {
                if r.current()
                    .parsed()?
                    .runtime
                    .dependencies
                    .iter()
                    .any(|d| affected.contains(&d.id))
                {
                    affected.insert(other.clone());
                }
            }
            if count == affected.len() {
                break;
            }
        }
        for other in affected {
            if other != id {
                let p = self.plan(&all[&other])?;
                if !matches!(
                    self.host.status(&p)?.state,
                    State::Stopped | State::Inactive | State::NotRequired
                ) {
                    return fail(format!("stop dependent package {other} before changing {id}; coordinated multi-package activation is unsupported"));
                }
            }
        }
        Ok(())
    }
    pub fn rollback(&mut self, id: &str, fault: &dyn Faults) -> Result<Report> {
        self.ready()?;
        let old = self.info(id)?;
        self.dependents_stopped(id)?;
        self.verify(id)?;
        let mut new = old.clone();
        let previous = old
            .previous
            .clone()
            .ok_or("no previous executable version retained")?;
        new.previous = Some(old.active.clone());
        new.active = previous;
        let m = new.current().parsed()?;
        resolver::resolve(&self.list()?, Some(&m), None)?;
        if m.compatibility.data_schema != old.current().parsed()?.compatibility.data_schema {
            return fail("data schema differs; automatic binary rollback refused");
        }
        let running = self.host.status(&self.plan(&old)?)?.state == State::Running;
        let op = Operation {
            kind: Kind::Rollback,
            package_id: id.into(),
            old: Some(old),
            new: Some(new),
            was_running: running,
            start: running,
        };
        let n = self.database.prepare(&op)?;
        fault.checkpoint("prepared")?;
        self.database.phase(n, "STAGED")?;
        fault.checkpoint("staged")?;
        self.activate(op, n, fault)
    }
    pub fn uninstall(&mut self, id: &str, fault: &dyn Faults) -> Result<Report> {
        self.ready()?;
        if self.database.retained(&self.root.path)?.len() >= 1024 {
            return fail("retained-data identity budget reached; explicit maintenance required before uninstall");
        }
        let old = self.info(id)?;
        self.verify(id)?;
        resolver::resolve(&self.list()?, None, Some(id))?;
        let running = self.host.status(&self.plan(&old)?)?.state == State::Running;
        let op = Operation {
            kind: Kind::Uninstall,
            package_id: id.into(),
            old: Some(old),
            new: None,
            was_running: running,
            start: false,
        };
        let n = self.database.prepare(&op)?;
        fault.checkpoint("prepared")?;
        self.database.phase(n, "STAGED")?;
        fault.checkpoint("staged")?;
        self.activate(op, n, fault)
    }
    pub fn lifecycle(&self, id: &str, verb: &str) -> Result<Status> {
        self.ready()?;
        let p = if verb == "stop" || verb == "status" {
            self.plan(&self.info(id)?)?
        } else {
            self.verify(id)?
        };
        match verb {
            "start" => {
                self.host.register(&p)?;
                self.host.start(&p)?;
                self.wait_health(&p)?;
            }
            "stop" => self.host.stop(&p)?,
            "restart" => {
                self.host.register(&p)?;
                self.host.restart(&p)?;
                self.wait_health(&p)?;
            }
            "status" => {}
            _ => return fail("unsupported lifecycle command"),
        }
        let status = self.host.status(&p)?;
        let mut r = self.info(id)?;
        r.service_state = match status.state {
            State::Running => "running",
            State::Stopped | State::Inactive => "stopped",
            State::NotRequired => "not-required",
            _ => "unknown",
        }
        .into();
        self.database.service_state(&r)?;
        Ok(status)
    }
    pub fn logs(&self, id: &str, stderr: bool) -> Result<String> {
        self.ready()?;
        self.host.logs(&self.plan(&self.info(id)?)?, stderr)
    }
    pub fn repair_plan(&self, id: &str) -> Result<serde_json::Value> {
        if !zpk_schema::manifest::package_id(id) {
            return fail("invalid package ID");
        }
        let r = self.list()?.remove(id);
        let retained = self.database.retained(&self.root.path)?.contains_key(id);
        Ok(
            serde_json::json!({"id":id,"retained_data":retained,"installed":r.is_some(),"active":r.as_ref().map(|r|&r.active),"previous":r.as_ref().and_then(|r|r.previous.as_ref()),"verification":self.verify(id).map(|_|"ok".to_owned()).unwrap_or_else(|e|e.to_string()),"operations":self.database.operation_states()?,"diagnostics":self.database.history()?,"changes_applied":false,"guidance":"Reconciliation restores only journaled references/config/service state. Preserve application data; do not edit SQLite or overwrite immutable versions. Restore a trusted backup if registry/content is corrupt."}),
        )
    }
}
fn sync_tree(path: &Path) -> Result<()> {
    for e in fs::read_dir(path)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            sync_tree(&e.path())?;
        } else {
            filesystem::sync(&e.path())?;
        }
    }
    filesystem::sync(path)
}
