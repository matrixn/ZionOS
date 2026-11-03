//! Linux controls, applied once immediately before exec; never a process supervisor.
use crate::{config::Plan, fail, Result};
pub const ENFORCED:&str="Dedicated UID/GID/no supplementary groups; zero capability sets/bounding/ambient; no_new_privs; bounded rlimits; required Landlock ABI>=6 filesystem/signal/abstract-UNIX scope; seccomp x86_64 denying IP sockets, outbound connect/FD passing, namespaces, tracing and privileged operations.";
pub const LIMITS:&str="Not a VM or complete syscall allowlist. No PID/mount namespace or cgroup aggregate quotas; RLIMIT_AS is virtual memory, CPU/FSIZE are per process/file; manifest storage quotas are not enforced. Root/kernel/administrator trusted. Permission declarations do not grant OS access; broker capabilities remain unavailable.";
#[cfg(target_os = "linux")]
pub fn landlock_abi() -> Result<i32> {
    let n = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0usize,
            1u32,
        )
    };
    if n < 6 {
        return fail(format!(
            "Landlock ABI >=6 required; result {n}, {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(n as i32)
}
#[cfg(not(target_os = "linux"))]
pub fn landlock_abi() -> Result<i32> {
    fail("Linux Landlock/seccomp are required")
}
#[cfg(target_os = "linux")]
fn check(value: i32) -> Result<()> {
    if value < 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}
#[cfg(target_os = "linux")]
fn limit(kind: u32, value: u64) -> Result<()> {
    check(unsafe {
        libc::setrlimit(
            kind as _,
            &libc::rlimit {
                rlim_cur: value,
                rlim_max: value,
            },
        )
    })
}
#[cfg(target_os = "linux")]
pub fn apply(plan: &Plan) -> Result<()> {
    let identity = &plan.config.identity;
    if identity.uid < 10000
        || unsafe { libc::getuid() } != identity.uid
        || unsafe { libc::geteuid() } != identity.uid
        || unsafe { libc::getegid() } != identity.gid
        || unsafe { libc::getgroups(0, std::ptr::null_mut()) } != 0
    {
        return fail("policy must only be applied after a complete dedicated-identity drop");
    }
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd, OwnedFd},
    };
    landlock_abi()?;
    let l = &plan.config.limits;
    limit(libc::RLIMIT_AS as _, l.address_space_bytes)?;
    limit(libc::RLIMIT_CPU as _, l.cpu_seconds)?;
    limit(libc::RLIMIT_NOFILE as _, l.open_files)?;
    limit(libc::RLIMIT_NPROC as _, l.processes)?;
    limit(libc::RLIMIT_FSIZE as _, l.file_bytes)?;
    limit(libc::RLIMIT_CORE as _, 0)?;
    limit(libc::RLIMIT_MEMLOCK as _, 0)?;
    #[repr(C)]
    struct Ruleset {
        fs: u64,
        net: u64,
        scoped: u64,
    }
    #[repr(C)]
    struct PathRule {
        access: u64,
        parent: i32,
    }
    let attr = Ruleset {
        fs: (1 << 16) - 1,
        net: 0,
        scoped: 3,
    };
    let fd = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &attr,
            std::mem::size_of::<Ruleset>(),
            0,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    let rule = |path: &std::path::Path, rights: u64| -> Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let name = CString::new(path.as_os_str().as_bytes())?;
        let raw = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let path_fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let attr = PathRule {
            access: rights,
            parent: path_fd.as_raw_fd(),
        };
        check(unsafe {
            libc::syscall(libc::SYS_landlock_add_rule, fd.as_raw_fd(), 1, &attr, 0) as i32
        })
    };
    const READ: u64 = (1 << 2) | (1 << 3);
    const WRITE_TREE: u64 = READ
        | (1 << 1)
        | (1 << 4)
        | (1 << 5)
        | (1 << 7)
        | (1 << 8)
        | (1 << 9)
        | (1 << 13)
        | (1 << 14);
    rule(&plan.layout.version, READ)?;
    for f in &plan.manifest.files {
        if f.kind == zpk_schema::manifest::FileKind::Executable {
            rule(&plan.layout.version.join(&f.path), 1)?;
        }
    }
    // One trusted musl loader; no host shell or general executable directory grant.
    let loader = std::fs::canonicalize("/lib/ld-musl-x86_64.so.1");
    if let Ok(loader) = loader {
        crate::installed::secure_path(std::path::Path::new("/"), &loader)?;
        rule(&loader, 1 | (1 << 2))?;
    }
    rule(&plan.layout.run, WRITE_TREE)?;
    for slot in &plan.manifest.storage {
        let (base, write) = match slot.kind {
            zpk_schema::manifest::StorageKind::Data => (
                &plan.layout.data,
                plan.config.grants.data.contains(&slot.name),
            ),
            zpk_schema::manifest::StorageKind::Config => (
                &plan.layout.configuration,
                plan.config.grants.configuration_write.contains(&slot.name),
            ),
        };
        rule(
            &base.join(&slot.name),
            if write { WRITE_TREE } else { READ },
        )?;
    }
    check(unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) })?;
    // Inherited descriptors are closed by the launcher; all future path access is restricted.
    check(unsafe { libc::syscall(libc::SYS_landlock_restrict_self, fd.as_raw_fd(), 0) as i32 })?;
    seccomp()?;
    Ok(())
}
#[cfg(not(target_os = "linux"))]
pub fn apply(_: &Plan) -> Result<()> {
    fail("Linux security controls unavailable")
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn seccomp() -> Result<()> {
    const LD: u16 = 0x20;
    const JEQ: u16 = 0x15;
    const JGE: u16 = 0x35;
    const JSET: u16 = 0x45;
    const AND: u16 = 0x54;
    const RET: u16 = 0x06;
    const DENY: u32 = 0x50000 | libc::EPERM as u32;
    const ALLOW: u32 = 0x7fff0000;
    const KILL: u32 = 0x80000000;
    fn ins(code: u16, k: u32) -> libc::sock_filter {
        libc::sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        }
    }
    fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
        libc::sock_filter { code, jt, jf, k }
    }
    let mut p = vec![
        ins(LD, 4),
        jump(JEQ, 0xc000003e, 1, 0),
        ins(RET, KILL),
        ins(LD, 0),
        jump(JGE, 0x40000000, 0, 1),
        ins(RET, KILL),
    ];
    let deny = [
        libc::SYS_shmget,
        libc::SYS_shmat,
        libc::SYS_shmdt,
        libc::SYS_shmctl,
        libc::SYS_semget,
        libc::SYS_semop,
        libc::SYS_semctl,
        libc::SYS_semtimedop,
        libc::SYS_msgget,
        libc::SYS_msgsnd,
        libc::SYS_msgrcv,
        libc::SYS_msgctl,
        libc::SYS_pidfd_getfd,
        libc::SYS_userfaultfd,
        libc::SYS_connect,
        libc::SYS_sendmsg,
        libc::SYS_sendmmsg,
        libc::SYS_recvmsg,
        libc::SYS_recvmmsg,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_keyctl,
        libc::SYS_add_key,
        libc::SYS_request_key,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_chroot,
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_mknod,
        libc::SYS_mknodat,
        libc::SYS_setuid,
        libc::SYS_setgid,
        libc::SYS_setresuid,
        libc::SYS_setresgid,
        libc::SYS_setreuid,
        libc::SYS_setregid,
        libc::SYS_setfsuid,
        libc::SYS_setfsgid,
        libc::SYS_setgroups,
        libc::SYS_capset,
        libc::SYS_open_by_handle_at,
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
        libc::SYS_open_tree,
        libc::SYS_move_mount,
        libc::SYS_fsopen,
        libc::SYS_fsconfig,
        libc::SYS_fsmount,
        libc::SYS_fspick,
        libc::SYS_mount_setattr,
    ];
    for nr in deny {
        p.push(jump(JEQ, nr as u32, 0, 1));
        p.push(ins(RET, DENY));
    }
    // Force libc to use clone's inspectable flags, rather than opaque clone3 pointers.
    p.extend([
        jump(JEQ, libc::SYS_clone3 as u32, 0, 1),
        ins(RET, 0x50000 | libc::ENOSYS as u32),
    ]);
    p.extend([
        jump(JEQ, libc::SYS_clone as u32, 0, 4),
        ins(LD, 16),
        jump(JSET, 0x7e020000, 0, 1),
        ins(RET, DENY),
        ins(RET, ALLOW),
    ]);
    // Only connected sendto(NULL) is needed for UnixStream; no datagram addressing bypass.
    p.extend([
        jump(JEQ, libc::SYS_sendto as u32, 0, 6),
        ins(LD, 48),
        jump(JEQ, 0, 0, 3),
        ins(LD, 52),
        jump(JEQ, 0, 0, 1),
        ins(RET, ALLOW),
        ins(RET, DENY),
    ]);
    for nr in [libc::SYS_socket, libc::SYS_socketpair] {
        p.extend([
            jump(JEQ, nr as u32, 0, 7),
            ins(LD, 16),
            jump(JEQ, libc::AF_UNIX as u32, 0, 4),
            ins(LD, 24),
            ins(AND, 0xf),
            jump(JEQ, libc::SOCK_STREAM as u32, 0, 1),
            ins(RET, ALLOW),
            ins(RET, DENY),
        ]);
    }
    p.push(ins(RET, ALLOW));
    let program = libc::sock_fprog {
        len: p.len() as u16,
        filter: p.as_mut_ptr(),
    };
    check(unsafe { libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) })
}
#[cfg(all(target_os = "linux", not(target_arch = "x86_64")))]
fn seccomp() -> Result<()> {
    fail("only the tested x86_64 seccomp profile is supported")
}
