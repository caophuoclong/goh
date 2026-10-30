//! The popup over ssh (`goh ssh`, see ssh.rs). A Unix socket here is forwarded to the remote host, whose path the
//! adapter there reads from `LC_GOH_POPUP` (macOS ssh sends `LC_*`, Debian and Ubuntu sshd accept it). That adapter
//! talks to `goh popup-bridge $LC_GOH_POPUP` instead of starting a popup itself, and each connection here gets its
//! own local `goh-popup`, which finds the caret in the terminal `goh ssh` runs in. Frames and answers are the same
//! newline-delimited JSON a local popup reads and writes.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The variable that carries the remote socket path to the adapter.
pub const VAR: &str = "LC_GOH_POPUP";

/// A listening relay. Its private folder (the socket, and room for ssh's control socket) goes when it is dropped.
pub struct Relay {
    pub dir: PathBuf,
    pub local: PathBuf,
    /// Where the socket is forwarded to on the remote host.
    pub remote: String,
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Listen for remote adapters. `None` (and a line saying why) when there is no popup to hand them to.
pub fn start() -> Option<Relay> {
    let Some(popup) = popup_binary() else {
        eprintln!("goh: goh-popup not found next to goh or on PATH; menus over ssh stay inline");
        return None;
    };
    let dir = std::env::temp_dir().join(format!("goh-ssh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        eprintln!("goh: {}: {e}", dir.display());
        return None;
    }
    let relay = Relay { local: dir.join("popup.sock"), remote: format!("/tmp/goh-popup-{}.sock", random_hex()), dir };
    let listener = match UnixListener::bind(&relay.local) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("goh: {}: {e}", relay.local.display());
            return None;
        }
    };
    // Asked here, while nothing else reads the terminal: the remote shell cannot ask it for this machine's popup.
    let cell_height = (std::env::var("TERM_PROGRAM").is_ok_and(|p| p == "iTerm.app") && std::env::var_os("TMUX").is_none())
        .then(goh_anchor::iterm_cell_height)
        .flatten();
    let own_permission = std::env::var("GOH_POPUP_OWN_PERMISSION").ok().or_else(|| {
        let lines = crate::config::env_lines(&goh_theme::read_config()?);
        lines.iter().find_map(|l| l.strip_prefix("GOH_POPUP_OWN_PERMISSION=").map(str::to_string))
    });
    std::thread::spawn(move || {
        for conn in listener.incoming().map_while(Result::ok) {
            let (popup, own_permission) = (popup.clone(), own_permission.clone());
            std::thread::spawn(move || serve(conn, &popup, cell_height, own_permission.as_deref()));
        }
    });
    Some(relay)
}

/// One remote adapter's popup: frames in, answers out, until either side closes.
fn serve(conn: UnixStream, popup: &Path, cell_height: Option<f64>, own_permission: Option<&str>) {
    let mut cmd = Command::new(popup);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(v) = own_permission {
        cmd.env("GOH_POPUP_OWN_PERMISSION", v);
    }
    let Ok(mut child) = cmd.spawn() else { return };
    let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else { return };
    let Ok(mut back) = conn.try_clone() else { return };
    let answers = std::thread::spawn(move || {
        let _ = io::copy(&mut stdout, &mut back);
        let _ = back.shutdown(Shutdown::Both);
    });
    for line in BufReader::new(&conn).lines().map_while(Result::ok) {
        let line = cell_height.map_or_else(|| line.clone(), |h| with_cell_height(&line, h));
        if writeln!(stdin, "{line}").and_then(|_| stdin.flush()).is_err() {
            break;
        }
    }
    drop(stdin); // the popup exits when its input ends
    let _ = child.wait();
    let _ = answers.join();
}

/// A `show` frame that does not say the cell height gets this machine's.
fn with_cell_height(line: &str, h: f64) -> String {
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(line) else { return line.to_string() };
    let Some(obj) = v.as_object_mut().filter(|o| o.get("type").and_then(|t| t.as_str()) == Some("show")) else { return line.to_string() };
    if obj.get("cell_height").is_some_and(|c| !c.is_null()) {
        return line.to_string();
    }
    obj.insert("cell_height".into(), h.into());
    v.to_string()
}

/// `goh popup-bridge SOCKET` (remote side, run by the adapter in place of goh-popup): stdin to the socket,
/// the socket to stdout. Without a connection it answers every frame "inline", as a popup that cannot draw does.
pub fn bridge(socket: &str) -> bool {
    let conn = match UnixStream::connect(socket) {
        Ok(c) => c,
        Err(e) => {
            let reason = format!("the popup relay is gone ({e}); reconnect with `goh ssh`");
            for line in io::stdin().lock().lines().map_while(Result::ok) {
                if line.contains(r#""type":"quit""#) {
                    break;
                }
                if line.contains(r#""type":"show""#) {
                    println!("{}", serde_json::json!({"type": "decision", "display": "inline", "reason": reason}));
                    let _ = io::stdout().flush();
                }
            }
            return true;
        }
    };
    let Ok(mut out) = conn.try_clone() else { return false };
    std::thread::spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut out);
        let _ = out.shutdown(Shutdown::Write);
    });
    let mut buf = [0u8; 8192];
    let (mut conn, mut stdout) = (conn, io::stdout().lock());
    loop {
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return true,
            Ok(n) => {
                if stdout.write_all(&buf[..n]).and_then(|_| stdout.flush()).is_err() {
                    return true;
                }
            }
        }
    }
}

fn popup_binary() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("goh-popup"))).filter(|p| p.is_file());
    beside.or_else(|| std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join("goh-popup")).find(|p| p.is_file()))
}

fn random_hex() -> String {
    let mut b = [0u8; 8];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        b = (t ^ u64::from(std::process::id())).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_frames_get_the_cell_height_they_lack() {
        let v: serde_json::Value = serde_json::from_str(&with_cell_height(r#"{"type":"show","state":{}}"#, 17.0)).unwrap();
        assert_eq!(v["cell_height"], 17.0);
        let kept: serde_json::Value = serde_json::from_str(&with_cell_height(r#"{"type":"show","cell_height":20}"#, 17.0)).unwrap();
        assert_eq!(kept["cell_height"], 20);
        assert_eq!(with_cell_height(r#"{"type":"hide"}"#, 17.0), r#"{"type":"hide"}"#);
        assert_eq!(with_cell_height("not json", 17.0), "not json");
    }
}
