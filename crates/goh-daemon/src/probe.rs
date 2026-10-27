//! Ask the terminal for its default foreground/background (OSC 10/11) on the controlling tty.
//!
//! Safety rails (docs/05, docs/07): only runs on a real tty, bounded by a deadline (plus at most one
//! 100ms read) and a byte cap,
//! reads only what the terminal sends back, keeps nothing but well-formed colour replies, and always
//! restores the terminal mode. Callers should run it between prompts, not while the user is typing:
//! typeahead that arrives during the window is consumed.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use goh_theme::osc::{parse_replies, Replies, MAX_INPUT};

const QUERY: &[u8] = b"\x1b]10;?\x07\x1b]11;?\x07";

/// Puts the tty in raw mode (no echo, no line buffering) and restores it on drop.
struct RawMode {
    fd: i32,
    saved: libc::termios,
}

impl RawMode {
    fn enter(fd: i32) -> io::Result<RawMode> {
        // SAFETY: `termios` is plain old data; tcgetattr fills it or returns an error.
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = saved;
        unsafe { libc::cfmakeraw(&mut raw) };
        raw.c_cc[libc::VMIN] = 0;
        // Each read waits up to 100ms for a byte. (poll/select on /dev/tty are unreliable on macOS.)
        raw.c_cc[libc::VTIME] = 1;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(RawMode { fd, saved })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // SAFETY: restores the attributes read in `enter`.
        unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved) };
    }
}

/// Query the terminal. `Err` means "could not ask" (no tty); an empty `Replies` means "asked, no answer".
pub fn query_colors(timeout: Duration) -> io::Result<Replies> {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let fd = tty.as_raw_fd();
    let _raw = RawMode::enter(fd)?;
    tty.write_all(QUERY)?;
    tty.flush()?;

    let deadline = Instant::now() + timeout;
    let mut buf = Vec::with_capacity(256);
    while Instant::now() < deadline && buf.len() < MAX_INPUT {
        let mut chunk = [0u8; 128];
        // SAFETY: reads at most chunk.len() bytes into a live buffer.
        let n = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), chunk.len()) };
        if n < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            break;
        }
        if n > 0 {
            buf.extend_from_slice(&chunk[..n as usize]);
            if parse_replies(&buf).complete() {
                break;
            }
        }
    }
    Ok(parse_replies(&buf))
}
