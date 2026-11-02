use crate::{
    config::{Activation, Plan},
    fail, installed, Result,
};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    NotRequired,
    Running,
    Starting,
    Stopping,
    Stopped,
    Inactive,
    Crashed,
    Unknown,
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub state: State,
    pub pid: Option<u32>,
    pub restart_count: Option<u32>,
    pub backend_code: i32,
}
#[derive(Debug)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
}
pub trait Operations {
    fn run(&self, tool: &str, args: &[&str]) -> Result<Output>;
    fn observes_child(&self, pid: u32, plan: &Plan) -> Result<bool>;
}
pub struct NativeOpenRc;
impl Operations for NativeOpenRc {
    fn observes_child(&self, pid: u32, plan: &Plan) -> Result<bool> {
        let status = match fs::read_to_string(format!("/proc/{pid}/status")) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };
        let uid = status
            .lines()
            .find(|l| l.starts_with("Uid:"))
            .and_then(|l| l.split_whitespace().nth(2))
            .and_then(|v| v.parse::<u32>().ok());
        let exe = fs::read_link(format!("/proc/{pid}/exe")).ok();
        Ok(uid == Some(plan.config.identity.uid)
            && exe.as_ref() == plan.program.as_ref().map(|p| &p.executable))
    }
    fn run(&self, tool: &str, args: &[&str]) -> Result<Output> {
        if unsafe { libc::geteuid() } != 0 || !Path::new("/sbin/openrc-run").is_file() {
            return fail("real OpenRC operations require a root administrator on Alpine; use mocks for test roots");
        }
        if !["/sbin/rc-service", "/sbin/rc-update"].contains(&tool) {
            return fail("unsupported init command");
        }
        let mut child = Command::new(tool)
            .args(args)
            .env("RC_COLOR", "NO")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().ok_or("missing stdout")?;
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            stdout.take(65537).read_to_string(&mut text).map(|_| text)
        });
        let deadline = Instant::now() + Duration::from_secs(40);
        let result = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return fail("OpenRC operation timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let text = reader.join().map_err(|_| "init stdout reader failed")??;
        if text.len() > 65536 {
            return fail("OpenRC output limit");
        }
        Ok(Output {
            code: result.code().unwrap_or(-1),
            stdout: text,
        })
    }
}
pub trait ServiceBackend {
    fn start(&self, plan: &Plan) -> Result<()>;
    fn stop(&self, plan: &Plan) -> Result<()>;
    fn restart(&self, plan: &Plan) -> Result<()>;
    fn status(&self, plan: &Plan) -> Result<Status>;
    fn autostart(&self, plan: &Plan, enabled: bool) -> Result<()>;
}
pub struct OpenRc<O: Operations> {
    pub operations: O,
}
impl<O: Operations> OpenRc<O> {
    fn command(&self, p: &Plan, verb: &str) -> Result<()> {
        if p.program.is_none() {
            return Ok(());
        }
        let out = self
            .operations
            .run("/sbin/rc-service", &[&p.layout.service, verb])?;
        if out.code != 0 {
            return fail(format!("OpenRC {verb} failed with code {}", out.code));
        }
        Ok(())
    }
}
impl<O: Operations> ServiceBackend for OpenRc<O> {
    fn start(&self, p: &Plan) -> Result<()> {
        self.command(p, "start")
    }
    fn stop(&self, p: &Plan) -> Result<()> {
        self.command(p, "stop")
    }
    fn restart(&self, p: &Plan) -> Result<()> {
        self.command(p, "restart")
    }
    fn status(&self, p: &Plan) -> Result<Status> {
        if p.program.is_none() {
            return Ok(Status {
                state: State::NotRequired,
                pid: None,
                restart_count: None,
                backend_code: 0,
            });
        }
        let out = self
            .operations
            .run("/sbin/rc-service", &[&p.layout.service, "status"])?;
        let mut state = match out.code {
            0 => State::Running,
            3 => State::Stopped,
            4 => State::Stopping,
            8 => State::Starting,
            16 => State::Inactive,
            32 | 64 => State::Crashed,
            _ => State::Unknown,
        };
        let details = self
            .operations
            .run("/sbin/rc-service", &[&p.layout.service, "runtime_state"])?;
        let mut lines = details.stdout.lines();
        let pid = lines
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|p| *p > 1);
        let restart_count = lines.next().and_then(|v| v.parse().ok());
        if lines.next() == Some("1") {
            state = State::Crashed;
        }
        if state == State::Running
            && !match pid {
                Some(pid) => self.operations.observes_child(pid, p)?,
                None => false,
            }
        {
            state = State::Starting;
        }
        Ok(Status {
            state,
            pid,
            restart_count,
            backend_code: out.code,
        })
    }
    fn autostart(&self, p: &Plan, enabled: bool) -> Result<()> {
        if enabled && (p.config.activation != Activation::Background || p.program.is_none()) {
            return fail("only native background services may autostart");
        }
        if p.program.is_none() {
            return Ok(());
        }
        let result = self.operations.run(
            "/sbin/rc-update",
            &[
                if enabled { "add" } else { "del" },
                &p.layout.service,
                "default",
            ],
        )?;
        if result.code != 0 {
            return fail("OpenRC autostart update failed");
        }
        Ok(())
    }
}
pub fn render(plan: &Plan) -> Result<String> {
    if plan.layout.root != Path::new("/") {
        return fail("OpenRC script generation is only for canonical system paths; test roots use mock operations");
    }
    if plan.program.is_none() {
        return fail("static desktop does not need an OpenRC service");
    }
    let c = &plan.config;
    let l = &plan.layout;
    let mut script = include_str!("../openrc/service.in").to_owned();
    for (key, value) in [
        ("@ID@", c.package_id.clone()),
        ("@CONFIG@", l.config.display().to_string()),
        ("@CONTROL@", l.control.display().to_string()),
        ("@SERVICE@", l.service.clone()),
        ("@DELAY@", c.restart.delay_seconds.to_string()),
        ("@MAX@", c.restart.max.to_string()),
        ("@PERIOD@", c.restart.period_seconds.to_string()),
        ("@STOP@", c.restart.stop_seconds.to_string()),
    ] {
        script = script.replace(key, &value);
    }
    Ok(script)
}
pub fn logs(p: &Plan, stderr: bool, max: usize) -> Result<String> {
    if max == 0 || max > 1024 * 1024 {
        return fail("logs limit is 1..1 MiB");
    }
    let path = p
        .layout
        .logs
        .join(if stderr { "stderr.log" } else { "stdout.log" });
    installed::secure_path(&p.layout.root, &path)?;
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(max as u64)))?;
    let mut data = vec![];
    file.take(max as u64).read_to_end(&mut data)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}
