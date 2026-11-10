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
