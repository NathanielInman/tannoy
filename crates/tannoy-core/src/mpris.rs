//! MPRIS, as plain data. The daemon turns D-Bus values into [`MValue`]s and
//! these pure functions turn them into the model, so every rule here is
//! tested against property dumps recorded from real players with
//! `busctl --user --json=pretty call <player> /org/mpris/MediaPlayer2
//! org.freedesktop.DBus.Properties GetAll s <interface>`.

use crate::model::{Player, Status, Track};
use std::collections::BTreeMap;

pub const PREFIX: &str = "org.mpris.MediaPlayer2.";
pub const PATH: &str = "/org/mpris/MediaPlayer2";
pub const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
pub const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";

/// A D-Bus value reduced to what MPRIS uses.
#[derive(Debug, Clone, PartialEq)]
pub enum MValue {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Str(String),
    Strs(Vec<String>),
    Dict(Props),
    Other,
}

pub type Props = BTreeMap<String, MValue>;

impl From<bool> for MValue {
    fn from(b: bool) -> MValue {
        MValue::Bool(b)
    }
}

impl From<u32> for MValue {
    fn from(n: u32) -> MValue {
        MValue::UInt(n as u64)
    }
}

impl MValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A string list; a lone string (some players send `xesam:artist` as
    /// `s`) becomes a list of one.
    pub fn as_strs(&self) -> Vec<String> {
        match self {
            MValue::Strs(v) => v.clone(),
            MValue::Str(s) if !s.is_empty() => vec![s.clone()],
            _ => vec![],
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            MValue::Int(i) => Some(*i),
            MValue::UInt(u) => i64::try_from(*u).ok(),
            MValue::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            MValue::Float(f) => Some(*f),
            MValue::Int(i) => Some(*i as f64),
            MValue::UInt(u) => Some(*u as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            MValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// `org.mpris.MediaPlayer2.chromium.instance910` → `chromium.instance910`.
pub fn suffix(bus: &str) -> &str {
    bus.strip_prefix(PREFIX).unwrap_or(bus)
}

/// The stable part of a player's bus name: `chromium.instance910` →
/// `chromium`, `firefox.instance_1_23` → `firefox`, `spotify` → `spotify`.
pub fn player_key(suffix: &str) -> String {
    let s = suffix.strip_prefix(PREFIX).unwrap_or(suffix);
    let key = match s.find(".instance") {
        Some(i) => &s[..i],
        None => s,
    };
    key.to_ascii_lowercase()
}

pub fn is_player_bus(name: &str) -> bool {
    name.starts_with(PREFIX) && name.len() > PREFIX.len()
}

pub fn parse_status(s: &str) -> Status {
    match s {
        "Playing" => Status::Playing,
        "Paused" => Status::Paused,
        _ => Status::Stopped,
    }
}

/// Reads `art_url` loosely: an empty string means no art.
fn non_empty(v: Option<&MValue>) -> Option<String> {
    v.and_then(MValue::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

pub fn parse_metadata(m: &Props) -> Track {
    let s = |k: &str| {
        m.get(k)
            .and_then(MValue::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let list = |k: &str| m.get(k).map(MValue::as_strs).unwrap_or_default();
    Track {
        id: s("mpris:trackid"),
        title: s("xesam:title"),
        artists: list("xesam:artist"),
        album: s("xesam:album"),
        album_artists: list("xesam:albumArtist"),
        genres: list("xesam:genre"),
        length: m
            .get("mpris:length")
            .and_then(MValue::as_i64)
            .filter(|l| *l > 0)
            .map(|l| l as u64),
        art_url: non_empty(m.get("mpris:artUrl")),
        url: non_empty(m.get("xesam:url")),
    }
}

/// What a property change touched, so the daemon knows what to redo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changed {
    pub status: bool,
    pub track: bool,
    pub position: bool,
}

/// Applies `org.mpris.MediaPlayer2.Player` properties (a `GetAll` reply or
/// a `PropertiesChanged` body) to a player. `now` stamps the position and
/// the last time the player was seen playing.
pub fn apply_player(p: &mut Player, props: &Props, now: f64) -> Changed {
    let mut changed = Changed::default();
    if let Some(st) = props.get("PlaybackStatus").and_then(MValue::as_str) {
        let st = parse_status(st);
        if st != p.status {
            // Keep the extrapolated position across a pause or a resume.
            p.position = p.position_now(now);
            p.position_at = now;
            p.status = st;
            changed.status = true;
        }
        if st == Status::Playing {
            p.last_playing = now;
        }
    }
    if let Some(MValue::Dict(m)) = props.get("Metadata") {
        let t = parse_metadata(m);
        if t != p.track {
            if t.id != p.track.id || t.title != p.track.title {
                // A new track starts from its beginning until told otherwise.
                p.position = 0;
                p.position_at = now;
            }
            p.track = t;
            changed.track = true;
        }
    }
    if let Some(pos) = props.get("Position").and_then(MValue::as_i64) {
        p.position = pos.max(0) as u64;
        p.position_at = now;
        changed.position = true;
    }
    if let Some(r) = props.get("Rate").and_then(MValue::as_f64) {
        p.rate = r;
    }
    let flag = |k: &str, slot: &mut bool| {
        if let Some(b) = props.get(k).and_then(MValue::as_bool) {
            *slot = b;
        }
    };
    flag("CanControl", &mut p.can_control);
    flag("CanPlay", &mut p.can_play);
    flag("CanPause", &mut p.can_pause);
    flag("CanSeek", &mut p.can_seek);
    flag("CanGoNext", &mut p.can_go_next);
    flag("CanGoPrevious", &mut p.can_go_previous);
    changed
}

/// Applies `org.mpris.MediaPlayer2` properties: the player's name and its
/// desktop entry.
pub fn apply_root(p: &mut Player, props: &Props) {
    if let Some(id) = non_empty(props.get("Identity")) {
        p.name = id;
    }
    if let Some(e) = non_empty(props.get("DesktopEntry")) {
        p.desktop_entry = Some(e);
    }
    if let Some(b) = props.get("CanRaise").and_then(MValue::as_bool) {
        p.can_raise = b;
    }
}

/// Converts `busctl --json` output into [`MValue`]s, so recorded property
/// dumps can be fed to the parsers.
pub fn from_busctl(v: &serde_json::Value) -> MValue {
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let data = v.get("data").cloned().unwrap_or_default();
    from_busctl_typed(ty, &data)
}

fn from_busctl_typed(ty: &str, data: &serde_json::Value) -> MValue {
    use serde_json::Value as J;
    match ty {
        "b" => MValue::Bool(data.as_bool().unwrap_or_default()),
        "x" | "i" | "n" => MValue::Int(data.as_i64().unwrap_or_default()),
        "t" | "u" | "q" | "y" => MValue::UInt(data.as_u64().unwrap_or_default()),
        "d" => MValue::Float(data.as_f64().unwrap_or_default()),
        "s" | "o" | "g" => MValue::Str(data.as_str().unwrap_or_default().to_string()),
        "as" | "ao" => MValue::Strs(
            data.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "v" => from_busctl(data),
        "a{sv}" => {
            // A call reply wraps its out-arguments in a list.
            let obj = match data {
                J::Array(a) => a.first().cloned().unwrap_or_default(),
                other => other.clone(),
            };
            MValue::Dict(
                obj.as_object()
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), from_busctl(v))).collect())
                    .unwrap_or_default(),
            )
        }
        _ => MValue::Other,
    }
}

/// A recorded `GetAll` dump as properties.
pub fn props_from_busctl(json: &str) -> Props {
    match serde_json::from_str::<serde_json::Value>(json)
        .map(|v| from_busctl(&v))
        .unwrap_or(MValue::Other)
    {
        MValue::Dict(d) => d,
        _ => Props::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME_PLAYER: &str = include_str!("../fixtures/chrome-player.json");
    const CHROME_ROOT: &str = include_str!("../fixtures/chrome-root.json");
    const SPOTIFY_PLAYER: &str = include_str!("../fixtures/spotify-player.json");
    const FAKE_PLAYER: &str = include_str!("../fixtures/fake-player.json");

    #[test]
    fn keys() {
        assert_eq!(player_key("chromium.instance910"), "chromium");
        assert_eq!(player_key("firefox.instance_1_23"), "firefox");
        assert_eq!(player_key("org.mpris.MediaPlayer2.Spotify"), "spotify");
        assert_eq!(suffix("org.mpris.MediaPlayer2.vlc"), "vlc");
        assert!(is_player_bus("org.mpris.MediaPlayer2.vlc"));
        assert!(!is_player_bus("org.mpris.MediaPlayer2."));
        assert!(!is_player_bus("org.erikreider.swaync.cc"));
    }

    #[test]
    fn chrome_podcast_in_a_tab() {
        let props = props_from_busctl(CHROME_PLAYER);
        let mut p = Player::new("org.mpris.MediaPlayer2.chromium.instance910");
        let c = apply_player(&mut p, &props, 1000.0);
        assert!(c.track);
        assert_eq!(p.status, Status::Paused);
        assert_eq!(p.track.artist(), "StarTalk");
        assert_eq!(
            p.track.title,
            "Why We Haven\u{2019}t Reached Superintelligence, with David Krakauer"
        );
        assert_eq!(p.track.album, "");
        assert_eq!(p.track.length, Some(4_669_981_000));
        assert_eq!(
            p.track.art_url.as_deref(),
            Some("file:///tmp/.com.google.Chrome.i7T45d")
        );
        assert_eq!(p.position, 376_745_940);
        assert!(p.can_seek && !p.can_go_next && !p.can_go_previous);
        // Over an hour with no album: a podcast.
        assert!(p.track.is_podcast());
        assert_eq!(p.last_playing, 0.0);
        p.can_raise = true;
        apply_root(&mut p, &props_from_busctl(CHROME_ROOT));
        assert_eq!(p.name, "Chrome");
        assert_eq!(p.key, "chromium");
        // Chrome cannot bring its window forward over MPRIS.
        assert!(!p.can_raise);
    }

    #[test]
    fn spotify_episode() {
        let props = props_from_busctl(SPOTIFY_PLAYER);
        let mut p = Player::new("org.mpris.MediaPlayer2.spotify");
        apply_player(&mut p, &props, 50.0);
        assert_eq!(p.status, Status::Playing);
        assert_eq!(p.last_playing, 50.0);
        assert!(p.track.is_podcast());
        assert!(
            p.track
                .art_url
                .as_deref()
                .unwrap()
                .starts_with("https://i.scdn.co/image/")
        );
        assert_eq!(p.track.artist(), "Neil deGrasse Tyson");
    }

    #[test]
    fn fake_player_music() {
        let props = props_from_busctl(FAKE_PLAYER);
        let mut p = Player::new("org.mpris.MediaPlayer2.tannoyfake");
        apply_player(&mut p, &props, 5.0);
        assert!(!p.track.is_podcast());
        assert_eq!(p.track.artists.len(), 2);
        assert_eq!(p.track.artist(), "Daft Punk, Pharrell Williams");
        assert_eq!(p.track.genres, vec!["Electronic".to_string()]);
    }

    #[test]
    fn changes_are_partial() {
        let mut p = Player::new("org.mpris.MediaPlayer2.spotify");
        apply_player(&mut p, &props_from_busctl(SPOTIFY_PLAYER), 10.0);
        let mut ch = Props::new();
        ch.insert("PlaybackStatus".into(), MValue::Str("Paused".into()));
        let c = apply_player(&mut p, &ch, 12.0);
        assert!(c.status && !c.track);
        assert_eq!(p.status, Status::Paused);
        // The position carried on while it played, then stopped.
        assert_eq!(p.position, 1_234_000_000 + 2_000_000);
        assert_eq!(p.track.artist(), "Neil deGrasse Tyson");
        assert_eq!(p.last_playing, 10.0);
    }

    #[test]
    fn lone_string_artist_and_unsigned_length() {
        let mut m = Props::new();
        m.insert("xesam:artist".into(), MValue::Str("Solo".into()));
        m.insert("mpris:length".into(), MValue::UInt(5_000_000));
        m.insert("mpris:artUrl".into(), MValue::Str(" ".into()));
        let t = parse_metadata(&m);
        assert_eq!(t.artists, vec!["Solo".to_string()]);
        assert_eq!(t.length, Some(5_000_000));
        assert_eq!(t.art_url, None);
    }
}
