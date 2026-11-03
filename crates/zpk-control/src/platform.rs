//! Explicit UUID-only ext4 application volume. No formatting, partitioning or automatic repair.
use crate::{auth::Auth, broker::Broker, fail, protocol::*, Result};
use serde_json::json;
use std::{
    fs,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use zpk_manager::{
    filesystem::Environment,
    host::{Alpine, Host},
    manager::Manager,
};
use zpk_runtime::backend::{ServiceBackend, State};
pub fn target() -> Result<()> {
    if unsafe { libc::geteuid() } != 0
        || !Path::new("/etc/alpine-release").is_file()
        || !Path::new("/etc/zionos-release").is_file()
    {
        return fail("platform administration is restricted to root on target Alpine/ZionOS; never on Windows/WSL");
    };
    Ok(())
}
fn ready() -> Result<()> {
    target()?;
    if !Path::new("/run/zionos/platform-ready").is_file() {
        return fail("persistent application platform is unavailable");
    };
    zpk_manager::filesystem::Root::new(Path::new("/"), Environment::Persistent)?
        .persistent(Path::new(STATE))
}
fn command(program: &str, args: &[&str]) -> Result<String> {
    device_command(program, args, None)
}
fn device_command(program: &str, args: &[&str], device: Option<&fs::File>) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(match device {
            Some(f) => Stdio::from(f.try_clone()?),
            None => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let output = child.stdout.take().ok_or("command output")?;
    let reader = std::thread::spawn(move || {
        let mut b = String::new();
        output.take(8193).read_to_string(&mut b).map(|_| b)
    });
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            let text = reader.join().map_err(|_| "command reader")??;
            if !status.success() || text.len() > 8192 {
                return fail(format!(
                    "{program} failed; persistent storage was not activated"
                ));
            }
            return Ok(text);
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            return fail("platform command timeout");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn disk_sequence(device: &fs::File) -> Result<u64> {
    let mut seq = 0u64;
    if unsafe { libc::ioctl(device.as_raw_fd(), 0x80081280u32 as _, &mut seq) } < 0 {
        return fail("stable block-device identity is unavailable");
    };
    Ok(seq)
}
fn mount_device(device: &fs::File, readonly: bool) -> Result<()> {
    let source = std::ffi::CString::new(format!("/proc/self/fd/{}", device.as_raw_fd()))?;
    let target = std::ffi::CString::new(STATE)?;
    let kind = std::ffi::CString::new("ext4")?;
    let options = std::ffi::CString::new(if readonly { "noload" } else { "" })?;
    let flags = libc::MS_NODEV
        | libc::MS_NOSUID
        | if readonly {
            libc::MS_RDONLY | libc::MS_NOEXEC
        } else {
            0
        };
    if unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            kind.as_ptr(),
            flags,
            options.as_ptr().cast(),
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
pub fn identity() -> Result<(u32, u32)> {
    target()?;
    let name = std::ffi::CString::new("zion-core")?;
    unsafe {
        if libc::getpwnam(name.as_ptr()).is_null() {
            if !libc::getgrnam(name.as_ptr()).is_null() {
                return fail("Core group collision");
            }
            command("/usr/sbin/addgroup", &["-S", "zion-core"])?;
            command(
                "/usr/sbin/adduser",
                &[
                    "-S",
                    "-D",
                    "-H",
                    "-s",
                    "/sbin/nologin",
                    "-G",
                    "zion-core",
                    "zion-core",
                ],
            )?;
        }
    }
    unsafe {
        let p = libc::getpwnam(name.as_ptr());
        let g = libc::getgrnam(name.as_ptr());
        if p.is_null()
            || g.is_null()
            || (*p).pw_uid == 0
            || (*p).pw_uid == 65534
            || (*p).pw_uid >= 10000
            || (*g).gr_gid == 0
            || (*g).gr_gid == 65534
            || (*p).pw_gid != (*g).gr_gid
        {
            return fail("unsafe Core identity");
        }
        let uid = (*p).pw_uid;
        let gid = (*p).pw_gid;
        let shadow = libc::getspnam(name.as_ptr());
        if shadow.is_null()
            || !matches!(
                std::ffi::CStr::from_ptr((*shadow).sp_pwdp)
                    .to_bytes()
                    .first(),
                Some(b'!' | b'*')
            )
        {
            return fail("Core service account must be password-locked");
        }
        let shell = std::ffi::CStr::from_ptr((*p).pw_shell).to_str()?;
        if shell != "/sbin/nologin" {
            return fail("Core must have a nologin identity");
        }
        libc::setpwent();
        let mut users = 0;
        loop {
            let u = libc::getpwent();
            if u.is_null() {
                break;
            }
            if (*u).pw_uid == uid {
                users += 1
            }
        }
        libc::endpwent();
        if users != 1 {
            return fail("shared Core UID refused");
        }
        Ok((uid, gid))
    }
}
pub fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
            }
        })
}
fn configured_uuid() -> Result<String> {
    let config = Path::new("/etc/zionos/apps-volume.uuid");
    let value = if config.exists() {
        zpk_runtime::installed::secure_path(Path::new("/"), config)?;
        String::from_utf8(zpk_runtime::installed::read_file(config, 80)?)?
    } else if fs::read_to_string("/sys/class/dmi/id/sys_vendor").is_ok_and(|s| s.trim() == "QEMU") {
        fs::read_to_string("/sys/firmware/qemu_fw_cfg/by_name/opt/zionos/apps-uuid/raw")
            .unwrap_or_default()
    } else {
        String::new()
    };
    let value = value.trim().trim_end_matches('\0').to_owned();
    if !uuid(&value) {
        return fail(
            "No valid persistent application UUID configured; local root setup is required",
        );
    };
    Ok(value)
}
fn mkdir(path: &Path, mode: u32) -> Result<()> {
    if let Ok(m) = fs::symlink_metadata(path) {
        if !m.is_dir() || m.file_type().is_symlink() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return fail("unsafe platform directory");
        }
    } else {
        fs::create_dir_all(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    zpk_runtime::installed::secure_path(Path::new("/"), path)?;
    Ok(())
}
fn public_status(available: bool, detail: &str) -> Result<()> {
    fs::create_dir_all("/run/zionos")?;
    let path = Path::new(STATUS);
    let tmp = path.with_extension("new");
    fs::write(
        &tmp,
        serde_json::to_vec(
            &json!({"available":available,"detail":detail,"filesystem":"ext4","formatting_supported":false,"remote_administration":false}),
        )?,
    )?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644))?;
    fs::rename(tmp, path)?;
    Ok(())
}
pub fn degraded(detail: &str) -> Result<()> {
    let _ = fs::remove_file("/run/zionos/platform-ready");
    public_status(false, detail)
}
fn activate_volume() -> Result<()> {
    target()?;
    let (uid, gid) = identity()?;
    let uuid = configured_uuid()?;
    command("/sbin/modprobe", &["virtio_pci"])?;
    command("/sbin/modprobe", &["virtio_blk"])?;
    command("/sbin/modprobe", &["ext4"])?;
    command("/sbin/mdev", &["-s"])?;
    let matches = command(
        "/sbin/blkid",
        &["-t", &format!("UUID={uuid}"), "-o", "device"],
    )?;
    let paths = matches.lines().collect::<Vec<_>>();
    if paths.len() != 1 {
        return fail("configured UUID is missing or ambiguous");
    }
    let device = fs::canonicalize(paths[0])?;
    if !device.starts_with("/dev") || !fs::metadata(&device)?.file_type().is_block_device() {
        return fail("UUID did not identify a block device");
    }
    let m = fs::metadata(&device)?;
    let number = format!("{}:{}", libc::major(m.rdev()), libc::minor(m.rdev()));
    let mountinfo = fs::read_to_string("/proc/self/mountinfo")?;
    if mountinfo
        .lines()
        .any(|l| l.split_whitespace().nth(2) == Some(&number))
    {
        return fail(
            "configured application volume is already mounted; refusing system/foreign mount",
        );
    }
    let device = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&device)?;
    if !device.metadata()?.file_type().is_block_device() || device.metadata()?.rdev() != m.rdev() {
        return fail("block-device identity changed");
    }
    let seq = disk_sequence(&device)?;
    let observed = device_command(
        "/sbin/blkid",
        &["-s", "UUID", "-o", "value", "/proc/self/fd/0"],
        Some(&device),
    )?;
    if observed.trim() != uuid {
        return fail("configured device UUID changed");
    }
    let kind = device_command(
        "/sbin/blkid",
        &["-s", "TYPE", "-o", "value", "/proc/self/fd/0"],
        Some(&device),
    )?;
    if kind.trim() != "ext4" {
        return fail("only preformatted ext4 application volumes are supported");
    }
    // Read-only check, no journal replay or repair. An unclean/corrupt volume is degraded.
    device_command(
        "/sbin/e2fsck",
        &["-f", "-n", "/proc/self/fd/0"],
        Some(&device),
    )?;
    mkdir(Path::new(STATE), 0o700)?;
    mount_device(&device, true)?;
    let marker = Path::new(STATE).join(".zionos-app-volume");
    let checked = (|| -> Result<()> {
        let root = fs::metadata(STATE)?;
        if root.uid() != 0 || root.mode() & 0o077 != 0 {
            return fail("application volume root must be root-owned mode 0700");
        }
        zpk_runtime::installed::secure_path(Path::new("/"), &marker)?;
        if zpk_runtime::installed::read_file(&marker, 128)?
            != format!("ZIONOS_APPS_V1\n{uuid}\n").as_bytes()
        {
            return fail("application volume marker mismatch");
        };
        Ok(())
    })();
    command("/bin/umount", &[STATE])?;
    checked?;
    if disk_sequence(&device)? != seq {
        return fail("application disk changed during validation");
    }
    mount_device(&device, false)?;
    for (name, destination, mode) in [
        ("apps", "/opt/zionos/apps", 0o755),
        ("config", "/etc/zionos/apps", 0o755),
        ("state", "/var/lib/zionos/apps", 0o755),
        ("logs", "/var/log/zionos/apps", 0o755),
        ("registry", "/var/lib/zionos/packages", 0o700),
    ] {
        let source = Path::new(STATE).join(name);
        mkdir(&source, mode)?;
        mkdir(Path::new(destination), mode)?;
        command(
            "/bin/mount",
            &[
                "--bind",
                source.to_str().ok_or("mount source")?,
                destination,
            ],
        )?;
    }
    let uploads = Path::new(STATE).join("uploads");
    if !uploads.exists() {
        fs::create_dir(&uploads)?;
    }
    let meta = fs::symlink_metadata(&uploads)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return fail("unsafe upload directory");
    }
    // Clear only orphaned Core upload files; never follow entries or remove other data.
    for e in fs::read_dir(&uploads)? {
        let e = e?;
        if !crate::token(&e.file_name().to_string_lossy()) || !e.file_type()?.is_file() {
            return fail("unsafe upload staging entry");
        }
        fs::remove_file(e.path())?;
    }
    fs::set_permissions(&uploads, fs::Permissions::from_mode(0o700))?;
    let c = std::ffi::CString::new(uploads.to_str().ok_or("uploads path")?)?;
    if unsafe { libc::chown(c.as_ptr(), uid, gid) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    mkdir(Path::new(UPLOADS), 0o755)?;
    command(
        "/bin/mount",
        &["--bind", uploads.to_str().ok_or("upload source")?, UPLOADS],
    )?;
    let control = Path::new("/run/zionos/control");
    mkdir(control, 0o750)?;
    let c = std::ffi::CString::new(control.to_str().ok_or("control path")?)?;
    if unsafe { libc::chown(c.as_ptr(), 0, gid) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    fs::write("/run/zionos/platform-ready", b"ZION_PLATFORM_V1\n")?;
    public_status(
        true,
        "Persistent application volume mounted; administrator authentication required",
    )
}
pub fn volume() -> Result<()> {
    target()?;
    if let Err(e) = activate_volume() {
        degraded(&e.to_string())?;
    }
    Ok(())
}
pub fn restore() -> Result<()> {
    ready()?;
    let result = (|| -> Result<()> {
        let _auth = Auth::open(&Path::new(STATE).join("control.sqlite3"))?;
        let _policy = crate::broker::Policy::read()?;
        let bundled = Path::new("/opt/zionos/catalog/zion-hello-1.0.0.zpk");
        if bundled.is_file() {
            import_catalog(bundled)?;
        }
        let catalog = Path::new(STATE).join("catalog");
        let index = catalog.join("index.json");
        if index.exists() {
            let entries: Vec<serde_json::Value> = serde_json::from_slice(
                &zpk_runtime::installed::read_file(&index, CATALOG_INDEX_LIMIT)?,
            )?;
            if entries.len() > 32 {
                return fail("local catalog limit");
            }
            for entry in entries.iter().filter(|e| e["manifest"].is_null()) {
                let id = entry["id"]
                    .as_str()
                    .filter(|id| crate::token(id))
                    .ok_or("invalid catalog id")?;
                let path = catalog.join(format!("{id}.zpk"));
                if crate::hash(&zpk_runtime::installed::read_file(&path, MAX_UPLOAD)?) != id {
                    return fail("catalog hash mismatch");
                }
                import_catalog(&path)?;
            }
        }
        let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine)?;
        for diagnostic in m.reconcile()? {
            eprintln!("ZPK recovery: {}", diagnostic.escape_debug());
        }
        if !m
            .database
            .pending(Path::new("/"))
            .is_ok_and(|p| p.is_empty())
        {
            return fail("package recovery requires local repair");
        }
        for (id, r) in m.list()? {
            let p = m.verify(&id)?;
            Alpine.register(&p)?;
            if r.current().configuration.autostart
                && p.program.is_some()
                && Alpine.status(&p)?.state != State::Running
            {
                m.lifecycle(&id, "start")?;
            }
        }
        m.publish_desktop()?;
        Ok(())
    })();
    if let Err(e) = &result {
        degraded(&e.to_string())?;
    }
    result
}
pub fn serve() -> Result<()> {
    ready()?;
    let (uid, gid) = identity()?;
    Broker::open(uid, gid)?.serve()
}
pub fn bootstrap(file: Option<PathBuf>) -> Result<()> {
    ready()?;
    let mut a = Auth::open(&Path::new(STATE).join("control.sqlite3"))?;
    if let Some(path) = file {
        let m = fs::symlink_metadata(&path)?;
        if !m.is_file() || m.uid() != 0 || m.mode() & 0o077 != 0 {
            return fail("bootstrap token file must be private and root-owned");
        };
        let code = String::from_utf8(zpk_runtime::installed::read_file(&path, 80)?)?;
        a.bootstrap(code.trim())?;
        println!("One-time setup enabled for ten minutes; use your local token in the desktop setup dialog.");
    } else {
        use std::io::Write;
        let code = crate::random()?;
        a.bootstrap(&code)?;
        let mut tty = fs::OpenOptions::new().write(true).open("/dev/tty")?;
        writeln!(
            tty,
            "One-time setup code (10 minutes, do not log/share): {code}"
        )?;
    }
    Ok(())
}
pub fn import_catalog(source: &Path) -> Result<()> {
    ready()?;
    let bytes = zpk_runtime::installed::read_file(source, MAX_UPLOAD)?;
    let p = zpk_schema::inspect(std::io::Cursor::new(&bytes), zpk_schema::Limits::default())?;
    let root = Path::new(STATE).join("catalog");
    mkdir(&root, 0o700)?;
    let digest = crate::hash(&bytes);
    let path = root.join(format!("{digest}.zpk"));
    if !path.exists() {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        use std::io::Write;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    let index = root.join("index.json");
    let mut entries: Vec<serde_json::Value> = if index.exists() {
        serde_json::from_slice(&zpk_runtime::installed::read_file(
            &index,
            CATALOG_INDEX_LIMIT,
        )?)?
    } else {
        vec![]
    };
    let entry = json!({"id":digest,"sha256":digest,"package_id":p.manifest.package.id,"name":p.manifest.package.name,"version":p.manifest.package.version,"description":p.manifest.package.description,"publisher":p.manifest.package.author,"manifest":p.manifest});
    if let Some(previous) = entries.iter_mut().find(|e| e["id"] == digest) {
        // Refresh older catalog summaries without replacing the package archive.
        *previous = entry;
    } else {
        entries.push(entry);
    }
    if entries.len() > 32 {
        return fail("local catalog limit");
    };
    let encoded = serde_json::to_vec(&entries)?;
    if encoded.len() as u64 > CATALOG_INDEX_LIMIT {
        return fail("local catalog metadata size limit");
    }
    let tmp = root.join("index.new");
    fs::write(&tmp, encoded)?;
    fs::File::open(&tmp)?.sync_all()?;
    fs::rename(tmp, index)?;
    fs::File::open(root)?.sync_all()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uuid_only_never_paths() {
        assert!(uuid("01234567-89ab-cdef-0123-456789abcdef"));
        for s in [
            "/dev/sda",
            "UUID=1234",
            "../../dev/sda",
            "01234567-89AB-cdef-0123-456789abcdef",
        ] {
            assert!(!uuid(s))
        }
    }
}
