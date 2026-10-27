//! Socket location and creation. The parent directory must be ours and private (0700).

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("GOH_SOCKET") {
        return p.into();
    }
    let dir = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) => PathBuf::from(d).join("goh"),
        None => PathBuf::from(format!("/tmp/goh-{}", euid())),
    };
    dir.join("gohd.sock")
}

fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

fn prepare_dir(dir: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != euid() || meta.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} must be a directory owned by you with mode 0700", dir.display()),
        ));
    }
    Ok(())
}

/// Bind the socket (0600). `Ok(None)` means a live daemon already owns it.
pub fn bind(path: &Path) -> io::Result<Option<UnixListener>> {
    let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "socket path has no parent"))?;
    prepare_dir(dir)?;
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Ok(None);
        }
        fs::remove_file(path)?; // stale socket from a dead daemon
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(Some(listener))
}
