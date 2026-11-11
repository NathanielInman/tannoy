//! The daemon: tracks every MPRIS player from D-Bus signals, keeps the
//! snapshot, serves the socket, feeds the bar module, caches cover art,
//! and starts the drawer the first time it is asked for.
//!
//! Nothing here polls for metadata. Players announce changes with
//! PropertiesChanged, come and go with NameOwnerChanged, and report jumps
//! with Seeked; swaync announces do not disturb with its Subscribe signals;
//! Hyprland's event socket announces screen shares.
//! The one thing MPRIS never signals is the position, so the daemon reads
//! it about once a second while the drawer is open, and never otherwise.
//! Every D-Bus call and download runs on a helper thread or a client's
//! connection thread, never under the lock that feeds the bar.

use crate::dbus;
use anyhow::{anyhow, Result};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};
use tannoy_core::config::Config;
use tannoy_core::model::{Player, Snapshot};
use tannoy_core::mpris::{self, PATH, PLAYER_IFACE, ROOT_IFACE};
use tannoy_core::protocol::{Event, Reply, Request, SubKind};
use tannoy_core::transport::{Listener, Stream};
use tannoy_core::{art, paths, pick};
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MsgType;
use zbus::zvariant::OwnedValue;
use zbus::MatchRule;

/// How often the position is read while the drawer is open.
const POSITION_EVERY: Duration = Duration::from_secs(1);
/// How often the config file is checked for changes.
const CONFIG_EVERY: Duration = Duration::from_secs(2);

struct State {
    cfg: Config,
    cfg_error: Option<String>,
    cfg_mtime: Option<SystemTime>,
    players: BTreeMap<String, Player>,
    /// Unique name (`:1.39`) to well-known name. Signals carry the former.
    owners: HashMap<String, String>,
    picked: Option<String>,
    dnd: bool,
    /// A screen share is running.
    sharing: bool,
    visible: bool,
    /// The first scan of the bus is done; commands wait for it.
    ready: bool,
    /// The art URL of the active track and its cached copy.
    art_url: Option<String>,
    art_path: Option<String>,
    /// What `tannoy art` prints; a change signals waybar.
    art_line: Option<String>,
    snap: Snapshot,
    boards: Vec<Stream>,
    bars: Vec<Stream>,
    bar_line: String,
    /// A show the drawer has not received yet because it is still starting.
    pending: Option<Event>,
    board_started: Option<Instant>,
    position_busy: bool,
}

pub struct Daemon {
    state: Mutex<State>,
    cv: Condvar,
    conn: Connection,
}

type Shared = Arc<Daemon>;

