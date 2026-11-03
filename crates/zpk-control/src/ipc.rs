//! Framed Unix IPC. Peer credentials are checked at BOTH ends; one optional SCM_RIGHTS fd.
use crate::{
    fail,
    protocol::{Reply, Request, FRAME, RESPONSE, SOCKET},
    Result,
};
use std::{
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::net::UnixStream,
    },
    path::PathBuf,
    time::Duration,
};
pub fn peer(stream: &UnixStream) -> Result<libc::ucred> {
    let mut c: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of_val(&c) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut c as *mut libc::ucred).cast(),
            &mut len,
        )
    } < 0
        || len as usize != std::mem::size_of_val(&c)
    {
        return fail("IPC peer credentials unavailable");
    }
    Ok(c)
}
// musl uses u32 ancillary lengths; glibc uses usize. Never assume their ABI matches.
fn ancillary_length<T: TryFrom<usize>>(value: usize) -> Result<T> {
    T::try_from(value).map_err(|_| "ancillary length overflow".into())
}
pub fn send(stream: &mut UnixStream, bytes: &[u8], fd: Option<RawFd>) -> Result<()> {
    let mut byte = b'Z';
    let mut iov = libc::iovec {
        iov_base: (&mut byte as *mut u8).cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 8];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    if let Some(fd) = fd {
        unsafe {
            msg.msg_control = control.as_mut_ptr().cast();
            msg.msg_controllen =
                ancillary_length(libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize)?;
            let c = libc::CMSG_FIRSTHDR(&msg);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len =
                ancillary_length(libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as usize)?;
            std::ptr::write_unaligned(libc::CMSG_DATA(c).cast::<RawFd>(), fd);
        }
    }
    if unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) } != 1 {
        return Err(std::io::Error::last_os_error().into());
    }
    stream.write_all(&1u16.to_be_bytes())?;
    stream.write_all(&(u32::try_from(bytes.len())?).to_be_bytes())?;
    stream.write_all(bytes)?;
    Ok(())
}
pub fn receive(stream: &mut UnixStream, limit: usize) -> Result<(Vec<u8>, Option<OwnedFd>)> {
    let mut byte = 0u8;
    let mut iov = libc::iovec {
        iov_base: (&mut byte as *mut u8).cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 8];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = ancillary_length(std::mem::size_of_val(&control))?;
    let n = unsafe { libc::recvmsg(stream.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC) };
    if n != 1 {
        return fail("incomplete IPC frame");
    }
    let mut fds = vec![];
    unsafe {
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let count = (((*c).cmsg_len as u64).saturating_sub(libc::CMSG_LEN(0) as u64)
                    / std::mem::size_of::<RawFd>() as u64) as usize;
                for i in 0..count {
                    fds.push(OwnedFd::from_raw_fd(std::ptr::read_unaligned(
                        libc::CMSG_DATA(c).cast::<RawFd>().add(i),
                    )))
                }
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
    }
    if byte != b'Z' || msg.msg_flags & libc::MSG_CTRUNC != 0 || fds.len() > 1 {
        return fail("invalid IPC descriptors or frame");
    }
    let mut version = [0; 2];
    stream.read_exact(&mut version)?;
    if u16::from_be_bytes(version) != 1 {
        return fail("incompatible broker protocol version");
    }
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > limit {
        return fail("IPC frame exceeds limit");
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok((bytes, fds.pop()))
}
#[derive(Clone)]
pub struct Client {
    path: PathBuf,
    server_uid: u32,
}
impl Default for Client {
    fn default() -> Self {
        Self {
            path: SOCKET.into(),
            server_uid: 0,
        }
    }
}
impl Client {
    pub fn call(&self, request: &Request, fd: Option<RawFd>) -> Result<Reply> {
        let bytes = serde_json::to_vec(request)?;
        if bytes.len() > FRAME {
            return fail("IPC request too large");
        }
        let mut stream = UnixStream::connect(&self.path)?;
        stream.set_read_timeout(Some(Duration::from_secs(60)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        if peer(&stream)?.uid != self.server_uid {
            return fail("untrusted broker identity");
        }
        send(&mut stream, &bytes, fd)?;
        let (bytes, fd) = receive(&mut stream, RESPONSE)?;
        if fd.is_some() {
            return fail("unexpected response descriptor");
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_and_owned_fd_transfer() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let f = tempfile::tempfile().unwrap();
        send(&mut a, b"test", Some(f.as_raw_fd())).unwrap();
        let (bytes, fd) = receive(&mut b, 100).unwrap();
        assert_eq!(bytes, b"test");
        assert!(fd.is_some());
        assert_eq!(peer(&a).unwrap().uid, unsafe { libc::geteuid() });
    }
    #[test]
    fn oversized_and_malformed_frames_fail() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        send(&mut a, b"too big", None).unwrap();
        assert!(receive(&mut b, 2).is_err());
        let (mut a, mut b) = UnixStream::pair().unwrap();
        a.write_all(b"x0000").unwrap();
        assert!(receive(&mut b, 100).is_err());
    }
}
