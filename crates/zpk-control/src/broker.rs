//! Root-only package adapter. Peers AND administrator credentials are independently verified.
use crate::{auth::Auth, fail, hash, ipc, now, protocol::*, random, token, Result};
use rusqlite::params;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Seek, Write},
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            net::UnixListener,
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use zpk_manager::{
    filesystem::Environment,
    host::Alpine,
    manager::{Faults, Manager},
    model::Review,
};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub max_upload: u64,
    pub reserve_bytes: u64,
    #[serde(default = "default_uploads")]
    pub max_uploads: usize,
}
fn default_uploads() -> usize {
    2
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            max_upload: MAX_UPLOAD,
            reserve_bytes: RESERVE,
            max_uploads: 2,
        }
    }
}
impl Policy {
    pub fn read() -> Result<Self> {
        let path = Path::new("/etc/zionos/package-policy.json");
        let p = if path.exists() {
            zpk_runtime::installed::secure_path(Path::new("/"), path)?;
            serde_json::from_slice(&zpk_runtime::installed::read_file(path, 4096)?)?
        } else {
            Self::default()
        };
        if !(128 * 1024..=MAX_UPLOAD).contains(&p.max_upload)
            || !(16 * 1024 * 1024..=1024 * 1024 * 1024).contains(&p.reserve_bytes)
            || !(1..=2).contains(&p.max_uploads)
        {
            return fail("invalid package policy limits");
        }
        Ok(p)
    }
}
pub fn free_bytes(path: &Path) -> Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(s.f_bavail.saturating_mul(s.f_frsize))
}
pub fn capacity(free: u64, needed: u64, reserve: u64) -> Result<()> {
    if needed.checked_add(reserve).is_none_or(|n| n > free) {
        return fail("INSUFFICIENT_STORAGE");
    }
    Ok(())
}
fn manager() -> Result<Manager<Alpine>> {
    Manager::open(Path::new("/"), Environment::Persistent, Alpine)
}
pub struct Broker {
    manager_gate: Mutex<()>,
    auth: Mutex<Auth>,
    busy: AtomicBool,
    connections: AtomicUsize,
    pub uid: u32,
    gid: u32,
    spool: PathBuf,
    policy: Policy,
}
impl Broker {
    pub fn open(uid: u32, gid: u32) -> Result<Arc<Self>> {
        crate::platform::target()?;
        let spool = Path::new(STATE).join("previews");
        fs::create_dir_all(&spool)?;
        fs::set_permissions(&spool, fs::Permissions::from_mode(0o700))?;
        zpk_runtime::installed::secure_path(Path::new("/"), &spool)?;
        let auth = Auth::open(&Path::new(STATE).join("control.sqlite3"))?;
        auth.db.execute("UPDATE operations SET state='RECOVERY_REQUIRED',detail='Broker restarted; Manager journal reconciled; inspect installed state' WHERE state NOT IN ('COMMITTED','FAILED','RECOVERY_REQUIRED')",[])?;
        // Preview receipts never survive a broker restart; stale files are removed without following links.
        auth.db.execute("DELETE FROM previews", [])?;
        for entry in fs::read_dir(&spool)? {
            let e = entry?;
            if !token(&e.file_name().to_string_lossy()) || !e.file_type()?.is_file() {
                return fail("unsafe preview spool entry");
            }
            fs::remove_file(e.path())?;
        }
        Ok(Arc::new(Self {
            manager_gate: Mutex::new(()),
            auth: Mutex::new(auth),
            busy: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
            uid,
            gid,
            spool,
            policy: Policy::read()?,
        }))
    }
    fn authorize(&self, a: &Credentials) -> Result<()> {
        self.auth
            .lock()
            .map_err(|_| "authentication unavailable")?
            .authorize(&a.session, Some(&a.csrf))
    }
    fn read_auth(&self, s: &str) -> Result<()> {
        self.auth
            .lock()
            .map_err(|_| "authentication unavailable")?
            .authorize(s, None)
    }
    fn preview(self: &Arc<Self>, session: &str, mut file: fs::File) -> Result<Value> {
        let m = file.metadata()?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != self.uid
            || m.mode() & 0o077 != 0
            || m.len() > self.policy.max_upload
            || m.len() == 0
        {
            return fail("invalid package descriptor");
        }
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
        if flags < 0 || flags & libc::O_ACCMODE != libc::O_RDONLY {
            return fail("package descriptor must be read-only");
        }
        file.rewind()?;
        let mut bytes = vec![];
        file.take(self.policy.max_upload + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > self.policy.max_upload {
            return fail("package size limit");
        }
        self.preview_bytes(session, &bytes)
    }
    fn preview_bytes(self: &Arc<Self>, session: &str, bytes: &[u8]) -> Result<Value> {
        let _manager = self
            .manager_gate
            .lock()
            .map_err(|_| "manager access unavailable")?;
        let inspection =
            zpk_schema::inspect(std::io::Cursor::new(bytes), zpk_schema::Limits::default())?;
        let m = &inspection.manifest;
        let mgr = manager()?;
        let installed = mgr.list()?;
        let old = installed.get(&m.package.id);
        m.check_compatibility(
            &"0.7.0".parse()?,
            old.map(|r| &r.current().configuration.version),
        )?;
        zpk_manager::resolver::resolve(&installed, Some(m), None)?;
        let expanded = inspection
            .files
            .values()
            .try_fold(0u64, |a, f| a.checked_add(f.size).ok_or("size overflow"))?;
        capacity(
            free_bytes(&self.spool)?,
            expanded
                .saturating_mul(2)
                .saturating_add(bytes.len() as u64),
            self.policy.reserve_bytes,
        )?;
        let id = random()?;
        let digest = hash(bytes);
        let auth = self.auth.lock().map_err(|_| "authentication unavailable")?;
        let time = now()?;
        let mut query = auth
            .db
            .prepare("SELECT token FROM previews WHERE expires<?1")?;
        let expired = query
            .query_map([time], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(query);
        for id in expired {
            if token(&id) {
                let _ = fs::remove_file(self.spool.join(&id));
            }
            auth.db
                .execute("DELETE FROM previews WHERE token=?1", [id])?;
        }
        let count: i64 = auth
            .db
            .query_row("SELECT count(*) FROM previews", [], |r| r.get(0))?;
        if count >= 4 {
            return fail(
                "PREVIEW_CAPACITY: at most four pending previews; wait for expiry or install",
            );
        }
        let path = self.spool.join(&id);
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::File::open(&self.spool)?.sync_all()?;
        auth.db.execute(
            "INSERT INTO previews VALUES(?1,?2,?3,?4)",
            params![id, hash(session.as_bytes()), digest, time + 600],
        )?;
        Ok(
            json!({"receipt":id,"manifest":m,"sha256":digest,"authenticated_publisher":false,"installed_version":old.map(|r|&r.active),"expanded_bytes":expanded,"expires_in":600,"warning":"Unsigned local development only. Hashes do not authenticate publishers. No package lifecycle scripts run as root."}),
        )
    }
    fn begin(
        self: &Arc<Self>,
        kind: &str,
        id: &str,
        work: impl FnOnce(&Progress) -> Result<Value> + Send + 'static,
    ) -> Result<Value> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return fail("BUSY: another package operation is active");
        }
        let operation = random()?;
        {
            let a = self.auth.lock().map_err(|_| "authentication unavailable")?;
            a.db.execute(
                "INSERT INTO operations VALUES(?1,?2,?3,'QUEUED','',?4)",
                params![operation, kind, id, now()?],
            )?;
            a.audit(kind, "queued")?;
        }
        let this = Arc::clone(self);
        let op = operation.clone();
        std::thread::spawn(move || {
            let progress = Progress {
                broker: Arc::clone(&this),
                id: op,
            };
            let _ = progress.set("RUNNING", "");
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _manager = this
                    .manager_gate
                    .lock()
                    .map_err(|_| "manager access unavailable")?;
                work(&progress)
            }));
            // The Manager guard is already released. Publish terminal state only after
            // readers can refresh, avoiding a COMMITTED -> transient BUSY UI race.
            this.busy.store(false, Ordering::SeqCst);
            match result {
                Ok(Ok(data)) => {
                    let _ = progress.set("COMMITTED", &data.to_string());
                }
                Ok(Err(e)) => {
                    let _ = progress.set("FAILED", &e.to_string());
                }
                Err(_) => {
                    let _ = progress.set(
                        "RECOVERY_REQUIRED",
                        "Worker interrupted; local reconciliation required",
                    );
                }
            }
        });
        Ok(json!({"operation_id":operation,"state":"QUEUED"}))
    }
    fn dispatch(self: &Arc<Self>, request: Request, fd: Option<OwnedFd>) -> Result<Value> {
        if !matches!(request, Request::Preview { .. }) && fd.is_some() {
            return fail("unexpected file descriptor");
        }
        match request {
            Request::Status {} => {
                let a = self.auth.lock().map_err(|_| "authentication unavailable")?;
                Ok(
                    json!({"available":true,"configured":a.configured()?,"busy":self.busy.load(Ordering::SeqCst),"max_upload":self.policy.max_upload,"max_uploads":self.policy.max_uploads,"reserve_bytes":self.policy.reserve_bytes,"transport":"localhost-development-only","remote_administration":false}),
                )
            }
            Request::Setup { code, password } => {
                self.auth
                    .lock()
                    .map_err(|_| "authentication unavailable")?
                    .setup(&code, &password)?;
                Ok(json!({"configured":true}))
            }
            Request::Login { password } => {
                let (s, c) = self
                    .auth
                    .lock()
                    .map_err(|_| "authentication unavailable")?
                    .login(&password)?;
                Ok(json!({"session":s,"csrf":c,"role":"administrator","expires_in":28800}))
            }
            Request::Session { session } => {
                let a = self.auth.lock().map_err(|_| "authentication unavailable")?;
                let csrf = a.csrf(&session)?;
                Ok(json!({"role":"administrator","csrf":csrf}))
            }
            Request::Logout { auth } => {
                self.auth
                    .lock()
                    .map_err(|_| "authentication unavailable")?
                    .logout(&auth.session, &auth.csrf)?;
                Ok(json!({"logged_out":true}))
            }
            Request::Preview { auth } => {
                self.authorize(&auth)?;
                self.preview(
                    &auth.session,
                    fs::File::from(fd.ok_or("preview requires exactly one descriptor")?),
                )
            }
            Request::Installed { session } => {
                self.read_auth(&session)?;
                if self.busy.load(Ordering::SeqCst) {
                    return fail("BUSY: package state is being changed");
                }
                let _manager = self
                    .manager_gate
                    .lock()
                    .map_err(|_| "manager access unavailable")?;
                let m = manager()?;
                let mut apps = vec![];
                for (id, r) in m.list()? {
                    let manifest = r.current().parsed()?;
                    let state = m.lifecycle(&id, "status")?;
                    apps.push(json!({"id":id,"manifest":manifest,"active":r.active,"previous":r.previous,"state":state,"effective_permissions":r.current().configuration.grants,"runtime_policy":r.current().runtime_policy,"desktop_enabled":r.desktop_enabled,"storage_usage_bytes":Value::Null,"storage_usage_reason":"Aggregate filesystem accounting is not implemented; declared storage limits are not quotas"}));
                }
                Ok(json!({"apps":apps}))
            }
            Request::Logs {
                session,
                id,
                stderr,
            } => {
                self.read_auth(&session)?;
                if self.busy.load(Ordering::SeqCst) {
                    return fail("BUSY: package state is being changed");
                }
                let _manager = self
                    .manager_gate
                    .lock()
                    .map_err(|_| "manager access unavailable")?;
                valid_id(&id)?;
                Ok(json!({"data":manager()?.logs(&id,stderr)?}))
            }
            Request::Progress { session, id } => {
                self.read_auth(&session)?;
                if !token(&id) {
                    return fail("invalid operation id");
                }
                let a = self.auth.lock().map_err(|_| "authentication unavailable")?;
                Ok(a.db.query_row("SELECT kind,package,state,detail,time FROM operations WHERE id=?1",[&id],|r|Ok(json!({"id":id,"kind":r.get::<_,String>(0)?,"package_id":r.get::<_,String>(1)?,"state":r.get::<_,String>(2)?,"detail":r.get::<_,String>(3)?,"time":r.get::<_,i64>(4)?})))?)
            }
            Request::Install {
                auth,
                receipt,
                upgrade,
                permissions,
                trusted_local,
                start,
                autostart,
                unix_health,
            } => {
                self.authorize(&auth)?;
                if !token(&receipt) || !trusted_local {
                    return fail("explicit trusted-local development approval required");
                }
                let (s, d, t): (String, String, i64) = self
                    .auth
                    .lock()
                    .map_err(|_| "authentication unavailable")?
                    .db
                    .query_row(
                        "SELECT session,digest,expires FROM previews WHERE token=?1",
                        [&receipt],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )?;
                if s != hash(auth.session.as_bytes()) || t < now()? {
                    return fail("expired or foreign preview receipt");
                }
                let path = self.spool.join(&receipt);
                let bytes = zpk_runtime::installed::read_file(&path, self.policy.max_upload)?;
                if hash(&bytes) != d {
                    return fail("preview hash changed");
                }
                let inspection = zpk_schema::inspect(
                    std::io::Cursor::new(&bytes),
                    zpk_schema::Limits::default(),
                )?;
                if permissions != inspection.manifest.permissions {
                    return fail("permission review must exactly match preview");
                }
                let expanded: u64 = inspection.files.values().map(|f| f.size).sum();
                capacity(
                    free_bytes(&self.spool)?,
                    expanded.saturating_mul(2),
                    self.policy.reserve_bytes,
                )?;
                let id = inspection.manifest.package.id.clone();
                let this = Arc::clone(self);
                let receipt_copy = receipt.clone();
                let result =
                    self.begin(if upgrade { "upgrade" } else { "install" }, &id, move |p| {
                        let mut m = manager()?;
                        m.reconcile()?;
                        let result = m.install(
                            &path,
                            upgrade,
                            &Review {
                                desktop_entry: None,
                                trusted_local_development: true,
                                approved_permissions: permissions,
                                start,
                                autostart,
                                unix_health,
                            },
                            p,
                        );
                        let _ = fs::remove_file(&path);
                        let _ = this
                            .auth
                            .lock()
                            .map_err(|_| "authentication unavailable")?
                            .db
                            .execute("DELETE FROM previews WHERE token=?1", [receipt_copy]);
                        Ok(serde_json::to_value(result?)?)
                    });
                result
            }
            Request::Lifecycle { auth, id, action } => {
                self.authorize(&auth)?;
                valid_id(&id)?;
                let package = id.clone();
                self.begin(action.name(), &package, move |p| {
                    let mut m = manager()?;
                    m.reconcile()?;
                    match action {
                        Action::Start | Action::Stop | Action::Restart => {
                            Ok(serde_json::to_value(m.lifecycle(&id, action.name())?)?)
                        }
                        Action::Rollback => Ok(serde_json::to_value(m.rollback(&id, p)?)?),
                        Action::Uninstall => Ok(serde_json::to_value(m.uninstall(&id, p)?)?),
                        Action::Verify => {
                            m.verify(&id)?;
                            Ok(json!({"verified":true}))
                        }
                    }
                })
            }
            Request::Catalog { session } => {
                self.read_auth(&session)?;
                Ok(json!({"packages":catalog()?}))
            }
            Request::CatalogPreview { auth, id } => {
                self.authorize(&auth)?;
                if !token(&id) {
                    return fail("invalid catalog id");
                }
                let entries = catalog()?;
                let entry = entries
                    .iter()
                    .find(|e| e["id"] == id)
                    .ok_or("catalog entry unavailable")?;
                let path = Path::new(STATE).join("catalog").join(format!("{id}.zpk"));
                let bytes = zpk_runtime::installed::read_file(&path, self.policy.max_upload)?;
                if hash(&bytes) != entry["sha256"].as_str().ok_or("invalid catalog hash")? {
                    return fail("catalog hash mismatch");
                }
                self.preview_bytes(&auth.session, &bytes)
            }
        }
    }
    pub fn serve(self: Arc<Self>) -> Result<()> {
        let path = Path::new(SOCKET);
        if path.exists() {
            fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o660))?;
        let c = std::ffi::CString::new(SOCKET)?;
        if unsafe { libc::chown(c.as_ptr(), 0, self.gid) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else { continue };
            let this = Arc::clone(&self);
            if self.connections.fetch_add(1, Ordering::SeqCst) >= 8 {
                self.connections.fetch_sub(1, Ordering::SeqCst);
                continue;
            }
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                let reply = match ipc::peer(&stream) {
                    Ok(peer) if peer.uid == this.uid && peer.pid > 1 => {
                        match ipc::receive(&mut stream, FRAME)
                            .and_then(|(b, f)| Ok((serde_json::from_slice::<Request>(&b)?, f)))
                            .and_then(|(r, f)| this.dispatch(r, f))
                        {
                            Ok(data) => Reply::success(data),
                            Err(e) => typed_error(&e.to_string()),
                        }
                    }
                    _ => Reply::error("IPC_DENIED", "Unauthorized local service identity"),
                };
                if let Ok(b) = serde_json::to_vec(&reply) {
                    if b.len() <= RESPONSE {
                        let _ = ipc::send(&mut stream, &b, None);
                    }
                }
                this.connections.fetch_sub(1, Ordering::SeqCst);
            });
        }
        Ok(())
    }
}
fn valid_id(id: &str) -> Result<()> {
    if !zpk_schema::manifest::package_id(id) {
        return fail("invalid package id");
    }
    Ok(())
}
fn typed_error(s: &str) -> Reply {
    let code = if s.contains("CSRF") {
        "CSRF_DENIED"
    } else if s.contains("UNAUTHENTICATED") || s.contains("SESSION_EXPIRED") {
        "UNAUTHENTICATED"
    } else if s.contains("RATE_LIMITED") {
        "RATE_LIMITED"
    } else if s.contains("BUSY") || s.contains("lock") {
        "BUSY"
    } else if s.contains("INSUFFICIENT_STORAGE") {
        "INSUFFICIENT_STORAGE"
    } else {
        "REJECTED"
    };
    Reply::error(code, s)
}
pub struct Progress {
    broker: Arc<Broker>,
    id: String,
}
impl Progress {
    fn set(&self, state: &str, detail: &str) -> Result<()> {
        let a = self
            .broker
            .auth
            .lock()
            .map_err(|_| "authentication unavailable")?;
        let text: String = detail
            .chars()
            .take(4096)
            .filter(|c| !c.is_control())
            .collect();
        a.db.execute(
            "UPDATE operations SET state=?1,detail=?2 WHERE id=?3",
            params![state, text, self.id],
        )?;
        if ["COMMITTED", "FAILED", "RECOVERY_REQUIRED"].contains(&state) {
            a.audit("package operation", state)?;
        }
        Ok(())
    }
}
impl Faults for Progress {
    fn checkpoint(&self, point: &str) -> Result<()> {
        self.set(point, "")
    }
}
fn catalog() -> Result<Vec<Value>> {
    let root = Path::new(STATE).join("catalog");
    let index = root.join("index.json");
    if !index.exists() {
        return Ok(vec![]);
    }
    zpk_runtime::installed::secure_path(Path::new("/"), &index)?;
    let entries: Vec<Value> = serde_json::from_slice(&zpk_runtime::installed::read_file(
        &index,
        CATALOG_INDEX_LIMIT,
    )?)?;
    if entries.len() > 32
        || entries.iter().any(|e| {
            !e["id"].as_str().is_some_and(token)
                || !e["sha256"].as_str().is_some_and(token)
                || e["name"].as_str().is_none_or(|n| n.len() > 128)
        })
    {
        return fail("invalid local curated catalog");
    }
    Ok(entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_overflows_and_reserve_fail_closed() {
        assert!(capacity(100, 80, 21).is_err());
        assert!(capacity(u64::MAX, u64::MAX, 1).is_err());
        capacity(100, 80, 20).unwrap();
    }
    #[test]
    fn unknown_claims_and_operations_fail_closed() {
        for s in [
            r#"{"operation":"shell","command":"id"}"#,
            r#"{"operation":"installed","session":"x","authorized":true}"#,
            r#"{"operation":"status","authorized":true}"#,
        ] {
            assert!(serde_json::from_str::<Request>(s).is_err());
        }
        assert!(valid_id("../../etc").is_err());
    }
    #[test]
    fn conflicting_operations_are_serialized_and_report_real_checkpoints() {
        let d = tempfile::tempdir().unwrap();
        let b = Arc::new(Broker {
            manager_gate: Mutex::new(()),
            auth: Mutex::new(Auth::open(&d.path().join("db")).unwrap()),
            busy: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
            uid: unsafe { libc::geteuid() },
            gid: unsafe { libc::getegid() },
            spool: d.path().into(),
            policy: Policy::default(),
        });
        let gate = Arc::new(std::sync::Barrier::new(2));
        let g = Arc::clone(&gate);
        let r = b
            .begin("test", "org.zionos.test", move |p| {
                p.checkpoint("staged")?;
                g.wait();
                Ok(json!({"verified":true}))
            })
            .unwrap();
        assert!(b
            .begin("test", "org.zionos.test", |_| Ok(Value::Null))
            .unwrap_err()
            .to_string()
            .contains("BUSY"));
        gate.wait();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let state: String = b
                .auth
                .lock()
                .unwrap()
                .db
                .query_row(
                    "SELECT state FROM operations WHERE id=?1",
                    [r["operation_id"].as_str().unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            if state == "COMMITTED" {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "operation did not commit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn descriptor_types_modes_links_and_sizes_fail_before_any_native_action() {
        let d = tempfile::tempdir().unwrap();
        let uid = unsafe { libc::geteuid() };
        let b = Arc::new(Broker {
            manager_gate: Mutex::new(()),
            auth: Mutex::new(Auth::open(&d.path().join("db")).unwrap()),
            busy: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
            uid,
            gid: unsafe { libc::getegid() },
            spool: d.path().into(),
            policy: Policy::default(),
        });
        assert!(b.preview("", fs::File::open(d.path()).unwrap()).is_err());
        let path = d.path().join("file");
        fs::write(&path, b"data").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let writable = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(b.preview("", writable).is_err());
        fs::hard_link(&path, d.path().join("link")).unwrap();
        assert!(b.preview("", fs::File::open(&path).unwrap()).is_err());
        fs::remove_file(d.path().join("link")).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_UPLOAD + 1)
            .unwrap();
        assert!(b.preview("", fs::File::open(&path).unwrap()).is_err());
    }
}
