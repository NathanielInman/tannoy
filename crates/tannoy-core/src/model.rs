//! What the daemon knows: every MPRIS player, which one is active, whether
//! do not disturb is hiding titles, and where the active cover art is cached.
//! The snapshot is what the drawer receives and `tannoy status --json`
//! prints.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Playing => "playing",
            Status::Paused => "paused",
            Status::Stopped => "stopped",
        }
    }
}

/// One track's metadata (`xesam:*` and `mpris:*`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Track {
    /// `mpris:trackid`, needed for `SetPosition`.
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artists: Vec<String>,
    pub genres: Vec<String>,
    /// Microseconds.
    pub length: Option<u64>,
    pub art_url: Option<String>,
    pub url: Option<String>,
}

/// Longer than this with no album reads as a podcast or an episode.
pub const PODCAST_LENGTH_US: u64 = 20 * 60 * 1_000_000;

impl Track {
    pub fn artist(&self) -> String {
        let list = if self.artists.iter().any(|a| !a.trim().is_empty()) {
            &self.artists
        } else {
            &self.album_artists
        };
        list.iter()
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn is_empty(&self) -> bool {
        self.title.trim().is_empty() && self.artist().is_empty()
    }

    /// Spotify episodes (`/com/spotify/episode/...`), anything tagged as a
    /// podcast, and long tracks without an album (a podcast or a talk in a
    /// browser tab).
    pub fn is_podcast(&self) -> bool {
        let id = self.id.to_ascii_lowercase();
        if id.contains("episode") || id.contains("podcast") {
            return true;
        }
        if self
            .genres
            .iter()
            .any(|g| g.to_ascii_lowercase().contains("podcast"))
        {
            return true;
        }
        self.album.trim().is_empty() && self.length.is_some_and(|l| l > PODCAST_LENGTH_US)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Player {
    /// The well-known bus name, `org.mpris.MediaPlayer2.chromium.instance910`.
    pub bus: String,
    /// The bus name without the prefix and instance (`chromium`): what a
    /// pick remembers, since the instance changes with every restart.
    pub key: String,
    /// `Identity` ("Chrome", "Spotify"), else the key.
    pub name: String,
    pub desktop_entry: Option<String>,
    pub status: Status,
    pub track: Track,
    /// Microseconds, as of `position_at`.
    pub position: u64,
    /// Unix seconds when `position` was read.
    pub position_at: f64,
    pub rate: f64,
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    /// Unix seconds when this player was last seen playing (0: never).
    pub last_playing: f64,
}

impl Player {
    pub fn new(bus: &str) -> Player {
        let suffix = crate::mpris::suffix(bus);
        let key = crate::mpris::player_key(suffix);
        Player {
            bus: bus.to_string(),
            name: key.clone(),
            key,
            rate: 1.0,
            can_control: true,
            can_play: true,
            can_pause: true,
            ..Default::default()
        }
    }

    /// The position now, extrapolated from the last reading while playing.
    pub fn position_now(&self, now: f64) -> u64 {
        let mut p = self.position as f64;
        if self.status == Status::Playing && self.position_at > 0.0 {
            let rate = if self.rate > 0.0 { self.rate } else { 1.0 };
            p += (now - self.position_at).max(0.0) * 1e6 * rate;
        }
        let p = p.max(0.0) as u64;
        match self.track.length {
            Some(l) if l > 0 => p.min(l),
            _ => p,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Snapshot {
    pub version: String,
    /// Unix seconds when this was made.
    pub now: f64,
    pub players: Vec<Player>,
    /// Bus name of the player commands go to.
    pub active: Option<String>,
    /// The user's pick (a player key), kept even while that player is gone.
    pub picked: Option<String>,
    /// swaync's do not disturb.
    pub dnd: bool,
    /// Do not disturb is hiding titles (`dnd` and `dnd.hide`).
    pub hidden: bool,
    /// The active track's cover, cached on disk.
    pub art: Option<String>,
    pub config_error: Option<String>,
}

impl Snapshot {
    pub fn active_player(&self) -> Option<&Player> {
        let bus = self.active.as_deref()?;
        self.players.iter().find(|p| p.bus == bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn podcast_rules() {
        let mut t = Track {
            id: "/com/spotify/episode/4rOoJ6Egrf8K2IrywzwOMk".into(),
            album: "StarTalk Radio".into(),
            length: Some(60_000_000),
            ..Default::default()
        };
        assert!(t.is_podcast());
        t.id = "/com/spotify/track/4uLU6hMCjMI75M1A2tKUQC".into();
        assert!(!t.is_podcast());
        t.genres = vec!["Podcast".into()];
        assert!(t.is_podcast());
        t.genres.clear();
        t.album.clear();
        t.length = Some(77 * 60 * 1_000_000);
        assert!(t.is_podcast());
        t.length = Some(4 * 60 * 1_000_000);
        assert!(!t.is_podcast());
    }

    #[test]
    fn artist_falls_back_to_album_artist() {
        let t = Track {
            artists: vec!["".into()],
            album_artists: vec!["Daft Punk".into()],
            ..Default::default()
        };
        assert_eq!(t.artist(), "Daft Punk");
    }

    #[test]
    fn position_extrapolates_while_playing() {
        let mut p = Player::new("org.mpris.MediaPlayer2.spotify");
        p.position = 10_000_000;
        p.position_at = 100.0;
        p.track.length = Some(12_000_000);
        assert_eq!(p.position_now(101.0), 10_000_000);
        p.status = Status::Playing;
        assert_eq!(p.position_now(101.0), 11_000_000);
        assert_eq!(p.position_now(200.0), 12_000_000);
    }
}