#[derive(Debug, Serialize)]
pub struct Health {
    pub healthy: bool,
    pub detail: String,
}
#[cfg(target_os = "linux")]
pub fn health(p: &Plan, status: &Status) -> Result<Health> {
    use std::os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixStream},
    };
    if status.state == State::NotRequired {
        return Ok(Health {
            healthy: true,
            detail: "no native process required".into(),
        });
    }
    if status.state != State::Running {
        return Ok(Health {
            healthy: false,
            detail: "backend is not running".into(),
        });
    }
    if p.config.health == crate::config::HealthMode::Process {
        return Ok(Health {
            healthy: true,
            detail: "verified primary process liveness only, not functional readiness".into(),
        });
    }
    let path = p.layout.run.join("service.sock");
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() > 107 {
        return fail("Unix health socket path too long");
    }
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as _;
    for (i, b) in bytes.iter().enumerate() {
        address.sun_path[i] = *b as _;
    }
    let connect = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of::<libc::sockaddr_un>() as _,
        )
    };
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLOUT,
        revents: 0,
    };
    if connect < 0
        && (std::io::Error::last_os_error().raw_os_error() != Some(libc::EINPROGRESS)
            || unsafe { libc::poll(&mut poll, 1, 1000) } <= 0)
    {
        return Ok(Health {
            healthy: false,
            detail: "Unix socket unavailable".into(),
        });
    }
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut size,
        )
    } < 0
        || credentials.uid != p.config.identity.uid
        || credentials.gid != p.config.identity.gid
    {
        return Ok(Health {
            healthy: false,
            detail: "Unix peer identity mismatch".into(),
        });
    }
    let mut stream = UnixStream::from(fd);
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let result = (|| -> std::io::Result<bool> {
        stream.write_all(b"ZPK_HEALTH_V1\n")?;
        let mut answer = [0u8; 4];
        stream.read_exact(&mut answer)?;
        Ok(&answer == b"OK\n\n")
    })();
    Ok(Health {
        healthy: result.unwrap_or(false),
        detail: "identity-checked Unix stream health protocol".into(),
    })
}
#[cfg(not(target_os = "linux"))]
pub fn health(_: &Plan, _: &Status) -> Result<Health> {
    fail("Linux Unix peer credentials required")
}
