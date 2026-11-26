//! The command socket. `paperlike <command>` connects, sends one line and
//! prints whatever the daemon answers.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

pub fn socket_path() -> PathBuf {
    let dir =
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let display: String = std::env::var("DISPLAY")
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    dir.join(format!("paperlike{display}.sock"))
}

/// Sends one command to a running daemon and returns its reply.
pub fn send(cmd: &str) -> std::io::Result<String> {
    let mut s = UnixStream::connect(socket_path())?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.write_all(cmd.as_bytes())?;
    s.write_all(b"\n")?;
    s.shutdown(std::net::Shutdown::Write)?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

impl Server {
    /// Binds the socket. Fails if another daemon is already listening.
    pub fn bind() -> std::io::Result<Server> {
        let path = socket_path();
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    "paperlike is already running",
                ));
            }
            let _ = std::fs::remove_file(&path);
        }
        let listener = UnixListener::bind(&path).map_err(|e| {
            std::io::Error::new(e.kind(), format!("cannot listen on {}: {e}", path.display()))
        })?;
        // Only matters when falling back to /tmp; $XDG_RUNTIME_DIR is private anyway.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        Ok(Server { listener, path })
    }

    pub fn fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }

    /// Accepts every pending connection and reads its request line.
    pub fn accept(&self) -> Vec<(UnixStream, String)> {
        let mut out = Vec::new();
        while let Ok((stream, _)) = self.listener.accept() {
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut line = String::new();
            if let Ok(clone) = stream.try_clone() {
                let _ = BufReader::new(clone).read_line(&mut line);
            }
            out.push((stream, line.trim().to_string()));
        }
        out
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn reply(mut stream: UnixStream, msg: &str) {
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let _ = stream.write_all(msg.as_bytes());
    if !msg.ends_with('\n') {
        let _ = stream.write_all(b"\n");
    }
}