impl Daemon {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn send_line(s: &mut Stream, line: &str) -> bool {
    s.write_all(line.as_bytes())
        .and_then(|_| s.write_all(b"\n"))
        .and_then(|_| s.flush())
        .is_ok()
}

/// Writes to every subscriber and drops the ones that went away. Returns how
/// many received it.
fn broadcast(subs: &mut Vec<Stream>, line: &str) -> usize {
    subs.retain_mut(|s| send_line(s, line));
    subs.len()
}

fn event_line(e: &Event) -> String {
    serde_json::to_string(e).unwrap_or_default()
}

fn config_mtime() -> Option<SystemTime> {
    std::fs::metadata(paths::config_file())
        .and_then(|m| m.modified())
        .ok()
}

fn load_pick() -> Option<String> {
    let text = std::fs::read_to_string(paths::state_file()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("picked")?.as_str().map(String::from)
}

fn save_pick(picked: Option<&str>) {
    let _ = std::fs::create_dir_all(paths::state_dir());
    let body = serde_json::json!({ "picked": picked });
    let _ = std::fs::write(paths::state_file(), format!("{body}\n"));
}

/// Sends `SIGRTMIN+n` to every waybar of this user that handles it, so
/// its image module runs `tannoy art` again.
fn signal_waybar(n: u8) {
    if n == 0 {
        return;
    }
    let sig = libc::SIGRTMIN() + n as i32;
    let uid = unsafe { libc::getuid() };
    for (pid, status) in tannoy_core::proc::find(std::path::Path::new("/proc"), "waybar", uid) {
        if tannoy_core::proc::catches(&status, sig) {
            unsafe {
                libc::kill(pid, sig);
            }
        }
    }
}

/// Recomputes the snapshot and pushes whatever changed: the bar line to bar
/// subscribers, the snapshot to the drawer, a signal to waybar when the art
/// path changed.
fn refresh(d: &Shared, st: &mut State) {
    let players: Vec<Player> = st.players.values().cloned().collect();
    let active = pick::choose(&players, st.picked.as_deref()).map(|i| players[i].bus.clone());
    let cfg = st.cfg.clone();

    let want = active
        .as_ref()
        .and_then(|b| st.players.get(b))
        .and_then(|p| p.track.art_url.clone())
        .filter(|_| cfg.art.enabled);
    if want != st.art_url {
        st.art_url = want.clone();
        st.art_path = None;
        if let Some(url) = want {
            match art::cached(&paths::art_dir(), &url) {
                Some(p) => st.art_path = Some(p.display().to_string()),
                None => fetch_art(d.clone(), url, cfg.art.timeout),
            }
        }
    }

    let snap = Snapshot {
        version: env!("CARGO_PKG_VERSION").into(),
        now: tannoy_core::now(),
        players,
        active,
        picked: st.picked.clone(),
        dnd: st.dnd,
        hidden: (st.dnd && cfg.dnd.hide) || (st.sharing && cfg.dnd.screen_share),
        sharing: st.sharing,
        art: st.art_path.clone(),
        config_error: st.cfg_error.clone(),
    };
    let bar = serde_json::to_string(&crate::bar::line(&snap, &cfg)).unwrap_or_default();
    if bar != st.bar_line {
        st.bar_line = bar.clone();
        broadcast(&mut st.bars, &bar);
    }
    let art_line = crate::bar::art_path(&snap, &cfg);
    if art_line != st.art_line {
        st.art_line = art_line;
        let n = cfg.art.signal;
        std::thread::spawn(move || signal_waybar(n));
    }
    let ev = event_line(&Event::Snapshot {
        snapshot: Box::new(snap.clone()),
    });
    broadcast(&mut st.boards, &ev);
    st.snap = snap;
}

/// Copies or downloads the art on a helper thread. A local file may be
/// announced a moment before the player finishes writing it, so those get
/// a few tries.
fn fetch_art(d: Shared, url: String, timeout: u64) {
    std::thread::spawn(move || {
        let local = matches!(art::source(&url), Some(art::Source::File(_)));
        let tries = if local { 4 } else { 1 };
        let mut result = Err(anyhow!("not tried"));
        for i in 0..tries {
            if i > 0 {
                std::thread::sleep(Duration::from_millis(300));
            }
            result = art::fetch(&paths::art_dir(), &url, Duration::from_secs(timeout.max(1)));
            if result.is_ok() {
                break;
            }
        }
        let mut st = d.lock();
        if st.art_url.as_deref() != Some(url.as_str()) {
            return;
        }
        match result {
            Ok(p) => {
                st.art_path = Some(p.display().to_string());
                refresh(&d, &mut st);
            }
            Err(e) => eprintln!("tannoy: no art from {url}: {e:#}"),
        }
    });
}

/// Starts tracking a player: maps its unique name, then reads both MPRIS
/// interfaces on this helper thread.
fn load(d: Shared, bus: String) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if d.lock().cfg.ignored(mpris::suffix(&bus)) {
            return;
        }
        let Some(owner) = dbus::name_owner(&d.conn, &bus) else {
            return;
        };
        {
            let mut st = d.lock();
            st.owners.insert(owner, bus.clone());
            st.players
                .entry(bus.clone())
                .or_insert_with(|| Player::new(&bus));
        }
        let player = dbus::get_all(&d.conn, &bus, PLAYER_IFACE);
        let root = dbus::get_all(&d.conn, &bus, ROOT_IFACE);
        let mut st = d.lock();
        let now = tannoy_core::now();
        if let Some(p) = st.players.get_mut(&bus) {
            match player {
                Ok(props) => {
                    mpris::apply_player(p, &props, now);
                }
                Err(e) => eprintln!("tannoy: reading {bus}: {e:#}"),
            }
            if let Ok(props) = root {
                mpris::apply_root(p, &props);
            }
        }
        refresh(&d, &mut st);
    })
}

fn forget(d: &Shared, bus: &str) {
    let mut st = d.lock();
    st.owners.retain(|_, b| b != bus);
    if st.players.remove(bus).is_some() {
        refresh(d, &mut st);
    }
}

