//! File changes from inotify, so the daemon sleeps until the config changes
//! instead of checking it on a timer.
//!
//! The directory is watched, not the file: most editors save by writing a
//! new file and renaming it over the old one, which a watch on the old file
//! never sees.

use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// `wd`, `mask`, `cookie` and `len` before each event's name.
const HEADER: usize = 16;

/// The mask and name of each event in what was read from an inotify fd.
pub fn events(buf: &[u8]) -> Vec<(u32, String)> {
    let word = |at: usize| u32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
    let mut out = vec![];
    let mut i = 0;
    while i + HEADER <= buf.len() {
        let mask = word(i + 4);
        let len = word(i + 12) as usize;
        let end = (i + HEADER + len).min(buf.len());
        // The name is padded with NULs to a multiple of 4 bytes.
        let name = buf[i + HEADER..end]
            .split(|b| *b == 0)
            .next()
            .unwrap_or(&[]);
        out.push((mask, String::from_utf8_lossy(name).into_owned()));
        i += HEADER + len;
    }
    out
}

struct Inotify(OwnedFd);

impl Inotify {
    fn new() -> Option<Inotify> {
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        (fd >= 0).then(|| Inotify(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    fn add(&self, dir: &Path, mask: u32) -> bool {
        let Ok(c) = CString::new(dir.as_os_str().as_bytes()) else {
            return false;
        };
        unsafe { libc::inotify_add_watch(self.0.as_raw_fd(), c.as_ptr(), mask) >= 0 }
    }

    /// Blocks until something happens.
    fn read(&self) -> Option<Vec<(u32, String)>> {
        let mut buf = [0u8; 4096];
        let n = unsafe { libc::read(self.0.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        (n > 0).then(|| events(&buf[..n as usize]))
    }
}

const CHANGES: u32 = libc::IN_CLOSE_WRITE
    | libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_MOVED_FROM
    | libc::IN_MOVED_TO;
/// The watched directory itself went away.
const GONE: u32 = libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED;

/// Calls `changed` whenever `file` is written, created, replaced or removed,
/// and once each time the watch on its directory starts (the file may have
/// changed while nothing watched it). Waits for the directory to appear
/// when it does not exist yet. Blocks for good; run it on its own thread.
/// Returns only when inotify cannot be used.
pub fn watch_file(file: &Path, changed: impl Fn()) {
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else {
        return;
    };
    let name = name.to_string_lossy();
    loop {
        let Some(ino) = Inotify::new() else { return };
        if dir.is_dir() {
            if !ino.add(dir, CHANGES | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF) {
                return;
            }
            changed();
            while let Some(evs) = ino.read() {
                if evs.iter().any(|(_, n)| *n == name) {
                    changed();
                }
                if evs.iter().any(|(m, _)| m & GONE != 0) {
                    break;
                }
            }
        } else {
            // Wait for the directory to be created, then watch it.
            let (Some(parent), Some(dir_name)) = (dir.parent(), dir.file_name()) else {
                return;
            };
            let dir_name = dir_name.to_string_lossy();
            if !ino.add(parent, libc::IN_CREATE | libc::IN_MOVED_TO) {
                return;
            }
            while !dir.is_dir() {
                match ino.read() {
                    Some(evs) if evs.iter().any(|(_, n)| *n == dir_name) => break,
                    Some(_) => {}
                    None => return,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(mask: u32, name: &str) -> Vec<u8> {
        let mut padded = name.as_bytes().to_vec();
        padded.resize((name.len() / 4 + 1) * 4, 0);
        let mut b = vec![];
        b.extend_from_slice(&1i32.to_ne_bytes());
        b.extend_from_slice(&mask.to_ne_bytes());
        b.extend_from_slice(&0u32.to_ne_bytes());
        b.extend_from_slice(&(padded.len() as u32).to_ne_bytes());
        b.extend_from_slice(&padded);
        b
    }

    #[test]
    fn reads_packed_events() {
        let mut buf = event(libc::IN_CREATE, ".config.toml.swp");
        buf.extend(event(libc::IN_MOVED_TO, "config.toml"));
        buf.extend(event(libc::IN_IGNORED, ""));
        assert_eq!(
            events(&buf),
            vec![
                (libc::IN_CREATE, ".config.toml.swp".into()),
                (libc::IN_MOVED_TO, "config.toml".into()),
                (libc::IN_IGNORED, String::new()),
            ]
        );
    }

    #[test]
    fn sees_a_rename_over_the_file() {
        let dir = std::env::temp_dir().join(format!("tannoy-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("config.toml");
        let (tx, rx) = std::sync::mpsc::channel();
        let f = file.clone();
        std::thread::spawn(move || {
            watch_file(&f, move || {
                let _ = tx.send(());
            })
        });
        let wait = || rx.recv_timeout(std::time::Duration::from_secs(2)).is_ok();
        // The directory does not exist yet: creating it arms the watch.
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(wait(), "no call when the directory appeared");
        std::fs::write(dir.join(".config.toml.new"), "[bar]\n").unwrap();
        std::fs::rename(dir.join(".config.toml.new"), &file).unwrap();
        assert!(wait(), "no call for a rename over the file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
