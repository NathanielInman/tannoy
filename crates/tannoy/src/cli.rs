//! Command-line entry points.

use anyhow::{anyhow, Result};
use std::io::Write;
use std::time::Duration;
use tannoy_core::config::Config;
use tannoy_core::model::Snapshot;
use tannoy_core::protocol::{Request, SubKind};
use tannoy_core::text::clock;
use tannoy_core::{now, paths};

const HELP: &str = "\
tannoy: now playing for waybar, with cover art and a drawer

Usage:
  tannoy play-pause                 play or pause the picked player
  tannoy next | previous            skip
  tannoy seek <+secs|-secs|secs>    seek relative (+5, -15) or to a position (90)
  tannoy pick <player|auto>         send commands to this player (auto: the
                                    one that most recently started playing)
  tannoy toggle [--center]          open or close the drawer from the clicked icon
  tannoy show [--center] | hide     open or close the drawer
  tannoy bar                        the waybar module (prints JSON lines)
  tannoy art                        the cover's path, for a waybar image module
  tannoy status [--json]            what is playing, on which players
  tannoy doctor                     check the setup
  tannoy daemon                     run the daemon in the foreground
  tannoy --version

Config: ~/.config/tannoy/config.toml
";

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

pub fn run(args: &[String]) -> Result<i32> {
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            print!("{HELP}");
            Ok(0)
        }
        Some("-V" | "--version" | "version") => {
            println!("tannoy {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Some("daemon") => crate::daemon::run(),
        Some("bar") => bar(),
        Some("art") => art(),
        Some(cmd @ ("toggle" | "show")) => open(cmd == "toggle", rest),
        Some("hide") => {
            if tannoy_core::transport::daemon_alive() {
                send(&Request::Hide)?;
            }
            Ok(0)
        }
        Some("play-pause" | "toggle-play" | "pp") => command(Request::PlayPause),
        Some("next") => command(Request::Next),
        Some("previous" | "prev") => command(Request::Previous),
        Some("seek") => {
            let arg = rest
                .first()
                .ok_or_else(|| anyhow!("usage: tannoy seek <+secs|-secs|secs>"))?;
            command(parse_seek(arg)?)
        }
        Some("pick") => command(Request::Pick {
            player: rest.first().cloned(),
        }),
        Some("status") => status(flag(rest, "--json")),
        Some("doctor") => doctor(),
        Some(other) => Err(anyhow!("unknown command `{other}` (try `tannoy --help`)")),
    }
}

/// `+5` and `-15` seek relative; `90` goes to a position.
pub fn parse_seek(arg: &str) -> Result<Request> {
    let a = arg.trim();
    let n: f64 = a
        .trim_start_matches('+')
        .parse()
        .map_err(|_| anyhow!("`{arg}` is not a number of seconds"))?;
    if !n.is_finite() {
        return Err(anyhow!("`{arg}` is not a number of seconds"));
    }
    Ok(if a.starts_with('+') || a.starts_with('-') {
        Request::Seek { offset: n }
    } else {
        Request::SetPosition { position: n }
    })
}

fn send(req: &Request) -> Result<()> {
    let r = tannoy_core::protocol::request(req)?;
    match r.error {
        Some(e) => Err(anyhow!(e)),
        None => Ok(()),
    }
}

/// Media keys and bar clicks: start the daemon if needed, then ask it.
fn command(req: Request) -> Result<i32> {
    ensure_daemon()?;
    send(&req)?;
    Ok(0)
}