fn rule(iface: &str, member: &str, path: &str) -> Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(MsgType::Signal)
        .interface(iface.to_string())?
        .member(member.to_string())?
        .path(path.to_string())?
        .build())
}

/// Players appearing and leaving, and swaync starting or stopping.
fn watch_names(d: Shared) -> Result<()> {
    let r = rule(
        "org.freedesktop.DBus",
        "NameOwnerChanged",
        "/org/freedesktop/DBus",
    )?;
    for msg in MessageIterator::for_match_rule(r, &d.conn, Some(256))? {
        let Ok(msg) = msg else { continue };
        let Ok((name, old, new)) = msg.body().deserialize::<(String, String, String)>() else {
            continue;
        };
        if mpris::is_player_bus(&name) {
            if !old.is_empty() {
                forget(&d, &name);
            }
            if !new.is_empty() {
                load(d.clone(), name);
            }
        } else if name == dbus::SWAYNC {
            let d = d.clone();
            std::thread::spawn(move || {
                let dnd = if new.is_empty() {
                    false
                } else {
                    std::thread::sleep(Duration::from_millis(300));
                    dbus::swaync_dnd(&d.conn).unwrap_or(false)
                };
                let mut st = d.lock();
                st.dnd = dnd;
                refresh(&d, &mut st);
            });
        }
    }
    Err(anyhow!("the session bus closed"))
}

/// Metadata, status and capability changes.
fn watch_properties(d: Shared) -> Result<()> {
    let r = rule("org.freedesktop.DBus.Properties", "PropertiesChanged", PATH)?;
    for msg in MessageIterator::for_match_rule(r, &d.conn, Some(256))? {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let Some(sender) = header.sender().map(|s| s.to_string()) else {
            continue;
        };
        let Ok((iface, changed, invalidated)) =
            msg.body()
                .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
        else {
            continue;
        };
        let mut st = d.lock();
        let Some(bus) = st.owners.get(&sender).cloned() else {
            continue;
        };
        let Some(p) = st.players.get_mut(&bus) else {
            continue;
        };
        let props = dbus::props(&changed);
        if iface == PLAYER_IFACE {
            mpris::apply_player(p, &props, tannoy_core::now());
        } else if iface == ROOT_IFACE {
            mpris::apply_root(p, &props);
        } else {
            continue;
        }
        refresh(&d, &mut st);
        drop(st);
        // A player may announce a change without the new value; read it.
        if iface == PLAYER_IFACE && !invalidated.is_empty() {
            load(d.clone(), bus);
        }
    }
    Err(anyhow!("the session bus closed"))
}

/// Jumps in position (the player's own scrub bar, or ours).
fn watch_seeked(d: Shared) -> Result<()> {
    let r = rule(PLAYER_IFACE, "Seeked", PATH)?;
    for msg in MessageIterator::for_match_rule(r, &d.conn, Some(64))? {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let Some(sender) = header.sender().map(|s| s.to_string()) else {
            continue;
        };
        let Ok((pos,)) = msg.body().deserialize::<(i64,)>() else {
            continue;
        };
        let mut st = d.lock();
        let Some(bus) = st.owners.get(&sender).cloned() else {
            continue;
        };
        if let Some(p) = st.players.get_mut(&bus) {
            p.position = pos.max(0) as u64;
            p.position_at = tannoy_core::now();
            refresh(&d, &mut st);
        }
    }
    Err(anyhow!("the session bus closed"))
}

/// swaync's do not disturb, from its Subscribe signals.
fn watch_dnd(d: Shared) -> Result<()> {
    let r = MatchRule::builder()
        .msg_type(MsgType::Signal)
        .interface(dbus::SWAYNC)?
        .path(dbus::SWAYNC_PATH)?
        .build();
    for msg in MessageIterator::for_match_rule(r, &d.conn, Some(64))? {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let member = header.member().map(|m| m.to_string()).unwrap_or_default();
        // SubscribeV2 is (count, dnd, cc_open, inhibited); Subscribe drops
        // the last.
        let args = match member.as_str() {
            "SubscribeV2" => msg
                .body()
                .deserialize::<(u32, bool, bool, bool)>()
                .ok()
                .map(|(n, d, c, i)| vec![n.into(), d.into(), c.into(), i.into()]),
            "Subscribe" => msg
                .body()
                .deserialize::<(u32, bool, bool)>()
                .ok()
                .map(|(n, d, c)| vec![n.into(), d.into(), c.into()]),
            _ => None,
        };
        let dnd = args.and_then(|a| tannoy_core::swaync::dnd_from_signal(&member, &a));
        if let Some(dnd) = dnd {
            let mut st = d.lock();
            if st.dnd != dnd {
                st.dnd = dnd;
                refresh(&d, &mut st);
            }
        }
    }
    Err(anyhow!("the session bus closed"))
}

