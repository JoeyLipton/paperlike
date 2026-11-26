//! Notices when the config file is saved, so edits made in a text editor
//! apply straight away.
//!
//! This watches the file's directory rather than the file itself: many
//! editors save by writing a new file and renaming it over the old one,
//! which would silently end a watch on the original file.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Size of `struct inotify_event` without its trailing name.
const EVENT_HEADER: usize = 16;

pub struct FileWatch {
    fd: OwnedFd,
    name: Vec<u8>,
}

impl FileWatch {
    /// Starts watching `path`, creating its directory if needed.
    pub fn new(path: &Path) -> io::Result<FileWatch> {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a file path"));
        };
        std::fs::create_dir_all(dir)?;
        let dir = CString::new(dir.as_os_str().as_bytes())?;

        // SAFETY: inotify_init1 has no preconditions; on success we take
        // ownership of the new descriptor straight away.
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };

        // A finished write, or another file renamed onto ours. Creation is
        // left out on purpose: a freshly created file is still empty.
        let mask = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO;
        // SAFETY: `fd` is a valid inotify descriptor and `dir` a C string.
        if unsafe { libc::inotify_add_watch(fd.as_raw_fd(), dir.as_ptr(), mask) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(FileWatch { fd, name: name.as_bytes().to_vec() })
    }

    pub fn fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    /// Reads all pending events. Returns true if any of them were for the
    /// watched file.
    pub fn changed(&self) -> bool {
        // Aligned for the kernel's event structs.
        let mut buf = vec![0u64; 512];
        let mut hit = false;
        loop {
            // SAFETY: reading into a buffer we own, of the length we pass.
            let n = unsafe { libc::read(self.fd(), buf.as_mut_ptr().cast(), buf.len() * 8) };
            if n <= 0 {
                break;
            }
            let bytes: Vec<u8> =
                buf.iter().flat_map(|w| w.to_ne_bytes()).take(n as usize).collect();
            let mut off = 0;
            while off + EVENT_HEADER <= bytes.len() {
                let len = u32::from_ne_bytes([
                    bytes[off + 12],
                    bytes[off + 13],
                    bytes[off + 14],
                    bytes[off + 15],
                ]) as usize;
                let end = (off + EVENT_HEADER + len).min(bytes.len());
                let raw_name = &bytes[off + EVENT_HEADER..end];
                // The name is NUL-padded.
                let name = raw_name.split(|&b| b == 0).next().unwrap_or_default();
                if name == self.name {
                    hit = true;
                }
                off = end;
            }
        }
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_writes_and_renames() {
        let dir = std::env::temp_dir().join(format!("paperlike-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("paperlike.toml");
        let watch = FileWatch::new(&file).unwrap();
        assert!(!watch.changed());

        std::fs::write(dir.join("other.toml"), "x").unwrap();
        assert!(!watch.changed(), "unrelated file");

        std::fs::write(&file, "enabled = false\n").unwrap();
        assert!(watch.changed(), "direct write");
        assert!(!watch.changed(), "events are drained");

        let tmp = dir.join("paperlike.toml.tmp");
        std::fs::write(&tmp, "enabled = true\n").unwrap();
        watch.changed();
        std::fs::rename(&tmp, &file).unwrap();
        assert!(watch.changed(), "rename onto the file");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
