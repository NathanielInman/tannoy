//! Where Tannoy keeps things. Every location can be moved with an
//! environment variable, which tests and development builds use to stay away
//! from an installed copy.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Expands a leading `~`.
pub fn expand(s: &str) -> PathBuf {
    let s = s.trim();
    if s == "~" {
        home()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(s)
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn xdg_config() -> PathBuf {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

pub fn config_dir() -> PathBuf {
    env_dir("TANNOY_CONFIG_DIR").unwrap_or_else(|| xdg_config().join("tannoy"))
}

/// Logs and the picked player.
pub fn state_dir() -> PathBuf {
    if let Some(d) = env_dir("TANNOY_STATE_DIR") {
        return d;
    }
    env_dir("XDG_STATE_HOME")
        .unwrap_or_else(|| home().join(".local/state"))
        .join("tannoy")
}

/// Cover art and anything else that can be fetched again.
pub fn cache_dir() -> PathBuf {
    if let Some(d) = env_dir("TANNOY_CACHE_DIR") {
        return d;
    }
    env_dir("XDG_CACHE_HOME")
        .unwrap_or_else(|| home().join(".cache"))
        .join("tannoy")
}

pub fn art_dir() -> PathBuf {
    cache_dir().join("art")
}

/// What `tannoy art` prints when there is no cover: a path that is never
/// created, so waybar's image module hides itself.
pub fn no_art() -> PathBuf {
    art_dir().join("none")
}

/// Per-user directory for the socket, shared by the daemon, bar module and
/// drawer.
pub fn runtime_dir() -> PathBuf {
    if let Some(d) = env_dir("TANNOY_RUNTIME_DIR") {
        return d;
    }
    #[cfg(unix)]
    {
        if let Some(d) = env_dir("XDG_RUNTIME_DIR") {
            return d.join("tannoy");
        }
        let base = env_dir("TMPDIR").unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join(format!("tannoy-{}", unsafe { libc::getuid() }))
    }
    #[cfg(not(unix))]
    {
        home().join(".tannoy").join("run")
    }
}

pub fn socket_path() -> PathBuf {
    runtime_dir().join("tannoy.sock")
}

/// Held (flock) by the running daemon, so two daemons started at once by the
/// bar module and the art module cannot both bind the socket.
pub fn lock_path() -> PathBuf {
    runtime_dir().join("daemon.lock")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// The user's stylesheet for the drawer, loaded over the theme.
pub fn style_file() -> PathBuf {
    config_dir().join("style.css")
}

/// The picked player survives restarts here.
pub fn state_file() -> PathBuf {
    state_dir().join("state.json")
}

pub fn board_log() -> PathBuf {
    state_dir().join("board.log")
}

pub fn daemon_log() -> PathBuf {
    state_dir().join("daemon.log")
}

/// A log past this size is rolled over to `<name>.1` when it is opened.
pub const LOG_MAX: u64 = 1024 * 1024;

/// Opens a log for appending, first moving it to `<name>.1` (replacing the
/// previous one) when it is over [`LOG_MAX`], so logs never grow without
/// bound. Logs are opened each time the daemon or the drawer starts.
pub fn open_log(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LOG_MAX) {
        let mut old = path.as_os_str().to_owned();
        old.push(".1");
        std::fs::rename(path, old)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
}

/// Waybar's config directory.
pub fn waybar_dir() -> PathBuf {
    xdg_config().join("waybar")
}

/// Shows a path under the home directory as `~/...`.
pub fn tilde(p: &std::path::Path) -> String {
    let home = home();
    match p.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// Linux names a running program's replaced binary `<path> (deleted)`.
const DELETED: &str = " (deleted)";

/// A program path as `/proc/self/exe` reports it, without the ` (deleted)`
/// Linux adds once the file was replaced, and whether it was.
pub fn split_deleted(p: &std::path::Path) -> (PathBuf, bool) {
    let s = p.to_string_lossy();
    match s.strip_suffix(DELETED) {
        Some(rest) => (PathBuf::from(rest), true),
        None => (p.to_path_buf(), false),
    }
}

/// This program's installed path, still right after an upgrade replaced
/// the file (`current_exe` then ends in ` (deleted)`).
pub fn exe() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|p| split_deleted(&p).0)
}

/// The binary this process runs was replaced on disk: an upgrade installed
/// a new one, and this process is the old code.
pub fn exe_replaced() -> bool {
    std::fs::read_link("/proc/self/exe").is_ok_and(|p| split_deleted(&p).1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn replaced_binaries() {
        assert_eq!(
            split_deleted(Path::new("/home/n/.local/bin/tannoy (deleted)")),
            (PathBuf::from("/home/n/.local/bin/tannoy"), true)
        );
        assert_eq!(
            split_deleted(Path::new("/usr/bin/tannoy")),
            (PathBuf::from("/usr/bin/tannoy"), false)
        );
        assert!(!exe_replaced());
    }

    #[test]
    fn logs_roll_over_past_the_limit() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("tannoy-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = dir.join("daemon.log");
        open_log(&log).unwrap().write_all(b"small\n").unwrap();
        open_log(&log).unwrap().write_all(b"kept\n").unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "small\nkept\n");
        std::fs::write(&log, vec![b'x'; LOG_MAX as usize + 1]).unwrap();
        open_log(&log).unwrap().write_all(b"fresh\n").unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "fresh\n");
        let rolled = dir.join("daemon.log.1");
        assert_eq!(std::fs::metadata(&rolled).unwrap().len(), LOG_MAX + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
