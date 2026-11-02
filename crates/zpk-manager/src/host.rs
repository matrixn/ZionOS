//! Constrained privileged adapter. No package shell/hooks and no generic command API.
use crate::{fail, Result};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};
use zpk_runtime::{
    backend::{self, NativeOpenRc, OpenRc, ServiceBackend, Status},
    config::{identity_name, Identity, Plan},
    installed, launch,
};
pub trait Host: ServiceBackend {
    fn identity(&self, id: &str, reserved: &std::collections::BTreeSet<u32>) -> Result<Identity>;
    fn register(&self, p: &Plan) -> Result<()>;
    fn unregister(&self, p: &Plan) -> Result<()>;
    fn healthy(&self, p: &Plan) -> Result<bool>;
    fn logs(&self, p: &Plan, stderr: bool) -> Result<String>;
}
pub struct Alpine;
fn backend() -> OpenRc<NativeOpenRc> {
    OpenRc {
        operations: NativeOpenRc,
    }
}
impl ServiceBackend for Alpine {
    fn start(&self, p: &Plan) -> Result<()> {
        backend().start(p)
    }
    fn stop(&self, p: &Plan) -> Result<()> {
        target(p)?;
        if !Path::new("/etc/init.d")
            .join(&p.layout.service)
            .try_exists()?
        {
            if p.layout
                .control
                .join(format!("{}.pid", p.layout.service))
                .try_exists()?
            {
                return fail("service definition missing with a possible live supervisor; manual recovery required");
            }
            return Ok(());
        }
        backend().stop(p)
    }
    fn restart(&self, p: &Plan) -> Result<()> {
        backend().restart(p)
    }
    fn status(&self, p: &Plan) -> Result<Status> {
        target(p)?;
        if p.program.is_some()
            && !Path::new("/etc/init.d")
                .join(&p.layout.service)
                .try_exists()?
            && !p
                .layout
                .control
                .join(format!("{}.pid", p.layout.service))
                .try_exists()?
        {
            return Ok(Status {
                state: backend::State::Stopped,
                pid: None,
                restart_count: None,
                backend_code: 3,
            });
        }
        backend().status(p)
    }
    fn autostart(&self, p: &Plan, on: bool) -> Result<()> {
        let link = Path::new("/etc/runlevels/default").join(&p.layout.service);
        if !on && !link.try_exists()? {
            return Ok(());
        }
        backend().autostart(p, on)
    }
}
fn target(p: &Plan) -> Result<()> {
    if p.layout.root != Path::new("/")
        || unsafe { libc::geteuid() } != 0
        || !Path::new("/sbin/openrc-run").is_file()
    {
        return fail("native manager host actions require target Alpine root; temporary roots must use mocks");
    }
    Ok(())
}
fn trusted_command(tool: &str, args: &[&str]) -> Result<()> {
    let result = Command::new(tool)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !result.success() {
        return fail(format!("dedicated identity helper failed: {tool}"));
    }
    Ok(())
}
impl Host for Alpine {
    fn identity(&self, id: &str, reserved: &std::collections::BTreeSet<u32>) -> Result<Identity> {
        use std::ffi::CString;
        let name = identity_name(id);
        let c = CString::new(name.clone())?;
        unsafe {
            let p = libc::getpwnam(c.as_ptr());
            let g = libc::getgrnam(c.as_ptr());
            if !p.is_null() && !g.is_null() {
                return Ok(Identity {
                    name,
                    uid: (*p).pw_uid,
                    gid: (*g).gr_gid,
                });
            }
            if !p.is_null() || !g.is_null() {
                return fail("partial/colliding service identity; administrator repair required");
            }
            for n in 10000..=60000 {
                if !reserved.contains(&n)
                    && libc::getpwuid(n).is_null()
                    && libc::getgrgid(n).is_null()
                {
                    return Ok(Identity {
                        name,
                        uid: n,
                        gid: n,
                    });
                }
            }
        }
        fail("no free dedicated UID/GID in supported range")
    }
    fn register(&self, p: &Plan) -> Result<()> {
        target(p)?;
        let id = &p.config.identity;
        let c = std::ffi::CString::new(id.name.clone())?;
        unsafe {
            if libc::getpwnam(c.as_ptr()).is_null() {
                // Narrow fixed executables and validated numeric/hash arguments; no shell.
                if !libc::getgrnam(c.as_ptr()).is_null()
                    || !libc::getpwuid(id.uid).is_null()
                    || !libc::getgrgid(id.gid).is_null()
                {
                    return fail("reserved UID/GID/name collision");
                }
                trusted_command("/usr/sbin/addgroup", &["-g", &id.gid.to_string(), &id.name])?;
                trusted_command(
                    "/usr/sbin/adduser",
                    &[
                        "-D",
                        "-H",
                        "-u",
                        &id.uid.to_string(),
                        "-G",
                        &id.name,
                        "-s",
                        "/sbin/nologin",
                        &id.name,
                    ],
                )?;
            }
        }
        launch::validate_identity(p)?;
        launch::prepare(p)?;
        if p.program.is_some() {
            let init = Path::new("/etc/init.d").join(&p.layout.service);
            crate::filesystem::sync(&init)?;
            crate::filesystem::sync(Path::new("/etc/init.d"))?;
            self.autostart(p, p.config.autostart)?;
            crate::filesystem::sync(Path::new("/etc/runlevels/default"))?;
        }
        Ok(())
    }
    fn unregister(&self, p: &Plan) -> Result<()> {
        target(p)?;
        if p.program.is_none() {
            return Ok(());
        }
        let init = Path::new("/etc/init.d").join(&p.layout.service);
        if init.try_exists()? {
            installed::secure_path(Path::new("/"), &init)?;
            if installed::read_file(&init, 65536)? != backend::render(p)?.as_bytes() {
                return fail("refusing to remove a foreign/modified service registration");
            }
            self.autostart(p, false)?;
            fs::remove_file(init)?;
            crate::filesystem::sync(Path::new("/etc/init.d"))?;
        }
        // Locked identities are intentionally retained: persistent data must never inherit a reused UID.
        Ok(())
    }
    fn healthy(&self, p: &Plan) -> Result<bool> {
        Ok(backend::health(p, &self.status(p)?)?.healthy)
    }
    fn logs(&self, p: &Plan, stderr: bool) -> Result<String> {
        backend::logs(p, stderr, 65536)
    }
}