/// Starts the daemon in the background when it is not running, and waits
/// for its socket.
fn ensure_daemon() -> Result<()> {
    if tannoy_core::transport::daemon_alive() {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let log = paths::daemon_log();
    std::fs::create_dir_all(paths::state_dir())?;
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    {
        use std::os::unix::process::CommandExt;
        // Its own session, so restarting the bar does not take the daemon
        // with it.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn()?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(50));
        if tannoy_core::transport::daemon_alive() {
            std::thread::spawn(move || child.wait());
            return Ok(());
        }
        // Lost the race to another starter: theirs will answer shortly.
        if let Ok(Some(_)) = child.try_wait() {
            continue;
        }
    }
    Err(anyhow!("the daemon did not start; see {}", log.display()))
}

fn open(toggle: bool, args: &[String]) -> Result<i32> {
    let (cfg, _) = Config::load();
    // Ask the compositor first: the cursor is still on the icon.
    let anchor = bar_anchor::hypr::anchor(&cfg.bar.namespace, flag(args, "--center"));
    ensure_daemon()?;
    send(&if toggle {
        Request::Toggle { anchor }
    } else {
        Request::Show { anchor }
    })?;
    Ok(0)
}

fn bar() -> Result<i32> {
    let offline = serde_json::to_string(&crate::bar::offline())?;
    let mut out = std::io::stdout().lock();
    let mut last = String::new();
    let mut tried_start = false;
    loop {
        match tannoy_core::protocol::subscribe_raw(SubKind::Bar) {
            Ok(lines) => {
                for line in lines {
                    let Ok(line) = line else { break };
                    if line != last {
                        writeln!(out, "{line}")?;
                        out.flush()?;
                        last = line;
                    }
                }
            }
            Err(_) if !tried_start => {
                tried_start = true;
                let _ = ensure_daemon();
                continue;
            }
            Err(_) => {}
        }
        if last != offline {
            writeln!(out, "{offline}")?;
            out.flush()?;
            last = offline.clone();
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// For waybar's image module, which runs this on its own main thread: it
/// must answer at once, so it never starts the daemon or waits long.
///
/// With no cover it prints a path that never exists (`art/none` in the
/// cache): waybar hides the module for a missing file, but keeps showing the
/// previous image when the output is empty.
fn art() -> Result<i32> {
    let reply =
        tannoy_core::protocol::request_within(&Request::Art, Duration::from_millis(400)).ok();
    match reply.and_then(|r| r.art) {
        Some(path) => println!("{path}"),
        None => println!("{}", paths::no_art().display()),
    }
    Ok(0)
}

/// A snapshot from the daemon.
fn snapshot() -> Option<Snapshot> {
    tannoy_core::protocol::request(&Request::Snapshot)
        .ok()?
        .snapshot
        .map(|s| *s)
}

fn status(json: bool) -> Result<i32> {
    let Some(s) = snapshot() else {
        if json {
            println!("null");
        } else {
            println!(
                "The Tannoy daemon is not running (`tannoy bar` or `tannoy daemon` starts it)."
            );
        }
        return Ok(1);
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&s)?);
        return Ok(0);
    }
    let t = now();
    match s.active_player() {
        Some(p) if !p.track.is_empty() => {
            let tr = &p.track;
            println!("{:<10}{} ({})", "player", p.name, p.status.as_str());
            if s.hidden {
                println!("{:<10}hidden while do not disturb is on", "track");
            } else {
                println!("{:<10}{}", "title", tr.title);
                println!("{:<10}{}", "artist", tr.artist());
                if !tr.album.is_empty() {
                    println!("{:<10}{}", "album", tr.album);
                }
            }
            let pos = clock(p.position_now(t));
            match tr.length {
                Some(l) => println!("{:<10}{pos} / {}", "position", clock(l)),
                None => println!("{:<10}{pos}", "position"),
            }
            if tr.is_podcast() {
                println!("{:<10}podcast (the drawer shows skip buttons)", "kind");
            }
            if let Some(a) = &s.art {
                println!("{:<10}{a}", "art");
            }
        }
        Some(p) => println!("{:<10}{} (nothing loaded)", "player", p.name),
        None => println!("Nothing is playing."),
    }
    if !s.players.is_empty() {
        println!("players");
        for p in &s.players {
            let mark = if Some(&p.bus) == s.active.as_ref() {
                "*"
            } else {
                " "
            };
            println!(
                "  {mark} {:<12}{:<9}{}",
                p.key,
                p.status.as_str(),
                tannoy_core::mpris::suffix(&p.bus)
            );
        }
    }
    println!(
        "{:<10}{}",
        "pick",
        s.picked
            .as_deref()
            .unwrap_or("auto (most recently playing)")
    );
    println!(
        "{:<10}{}",
        "dnd",
        match (s.dnd, s.hidden) {
            (true, true) => "on, titles hidden",
            (true, false) => "on (dnd.hide is off)",
            _ => "off",
        }
    );
    if let Some(e) = &s.config_error {
        println!("\nconfig: {e}");
    }
    Ok(0)
}

fn check(ok: Option<bool>, what: &str, detail: &str) {
    let tag = match ok {
        Some(true) => "ok  ",
        Some(false) => "fail",
        None => "info",
    };
    if detail.is_empty() {
        println!("{tag} {what}");
    } else {
        println!("{tag} {what}: {detail}");
    }
}

fn doctor() -> Result<i32> {
    let mut failed = false;
    let (cfg, err) = Config::load();
    match err {
        Some(e) => {
            failed = true;
            check(Some(false), "config", &e);
        }
        None if paths::config_file().exists() => check(
            Some(true),
            "config",
            &paths::config_file().display().to_string(),
        ),
        None => check(None, "config", "no config file, using defaults"),
    }
    match crate::dbus::connect() {
        Ok(conn) => {
            check(Some(true), "session bus", "connected");
            let players = crate::dbus::list_players(&conn).unwrap_or_default();
            let shown: Vec<&str> = players
                .iter()
                .map(|b| tannoy_core::mpris::suffix(b))
                .filter(|s| !cfg.ignored(s))
                .collect();
            check(
                None,
                "players",
                &if shown.is_empty() {
                    "none running".to_string()
                } else {
                    shown.join(", ")
                },
            );
            match crate::dbus::swaync_dnd(&conn) {
                Some(d) => check(
                    Some(true),
                    "swaync",
                    &format!(
                        "do not disturb is {}{}",
                        if d { "on" } else { "off" },
                        if d && cfg.dnd.hide {
                            ", titles hidden"
                        } else {
                            ""
                        }
                    ),
                ),
                None => check(None, "swaync", "not running; do not disturb counts as off"),
            }
        }
        Err(e) => {
            failed = true;
            check(Some(false), "session bus", &format!("{e:#}"));
        }
    }
    let alive = tannoy_core::transport::daemon_alive();
    check(
        alive.then_some(true),
        "daemon",
        if alive {
            "running"
        } else {
            "not running (`tannoy bar` or a command starts it)"
        },
    );
    match bar_anchor::hypr::socket() {
        Some(_) => check(
            Some(true),
            "Hyprland IPC",
            "the drawer opens from the clicked icon",
        ),
        None => check(None, "Hyprland IPC", "not found; the drawer opens centered"),
    }
    match tannoy_core::waybar::config_path() {
        Some(p) => {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let has_bar = text.contains("tannoy bar");
            let has_art = text.contains("tannoy art");
            check(
                Some(has_bar),
                "waybar config",
                &format!(
                    "{}: {}, {}",
                    paths::tilde(&p),
                    if has_bar {
                        "has `tannoy bar`"
                    } else {
                        "no module runs `tannoy bar`"
                    },
                    if has_art {
                        "has `tannoy art`"
                    } else {
                        "no image module runs `tannoy art`"
                    }
                ),
            );
        }
        None => check(None, "waybar config", "not found"),
    }
    if cfg.art.enabled && cfg.art.signal > 0 {
        let sig = libc::SIGRTMIN() + cfg.art.signal as i32;
        let uid = unsafe { libc::getuid() };
        let bars = tannoy_core::proc::find(std::path::Path::new("/proc"), "waybar", uid);
        let catching = bars
            .iter()
            .filter(|(_, s)| tannoy_core::proc::catches(s, sig))
            .count();
        check(
            (!bars.is_empty()).then_some(catching == bars.len()),
            "art signal",
            &format!(
                "SIGRTMIN+{} to {} waybar process{}",
                cfg.art.signal,
                bars.len(),
                if bars.len() == 1 { "" } else { "es" }
            ),
        );
    }
    check(
        None,
        "art cache",
        &paths::tilde(&paths::art_dir()).to_string(),
    );
    let board = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("tannoy-board")))
        .filter(|p| p.exists());
    match board {
        Some(p) => check(Some(true), "drawer", &p.display().to_string()),
        None => check(
            None,
            "drawer",
            "tannoy-board not next to tannoy; it must be on PATH",
        ),
    }
    Ok(if failed { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_arguments() {
        assert_eq!(parse_seek("+5").unwrap(), Request::Seek { offset: 5.0 });
        assert_eq!(parse_seek("-15").unwrap(), Request::Seek { offset: -15.0 });
        assert_eq!(
            parse_seek("90").unwrap(),
            Request::SetPosition { position: 90.0 }
        );
        assert!(parse_seek("soon").is_err());
        assert!(parse_seek("+inf").is_err());
    }
}
