//! Owned native fixture. Run only through the runtime in a disposable test VM.
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    if uid < 10000
        || unsafe { libc::geteuid() } != uid
        || unsafe { libc::getgroups(0, std::ptr::null_mut()) } != 0
    {
        return Err("fixture must run as dedicated non-root without supplementary groups".into());
    }
    let data = PathBuf::from(std::env::var("ZPK_DATA_DIR")?).join("state");
    let config = PathBuf::from(std::env::var("ZPK_CONFIG_DIR")?).join("settings");
    let run = PathBuf::from(std::env::var("ZPK_RUNTIME_DIR")?);
    let count = data.join("starts");
    let starts = fs::read_to_string(&count)
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        + 1;
    fs::write(&count, starts.to_string())?;
    fs::write(config.join("example.conf"), "fixture configuration\n")?;
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Caps {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    let h = Header {
        version: 0x20080522,
        pid: 0,
    };
    let mut caps = [Caps::default(); 2];
    let result = unsafe { libc::syscall(libc::SYS_capget, &h, caps.as_mut_ptr()) };
    let caps_zero = result == 0
        && caps
            .iter()
            .all(|c| c.effective == 0 && c.permitted == 0 && c.inheritable == 0)
        && (0..=40).all(|c| unsafe { libc::prctl(libc::PR_CAPBSET_READ, c, 0, 0, 0) } == 0);
    let fd_leak = (3..256).any(|fd| unsafe { libc::fcntl(fd, libc::F_GETFD) } >= 0);
    let no_new_privs = unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) } == 1;
    let seccomp = unsafe { libc::prctl(libc::PR_GET_SECCOMP, 0, 0, 0, 0) } == 2;
    let nas_read_denied = fs::read("/var/lib/zionos/nas-fixture/public.txt").is_err();
    let nas_write_denied = fs::write("/var/lib/zionos/nas-fixture/public.txt", b"denied").is_err();
    let passwd_denied = fs::read("/etc/passwd").is_err();
    let device_denied = fs::File::open("/dev/null").is_err();
    let shell_denied = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("exit 0")
        .status()
        .is_err();
    let ip_denied = std::net::TcpListener::bind("127.0.0.1:0").is_err();
    let outbound_unix_denied = UnixStream::connect("/run/nonexistent-zpk.sock")
        .err()
        .and_then(|e| e.raw_os_error())
        == Some(libc::EPERM);
    let mut allocation = Vec::<u8>::new();
    let memory_denied = allocation.try_reserve(512 * 1024 * 1024).is_err();
    let mut open = vec![];
    while let Ok(file) = fs::File::open(
        std::env::current_exe()?
            .parent()
            .ok_or("parent")?
            .join("../../manifest.toml"),
    ) {
        open.push(file);
        if open.len() > 128 {
            break;
        }
    }
    let nofile_limited = open.len() < 128;
    drop(open);
    unsafe {
        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
    }
    let big = data.join("oversize");
    let fsize_denied = fs::write(&big, vec![0u8; 2 * 1024 * 1024]).is_err();
    let _ = fs::remove_file(big);
    let mut children = vec![];
    let mut nproc_denied = false;
    for _ in 0..16 {
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            unsafe {
                libc::sleep(10);
                libc::_exit(0);
            }
        }
        if pid < 0 {
            nproc_denied = true;
            break;
        }
        children.push(pid);
    }
    for pid in children {
        unsafe {
            libc::kill(pid, libc::SIGTERM);
            libc::waitpid(pid, std::ptr::null_mut(), 0);
        }
    }
    unsafe {
        libc::signal(libc::SIGTERM, stop as *const () as usize);
        libc::signal(libc::SIGINT, stop as *const () as usize);
        libc::signal(libc::SIGXFSZ, libc::SIG_DFL);
    }
    let sysvipc_denied = unsafe { libc::shmget(libc::IPC_PRIVATE, 4096, libc::IPC_CREAT | 0o600) }
        < 0
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    let namespace_denied = unsafe { libc::unshare(libc::CLONE_NEWUSER) } < 0
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    let report = serde_json::json!({"uid":uid,"gid":gid,"starts":starts,"caps_zero":caps_zero,"no_new_privs":no_new_privs,"seccomp":seccomp,"fd_leak":fd_leak,"nas_read_denied":nas_read_denied,"nas_write_denied":nas_write_denied,"passwd_denied":passwd_denied,"device_denied":device_denied,"shell_denied":shell_denied,"ip_denied":ip_denied,"outbound_unix_denied":outbound_unix_denied,"memory_denied":memory_denied,"nofile_limited":nofile_limited,"fsize_denied":fsize_denied,"nproc_denied":nproc_denied,"sysvipc_denied":sysvipc_denied,"namespace_denied":namespace_denied});
    let socket = run.join("service.sock");
    let _ = fs::remove_file(&socket);
    let listener = UnixListener::bind(socket)?;
    listener.set_nonblocking(true)?;
    println!("runtime fixture ready: {report}");
    while !STOP.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut request = vec![];
                let mut byte = [0];
                while request.len() < 64 && stream.read(&mut byte)? == 1 {
                    request.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                match request.as_slice() {
                    b"ZPK_HEALTH_V1\n" => stream.write_all(b"OK\n\n")?,
                    b"REPORT\n" => {
                        stream.write_all(report.to_string().as_bytes())?;
                        stream.write_all(b"\n")?;
                    }
                    b"CRASH\n" => {
                        println!("requested fixture crash");
                        std::process::exit(55);
                    }
                    b"CPU\n" => loop {
                        std::hint::black_box(123u64.wrapping_mul(456));
                    },
                    _ => {}
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(e) => return Err(e.into()),
        }
    }
    fs::write(data.join("clean-shutdown"), "yes")?;
    println!("clean shutdown");
    Ok(())
}
