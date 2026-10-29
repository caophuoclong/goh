//! Starting and stopping the daemon from the command line (`goh start | stop | restart | reload | status`).

use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Start `gohd` (next to the running executable) detached from this terminal.
pub fn spawn() -> io::Result<()> {
    let gohd = std::env::current_exe()?.with_file_name("gohd");
    Command::new(gohd).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn().map(|_| ())
}

/// Wait (bounded) until the socket accepts connections.
pub fn wait_until_up(sock: &Path, limit: Duration) -> bool {
    wait(limit, || UnixStream::connect(sock).is_ok())
}

/// Wait (bounded) until a daemon that was just started answers. It binds the socket before it loads the
/// specs, so a request sent as soon as the socket is up can outlast the client's timeout on a busy machine.
pub fn wait_until_ready(sock: &Path, limit: Duration) -> bool {
    wait(limit, || crate::client::hello(sock).is_ok())
}

fn wait(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// The process on the other end of a connected socket. The socket's directory is private (0700), so it
/// is ours, but the name is still checked before anything is signalled.
fn peer_pid(stream: &UnixStream) -> Option<i32> {
    let fd = stream.as_raw_fd();
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    #[cfg(target_os = "macos")]
    // SAFETY: valid fd; `pid` and `len` describe a pid_t-sized buffer. LOCAL_PEERPID is 0x002 at SOL_LOCAL (0).
    let r = unsafe { libc::getsockopt(fd, 0, 0x002, (&mut pid as *mut libc::pid_t).cast(), &mut len) };
    #[cfg(not(target_os = "macos"))]
    let r = {
        let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
        let mut l = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: valid fd; `cred` is a ucred-sized buffer.
        let r = unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut l) };
        pid = cred.pid;
        let _ = &mut len;
        r
    };
    (r == 0 && pid > 0).then_some(pid)
}

/// By process name, not executable path: an upgrade deletes the old binary under a daemon that is still
/// running, and its path can then no longer be read (macOS) or ends in " (deleted)" (Linux).
fn is_gohd(pid: i32) -> bool {
    #[cfg(target_os = "macos")]
    let name = {
        let mut buf = [0u8; 64];
        // SAFETY: buffer length is passed alongside the pointer.
        let n = unsafe { libc::proc_name(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
    };
    #[cfg(not(target_os = "macos"))]
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim_end().to_string());
    name.is_some_and(|n| n == "gohd")
}

/// pid of the daemon behind `sock`, if one answers there.
pub fn running(sock: &Path) -> Option<i32> {
    peer_pid(&UnixStream::connect(sock).ok()?).filter(|&p| is_gohd(p))
}

/// Stop the daemon behind `sock`. `Ok(None)`: none was running. `Ok(Some(pid))`: it is gone.
pub fn stop(sock: &Path) -> io::Result<Option<i32>> {
    let Some(pid) = running(sock) else { return Ok(None) };
    // SAFETY: plain signal to a process verified to be gohd.
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if !wait(Duration::from_secs(2), || UnixStream::connect(sock).is_err()) {
        return Err(io::Error::other(format!("gohd (pid {pid}) did not stop within 2 s")));
    }
    let _ = std::fs::remove_file(sock);
    Ok(Some(pid))
}