/// A screen share started or ended: titles hide while one runs, as they
/// do for do not disturb. Tannoy reads the share and changes nothing
/// outside itself.
fn screen_shared(d: &Shared, on: bool) {
    let mut st = d.lock();
    st.sharing = on;
    refresh(d, &mut st);
}

/// Screen shares from Hyprland's event socket. Ends when Tannoy is not
/// running under Hyprland or Hyprland goes away.
fn watch_screencast(d: Shared) {
    while let Some(path) = tannoy_core::screencast::socket() {
        if let Ok(s) = std::os::unix::net::UnixStream::connect(&path) {
            let mut shares = tannoy_core::screencast::Shares::default();
            for line in BufReader::new(s).lines() {
                let Ok(line) = line else { break };
                if let Some(on) = shares.apply(&line) {
                    screen_shared(&d, on);
                }
            }
            if shares.sharing() {
                screen_shared(&d, false);
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Keeps a watcher alive: if its stream ends, it starts again.
fn keep(d: &Shared, name: &'static str, f: fn(Shared) -> Result<()>) {
    let d = d.clone();
    std::thread::spawn(move || loop {
        if let Err(e) = f(d.clone()) {
            eprintln!("tannoy: {name} watcher: {e:#}");
        }
        std::thread::sleep(Duration::from_secs(2));
    });
}

/// Reads the position while the drawer is open and picks up config changes.
fn ticker(d: Shared) {
    let mut config_at = Instant::now();
    loop {
        let mut st = d.lock();
        let wait = if st.visible {
            POSITION_EVERY
        } else {
            CONFIG_EVERY
        };
        st =
            d.cv.wait_timeout(st, wait)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| e.into_inner().0);
        if config_at.elapsed() >= CONFIG_EVERY {
            config_at = Instant::now();
            let m = config_mtime();
            if m != st.cfg_mtime {
                st.cfg_mtime = m;
                let (cfg, err) = Config::load();
                st.cfg = cfg;
                st.cfg_error = err;
                let ignored: Vec<String> = st
                    .players
                    .keys()
                    .filter(|b| st.cfg.ignored(mpris::suffix(b)))
                    .cloned()
                    .collect();
                for b in ignored {
                    st.players.remove(&b);
                    st.owners.retain(|_, v| *v != b);
                }
                refresh(&d, &mut st);
                let d2 = d.clone();
                std::thread::spawn(move || rescan(&d2));
            }
        }
        if st.visible && !st.position_busy {
            if let Some(bus) = st.snap.active.clone() {
                st.position_busy = true;
                let d = d.clone();
                std::thread::spawn(move || {
                    let pos = dbus::position(&d.conn, &bus);
                    let mut st = d.lock();
                    st.position_busy = false;
                    if let (Ok(pos), Some(p)) = (pos, st.players.get_mut(&bus)) {
                        p.position = pos.max(0) as u64;
                        p.position_at = tannoy_core::now();
                        refresh(&d, &mut st);
                    }
                });
            }
        }
    }
}

/// Loads every player on the bus that is not tracked yet.
fn rescan(d: &Shared) -> Vec<std::thread::JoinHandle<()>> {
    let names = dbus::list_players(&d.conn).unwrap_or_default();
    let known: Vec<String> = d.lock().players.keys().cloned().collect();
    names
        .into_iter()
        .filter(|n| !known.contains(n))
        .map(|n| load(d.clone(), n))
        .collect()
}

/// Starts `tannoy-board`, preferring the one installed next to `tannoy`.
fn spawn_board() {
    let exe = std::env::current_exe().ok();
    let local = exe
        .as_ref()
        .and_then(|e| e.parent())
        .map(|dir| dir.join("tannoy-board"))
        .filter(|p| p.exists());
    let bin = local
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "tannoy-board".into());
    let log = paths::board_log();
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut cmd = std::process::Command::new(&bin);
    cmd.stdin(std::process::Stdio::null());
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    {
        if let Ok(f2) = f.try_clone() {
            cmd.stdout(f).stderr(f2);
        }
    }
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("tannoy: cannot start {}: {e}", bin.to_string_lossy()),
    }
}

/// Delivers a show or toggle to the drawer, starting it when none is
/// connected.
fn to_board(st: &mut State, ev: Event) {
    if broadcast(&mut st.boards, &event_line(&ev)) > 0 {
        return;
    }
    let show = match ev {
        Event::Toggle { anchor } | Event::Show { anchor } => Event::Show { anchor },
        other => other,
    };
    if matches!(show, Event::Hide) {
        return;
    }
    st.pending = Some(show);
    if st
        .board_started
        .is_none_or(|t| t.elapsed() > Duration::from_secs(5))
    {
        st.board_started = Some(Instant::now());
        spawn_board();
    }
}

/// A usable `mpris:trackid` for SetPosition.
fn valid_track_id(id: &str) -> bool {
    id.starts_with('/') && id != "/org/mpris/MediaPlayer2/TrackList/NoTrack"
}

/// The active player, once the first scan is done.
fn active(d: &Shared) -> Result<Player> {
    let mut st = d.lock();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !st.ready {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        st =
            d.cv.wait_timeout(st, left)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| e.into_inner().0);
    }
    st.snap
        .active_player()
        .cloned()
        .ok_or_else(|| anyhow!("no player is running"))
}

