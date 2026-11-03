//! Trusted paths and durable writes; no package-derived absolute destinations.
use crate::{fail, Result};
use std::{
    fs,
    io::Write,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Environment {
    Persistent,
    TemporaryTest,
}
pub struct Root {
    pub path: PathBuf,
    pub environment: Environment,
}
impl Root {
    pub fn new(path: &Path, environment: Environment) -> Result<Self> {
        if !path.is_absolute() || fs::canonicalize(path)? != path {
            return fail("manager root must be absolute and canonical");
        }
        if environment == Environment::TemporaryTest
            && (path == Path::new("/")
                || fs::symlink_metadata(path)?.uid() != unsafe { libc::geteuid() })
        {
            return fail("test root must be private, owned, and distinct from the system root");
        }
        if environment == Environment::Persistent
            && (path != Path::new("/") || unsafe { libc::geteuid() } != 0)
        {
            return fail("persistent administration requires root on the target OS; use library test roots only for mocks");
        }
        let r = Self {
            path: path.into(),
            environment,
        };
        r.check(path)?;
        Ok(r)
    }
    pub fn join(&self, s: &str) -> PathBuf {
        self.path.join(s)
    }
    pub fn check(&self, path: &Path) -> Result<()> {
        let relative = path.strip_prefix(&self.path)?;
        let mut p = self.path.clone();
        self.metadata(&p)?;
        for c in relative.components() {
            if !matches!(c, std::path::Component::Normal(_)) {
                return fail("noncanonical manager path");
            }
            p.push(c);
            self.metadata(&p)?;
        }
        Ok(())
    }
    fn metadata(&self, path: &Path) -> Result<()> {
        let m = fs::symlink_metadata(path)?;
        if m.file_type().is_symlink()
            || m.mode() & 0o022 != 0
            || (self.environment == Environment::Persistent && m.uid() != 0)
        {
            return fail(format!("untrusted manager path: {}", path.display()));
        }
        Ok(())
    }
    pub fn mkdir(&self, path: &Path, mode: u32) -> Result<()> {
        let mut p = self.path.clone();
        self.check(&p)?;
        for c in path.strip_prefix(&self.path)?.components() {
            if !matches!(c, std::path::Component::Normal(_)) {
                return fail("noncanonical manager directory");
            }
            p.push(c);
            match fs::symlink_metadata(&p) {
                Ok(m) => {
                    self.check(&p)?;
                    if !m.is_dir() {
                        return fail("manager directory is not a directory");
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&p)?;
                    fs::set_permissions(&p, fs::Permissions::from_mode(mode))?;
                    sync(p.parent().ok_or("missing parent")?)?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    pub fn persistent(&self, path: &Path) -> Result<()> {
        self.check(path)?;
        if self.environment == Environment::TemporaryTest {
            return Ok(());
        }
        let p = std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()))?;
        let mut s: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(p.as_ptr(), &mut s) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut v: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(p.as_ptr(), &mut v) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if ![0xef53i64, 0x58465342, 0x9123683e].contains(&(s.f_type as i64))
            || v.f_flag & libc::ST_RDONLY != 0
        {
            return fail("persistent manager targets require writable local ext4/XFS/Btrfs; RAM, overlay, network and read-only filesystems are refused");
        }
        Ok(())
    }
    pub fn remove_tree(&self, path: &Path) -> Result<()> {
        self.check(path)?;
        // Refuse foreign/symlink descendants before recursive removal. Never follow symlinks.
        for e in fs::read_dir(path)? {
            let e = e?;
            self.check(&e.path())?;
            if e.file_type()?.is_dir() {
                self.remove_tree(&e.path())?;
            } else {
                fs::remove_file(e.path())?;
            }
        }
        fs::remove_dir(path)?;
        sync(path.parent().ok_or("missing parent")?)
    }
}
pub fn sync(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}
pub fn publish(source: &Path, destination: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let from = CString::new(source.as_os_str().as_bytes())?;
    let to = CString::new(destination.as_os_str().as_bytes())?;
    if unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            1u32,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    sync(destination.parent().ok_or("missing publication parent")?)
}
pub fn atomic(root: &Root, path: &Path, bytes: &[u8], tag: &str) -> Result<()> {
    let parent = path.parent().ok_or("missing parent")?;
    root.check(parent)?;
    if let Ok(m) = fs::symlink_metadata(path) {
        root.check(path)?;
        if !m.is_file() || m.nlink() != 1 {
            return fail("unsafe atomic-write destination");
        }
    }
    let temporary = parent.join(format!(
        ".zpk-write-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temporary)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(&temporary, path)?;
    sync(parent)
}
pub struct Lock(pub fs::File);
impl Lock {
    pub fn acquire(root: &Root, path: &Path) -> Result<Self> {
        root.check(path.parent().ok_or("missing lock parent")?)?;
        let f = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        root.check(path)?;
        if !f.metadata()?.is_file()
            || f.metadata()?.nlink() != 1
            || f.metadata()?.mode() & 0o777 != 0o600
        {
            return fail("unsafe manager lock");
        }
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
            return fail("another package manager operation holds the lock");
        }
        Ok(Self(f))
    }
}
