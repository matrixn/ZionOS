use crate::{
    config::{Plan, RuntimeConfig, StorageMode},
    fail, installed, Result,
};
use std::{fs, path::Path};
pub fn system_plan(config_path: &Path) -> Result<Plan> {
    let c = control_config(config_path)?;
    let p = installed::load(Path::new("/"), &c)?;
    validate_identity(&p)?;
    Ok(p)
}
pub fn control_config(config_path: &Path) -> Result<RuntimeConfig> {
    if unsafe { libc::geteuid() } != 0 {
        return fail(
            "runtime administration requires root; application execution does not retain root",
        );
    }
    installed::secure_path(Path::new("/"), config_path)?;
    let c = RuntimeConfig::parse(std::str::from_utf8(&installed::read_file(
        config_path,
        64 * 1024,
    )?)?)?;
    let layout = crate::config::Layout::new(Path::new("/"), &c)?;
    if c.runtime_version != 1
        || c.identity.name != crate::config::identity_name(&c.package_id)
        || config_path != layout.config
    {
        return fail("runtime config must use its canonical trusted per-package path");
    }
    Ok(c)
}
pub fn save_config(path: &Path, c: &RuntimeConfig) -> Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(toml::to_string_pretty(c)?.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
#[cfg(target_os = "linux")]
pub fn validate_identity(plan: &Plan) -> Result<()> {
    use std::ffi::{CStr, CString};
    let id = &plan.config.identity;
    let name = CString::new(id.name.clone())?;
    unsafe {
        let p = libc::getpwnam(name.as_ptr());
        let g = libc::getgrnam(name.as_ptr());
        if p.is_null()
            || g.is_null()
            || (*p).pw_uid != id.uid
            || (*p).pw_gid != id.gid
            || (*g).gr_gid != id.gid
        {
            return fail(
                "dedicated Unix user/group is missing or does not match configured UID/GID",
            );
        }
        let shell = CStr::from_ptr((*p).pw_shell).to_str()?;
        let shadow = libc::getspnam(name.as_ptr());
        if shadow.is_null()
            || !matches!(
                CStr::from_ptr((*shadow).sp_pwdp).to_bytes().first(),
                Some(b'!' | b'*')
            )
        {
            return fail("service account password must be locked");
        }
        if !["/sbin/nologin", "/usr/sbin/nologin"].contains(&shell) {
            return fail("service account must have a nologin shell");
        }
        libc::setpwent();
        let mut count = 0;
        let mut shared_primary_group = false;
        loop {
            let entry = libc::getpwent();
            if entry.is_null() {
                break;
            }
            if (*entry).pw_uid == id.uid {
                count += 1;
            } else if (*entry).pw_gid == id.gid {
                shared_primary_group = true;
            }
        }
        libc::endpwent();
        libc::setgrent();
        let mut group_count = 0;
        let mut external_members = false;
        loop {
            let entry = libc::getgrent();
            if entry.is_null() {
                break;
            }
            if (*entry).gr_gid == id.gid {
                group_count += 1;
                let mut members = (*entry).gr_mem;
                if !members.is_null() {
                    while !(*members).is_null() {
                        external_members |=
                            CStr::from_ptr(*members).to_bytes() != id.name.as_bytes();
                        members = members.add(1);
                    }
                }
            }
        }
        libc::endgrent();
        if count != 1 || group_count != 1 || external_members || shared_primary_group {
            return fail("UID/GID must be exclusive to this package; no foreign primary/supplementary group members");
        }
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
pub fn validate_identity(_: &Plan) -> Result<()> {
    fail("Linux identity controls required")
}
#[cfg(target_os = "linux")]
fn trusted_directories(path: &Path) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mut current = std::path::PathBuf::from("/");
    for component in path.strip_prefix("/")?.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return fail("noncanonical runtime directory");
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(m) => {
                if !m.is_dir()
                    || m.file_type().is_symlink()
                    || m.uid() != 0
                    || m.mode() & 0o022 != 0
                {
                    return fail(
                        "runtime ancestors must be trusted root-owned directories, not symlinks",
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
                fs::set_permissions(&current, fs::Permissions::from_mode(0o755))?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn directory(path: &Path, uid: u32, gid: u32, mode: u32) -> Result<()> {
    use std::{
        ffi::CString,
        os::unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
        },
    };
    // The final directory may already belong to the app, but ancestors must not be writable by it.
    if let Some(parent) = path.parent() {
        trusted_directories(parent)?;
    }
    match fs::symlink_metadata(path) {
        Ok(m) => {
            if !m.is_dir() || m.file_type().is_symlink() {
                return fail("runtime directory symlink/type mismatch");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(path)?,
        Err(e) => return Err(e.into()),
    }
    let c = CString::new(path.as_os_str().as_bytes())?;
    if unsafe { libc::chown(c.as_ptr(), uid, gid) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    let m = fs::symlink_metadata(path)?;
    if m.uid() != uid || m.gid() != gid {
        return fail("runtime ownership setup failed");
    }
    Ok(())
}
pub fn prepare(p: &Plan) -> Result<()> {
    if p.layout.root != Path::new("/") || unsafe { libc::geteuid() } != 0 {
        return fail("real preparation is limited to an administrator in the target OS; use temporary roots/mocks for unit tests");
    }
    validate_identity(p)?;
    crate::policy::landlock_abi()?;
    #[cfg(target_os = "linux")]
    {
        for base in [
            &p.layout.data,
            &p.layout.configuration,
            &p.layout.logs,
            &p.layout.control,
        ] {
            // Create only missing trusted ancestors; refuse symlink ancestors before ownership changes.
            let parent = base.parent().ok_or("missing directory parent")?;
            trusted_directories(parent)?;
            installed::secure_path(Path::new("/"), parent)?;
            directory(base, 0, 0, 0o755)?;
        }
        directory(&p.layout.run, 0, 0, 0o755)?;
        for slot in &p.manifest.storage {
            let base = match slot.kind {
                zpk_schema::manifest::StorageKind::Data => &p.layout.data,
                zpk_schema::manifest::StorageKind::Config => &p.layout.configuration,
            };
            directory(
                &base.join(&slot.name),
                p.config.identity.uid,
                p.config.identity.gid,
                0o700,
            )?;
        }
        directory(
            &p.layout.run,
            p.config.identity.uid,
            p.config.identity.gid,
            0o700,
        )?;
        directory(&p.layout.logs, 0, 0, 0o700)?;
        directory(&p.layout.control, 0, 0, 0o700)?;
    }
    verify_persistence(p)?;
    let binding = p.layout.control.join(format!("{}.owner", p.layout.service));
    if binding.try_exists()? {
        if installed::read_file(&binding, 512)? != p.config.package_id.as_bytes() {
            return fail("runtime identity/service key already belongs to another package");
        }
    } else {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&binding)?
            .write_all(p.config.package_id.as_bytes())?;
    }
    if p.program.is_some() {
        let path = Path::new("/etc/init.d").join(&p.layout.service);
        installed::secure_path(Path::new("/"), Path::new("/etc/init.d"))?;
        if path.try_exists()? {
            installed::secure_path(Path::new("/"), &path)?;
            if !std::str::from_utf8(&installed::read_file(&path, 65536)?)?
                .contains("# Trusted ZPK Runtime adapter.")
            {
                return fail("refusing to overwrite a foreign OpenRC definition");
            }
        }
        fs::write(
            Path::new("/etc/init.d").join(&p.layout.service),
            crate::backend::render(p)?,
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                Path::new("/etc/init.d").join(&p.layout.service),
                fs::Permissions::from_mode(0o755),
            )?;
        }
    }
    Ok(())
}
pub fn verify_persistence(p: &Plan) -> Result<()> {
    if p.config.storage_mode == StorageMode::EphemeralDevelopment {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        for path in [
            &p.layout.version,
            &p.layout.data,
            &p.layout.configuration,
            &p.layout.logs,
        ] {
            let c = CString::new(path.as_os_str().as_bytes())?;
            let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
            if unsafe { libc::statfs(c.as_ptr(), &mut stat) } < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if ![0xef53i64, 0x58465342, 0x9123683e].contains(&(stat.f_type as i64)) {
                return fail("persistent-required needs explicitly provisioned ext4/XFS/Btrfs paths; live tmpfs/overlay is not persistence");
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    fail("Linux persistent filesystem checks unavailable")
}
#[cfg(target_os = "linux")]
pub fn execute(p: &Plan) -> Result<()> {
    use std::{
        ffi::CString,
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    };
    verify_persistence(p)?;
    validate_identity(p)?;
    crate::policy::landlock_abi()?;
    let program = p.program.as_ref().ok_or("static desktop has no process")?;
    let exe = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&program.executable)?;
    // No app-supplied path controls the privileged log opens.
    for (target, name) in [(1, "stdout.log"), (2, "stderr.log")] {
        let file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(p.layout.logs.join(name))?;
        {
            use std::os::unix::fs::MetadataExt;
            let m = file.metadata()?;
            if m.uid() != 0 || m.nlink() != 1 || m.mode() & 0o777 != 0o600 {
                return fail("logs must be private root-owned regular files with one link");
            }
        }
        if unsafe { libc::dup2(file.as_raw_fd(), target) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    let input = fs::File::open("/dev/null")?;
    unsafe {
        libc::dup2(input.as_raw_fd(), 0);
        libc::umask(0o077);
    }
    // Drop bounding/ambient capabilities while privileged, then all user/group identity and sets.
    let last = fs::read_to_string("/proc/sys/kernel/cap_last_cap")?
        .trim()
        .parse::<u32>()?;
    for capability in 0..=last {
        if unsafe { libc::prctl(libc::PR_CAPBSET_DROP, capability, 0, 0, 0) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    if unsafe {
        libc::prctl(
            libc::PR_CAP_AMBIENT,
            libc::PR_CAP_AMBIENT_CLEAR_ALL,
            0,
            0,
            0,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let id = &p.config.identity;
    if unsafe { libc::setgroups(0, std::ptr::null()) } < 0
        || unsafe { libc::setresgid(id.gid, id.gid, id.gid) } < 0
        || unsafe { libc::setresuid(id.uid, id.uid, id.uid) } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    let header = Header {
        version: 0x20080522,
        pid: 0,
    };
    let data = [Data {
        effective: 0,
        permitted: 0,
        inheritable: 0,
    }; 2];
    if unsafe { libc::syscall(libc::SYS_capset, &header, data.as_ptr()) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut uid = [0u32; 3];
    let mut gid = [0u32; 3];
    unsafe {
        libc::getresuid(&mut uid[0], &mut uid[1], &mut uid[2]);
        libc::getresgid(&mut gid[0], &mut gid[1], &mut gid[2]);
    }
    if uid != [id.uid; 3] || gid != [id.gid; 3] {
        return fail("identity drop failed");
    }
    std::env::set_current_dir(&p.layout.version)?;
    crate::policy::apply(p)?;
    let args: Vec<_> = std::iter::once(
        program
            .executable
            .to_str()
            .ok_or("program path encoding")?
            .to_owned(),
    )
    .chain(program.arguments.clone())
    .map(CString::new)
    .collect::<std::result::Result<_, _>>()?;
    let env: Vec<_> = [
        format!("HOME={}", p.layout.data.display()),
        format!("TMPDIR={}", p.layout.run.display()),
        format!("ZPK_DATA_DIR={}", p.layout.data.display()),
        format!("ZPK_CONFIG_DIR={}", p.layout.configuration.display()),
        format!("ZPK_RUNTIME_DIR={}", p.layout.run.display()),
        "LANG=C".into(),
        "PATH=/nonexistent".into(),
    ]
    .into_iter()
    .map(CString::new)
    .collect::<std::result::Result<_, _>>()?;
    let mut argv: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
    argv.push(std::ptr::null());
    let mut envp: Vec<_> = env.iter().map(|s| s.as_ptr()).collect();
    envp.push(std::ptr::null());
    let keep = exe.as_raw_fd();
    // Mark every non-stdio descriptor CLOEXEC, including numbers above the new RLIMIT_NOFILE.
    if unsafe { libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let result = unsafe { libc::fexecve(keep, argv.as_ptr(), envp.as_ptr()) };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
pub fn execute(_: &Plan) -> Result<()> {
    fail("Linux runtime required")
}