/// Moves to `target` microseconds, clamped to the track.
fn seek_to(d: &Shared, p: &Player, current: u64, target: i64) -> Result<()> {
    if !p.can_seek {
        return Err(anyhow!("{} cannot seek this track", p.name));
    }
    let mut target = target.max(0);
    if let Some(len) = p.track.length {
        // Seeking past the end skips the track; stop just short of it.
        target = target.min(len.saturating_sub(1_000_000) as i64);
    }
    if valid_track_id(&p.track.id) {
        dbus::set_position(&d.conn, &p.bus, &p.track.id, target)?;
    } else {
        dbus::seek(&d.conn, &p.bus, target - current as i64)?;
    }
    let mut st = d.lock();
    if let Some(q) = st.players.get_mut(&p.bus) {
        q.position = target as u64;
        q.position_at = tannoy_core::now();
    }
    refresh(d, &mut st);
    Ok(())
}

fn control(d: &Shared, req: Request) -> Result<()> {
    let p = active(d)?;
    match req {
        Request::PlayPause => dbus::player_call(&d.conn, &p.bus, "PlayPause"),
        Request::Next if !p.can_go_next => Err(anyhow!("{} has no next track", p.name)),
        Request::Next => dbus::player_call(&d.conn, &p.bus, "Next"),
        Request::Previous if !p.can_go_previous => Err(anyhow!("{} has no previous track", p.name)),
        Request::Previous => dbus::player_call(&d.conn, &p.bus, "Previous"),
        Request::Seek { offset } => {
            // Read the position fresh: it is only polled while the drawer
            // is open.
            let now = dbus::position(&d.conn, &p.bus)
                .map(|x| x.max(0) as u64)
                .unwrap_or_else(|_| p.position_now(tannoy_core::now()));
            seek_to(d, &p, now, now as i64 + (offset * 1e6) as i64)
        }
        Request::SetPosition { position } => {
            let now = p.position_now(tannoy_core::now());
            seek_to(d, &p, now, (position * 1e6) as i64)
        }
        _ => Ok(()),
    }
}

fn handle(d: Shared, stream: Stream) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    let reader = BufReader::new(read_half);
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = send_line(
                    &mut writer,
                    &serde_json::to_string(&Reply::err(format!("bad request: {e}")))
                        .unwrap_or_default(),
                );
                continue;
            }
        };
        let reply = match req {
            Request::Ping => Reply {
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Reply::ok()
            },
            Request::Snapshot => Reply {
                snapshot: Some(Box::new(d.lock().snap.clone())),
                ..Reply::ok()
            },
            Request::Art => Reply {
                art: d.lock().art_line.clone(),
                ..Reply::ok()
            },
            Request::Subscribe { kind } => {
                let _ = writer.set_write_timeout(Some(Duration::from_secs(2)));
                let mut st = d.lock();
                match kind {
                    SubKind::Board => {
                        let mut ok = send_line(
                            &mut writer,
                            &event_line(&Event::Snapshot {
                                snapshot: Box::new(st.snap.clone()),
                            }),
                        );
                        if let Some(p) = st.pending.take() {
                            ok &= send_line(&mut writer, &event_line(&p));
                        }
                        if ok {
                            st.boards.push(writer);
                        }
                    }
                    SubKind::Bar => {
                        let line = st.bar_line.clone();
                        if line.is_empty() || send_line(&mut writer, &line) {
                            st.bars.push(writer);
                        }
                    }
                }
                return;
            }
            Request::Toggle { anchor } => {
                to_board(&mut d.lock(), Event::Toggle { anchor });
                Reply::ok()
            }
            Request::Show { anchor } => {
                to_board(&mut d.lock(), Event::Show { anchor });
                Reply::ok()
            }
            Request::Hide => {
                let mut st = d.lock();
                st.pending = None;
                broadcast(&mut st.boards, &event_line(&Event::Hide));
                Reply::ok()
            }
            Request::BoardVisible { visible } => {
                let mut st = d.lock();
                st.visible = visible;
                d.cv.notify_all();
                Reply::ok()
            }
            Request::Pick { player } => {
                let mut st = d.lock();
                let players: Vec<Player> = st.players.values().cloned().collect();
                match tannoy_core::pick::resolve(&players, player.as_deref().unwrap_or("auto")) {
                    Ok(key) => {
                        save_pick(key.as_deref());
                        st.picked = key;
                        refresh(&d, &mut st);
                        Reply::ok()
                    }
                    Err(e) => Reply::err(e),
                }
            }
            req => match control(&d, req) {
                Ok(()) => Reply::ok(),
                Err(e) => Reply::err(format!("{e:#}")),
            },
        };
        if !send_line(
            &mut writer,
            &serde_json::to_string(&reply).unwrap_or_default(),
        ) {
            return;
        }
    }
}

pub fn run() -> Result<i32> {
    let listener = Listener::bind()?;
    let conn = dbus::connect()?;
    let (cfg, cfg_error) = Config::load();
    if let Some(e) = &cfg_error {
        eprintln!("tannoy: {e}");
    }
    let dnd = dbus::swaync_dnd(&conn).unwrap_or(false);
    let d: Shared = Arc::new(Daemon {
        state: Mutex::new(State {
            cfg,
            cfg_error,
            cfg_mtime: config_mtime(),
            players: BTreeMap::new(),
            owners: HashMap::new(),
            picked: load_pick(),
            dnd,
            sharing: false,
            visible: false,
            ready: false,
            art_url: None,
            art_path: None,
            art_line: None,
            snap: Snapshot::default(),
            boards: vec![],
            bars: vec![],
            bar_line: String::new(),
            pending: None,
            board_started: None,
            position_busy: false,
        }),
        cv: Condvar::new(),
        conn,
    });
    // Watch first, then scan, so nothing that changes in between is lost.
    keep(&d, "player", watch_names);
    keep(&d, "properties", watch_properties);
    keep(&d, "seek", watch_seeked);
    keep(&d, "do not disturb", watch_dnd);
    {
        let d = d.clone();
        std::thread::spawn(move || watch_screencast(d));
    }
    {
        let d = d.clone();
        std::thread::spawn(move || {
            for h in rescan(&d) {
                let _ = h.join();
            }
            let mut st = d.lock();
            st.ready = true;
            refresh(&d, &mut st);
            d.cv.notify_all();
        });
    }
    {
        let d = d.clone();
        std::thread::spawn(move || ticker(d));
    }
    eprintln!("tannoy: listening on {}", paths::socket_path().display());
    loop {
        match listener.accept() {
            Ok(stream) => {
                let d = d.clone();
                std::thread::spawn(move || handle(d, stream));
            }
            Err(e) => {
                eprintln!("tannoy: accept failed: {e}");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_ids() {
        assert!(valid_track_id(
            "/org/chromium/MediaPlayer2/TrackList/Track62E6C8AAB7C2EEA1334E85716070335A"
        ));
        assert!(!valid_track_id("/org/mpris/MediaPlayer2/TrackList/NoTrack"));
        assert!(!valid_track_id(""));
    }
}
