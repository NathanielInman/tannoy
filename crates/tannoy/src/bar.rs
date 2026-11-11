//! The waybar line: artist and title, cut at a word boundary, with a class
//! for the bar's CSS. While do not disturb hides titles, only a glyph.
//!
//! The classes are the state (`playing`, `paused`, `hidden`, `stopped`,
//! `offline`) and `with-art` while the image module beside it shows a cover:
//! waybar cannot style a group by what its modules show, so the text module
//! is a whole pill on its own and gives up its left end to the cover.

use tannoy_core::config::Config;
use tannoy_core::model::{Snapshot, Status};
use tannoy_core::protocol::BarLine;
use tannoy_core::text::{ellipsize, escape};

/// `artist · title`, or whichever of the two exists.
pub fn label(artist: &str, title: &str, sep: &str) -> String {
    match (artist.trim(), title.trim()) {
        ("", t) => t.to_string(),
        (a, "") => a.to_string(),
        (a, t) => format!("{a}{sep}{t}"),
    }
}

fn empty(class: &str, tooltip: &str) -> BarLine {
    BarLine {
        text: String::new(),
        class: vec![class.into()],
        alt: class.into(),
        tooltip: tooltip.into(),
    }
}

pub fn line(snap: &Snapshot, cfg: &Config) -> BarLine {
    let Some(p) = snap.active_player() else {
        return empty("stopped", "Nothing is playing");
    };
    if p.status == Status::Stopped || p.track.is_empty() {
        return empty("stopped", "Nothing is playing");
    }
    if snap.hidden {
        let mut text = cfg.bar.icon_hidden.clone();
        if !cfg.dnd.text.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&escape(&cfg.dnd.text));
        }
        return BarLine {
            text,
            class: vec!["hidden".into()],
            alt: "hidden".into(),
            tooltip: format!(
                "Hidden while {}\n{} is {}",
                snap.hidden_reason(cfg),
                escape(&p.name),
                p.status.as_str()
            ),
        };
    }
    let t = &p.track;
    let artist = t.artist();
    let body = ellipsize(
        &label(&artist, &t.title, &cfg.bar.separator),
        cfg.bar.max_chars,
    );
    let icon = match p.status {
        Status::Playing => &cfg.bar.icon_playing,
        _ => &cfg.bar.icon_paused,
    };
    let mut text = String::new();
    if !icon.is_empty() {
        text.push_str(icon);
        text.push(' ');
    }
    text.push_str(&escape(&body));

    let mut tip = vec![];
    if !t.title.is_empty() {
        tip.push(t.title.clone());
    }
    if !artist.is_empty() {
        tip.push(artist);
    }
    if !t.album.is_empty() {
        tip.push(t.album.clone());
    }
    let state = if p.status == Status::Paused {
        " (paused)"
    } else {
        ""
    };
    tip.push(format!("{}{state}", p.name));
    let class = p.status.as_str();
    let mut classes = vec![class.to_string()];
    if art_path(snap, cfg).is_some() {
        classes.push("with-art".into());
    }
    BarLine {
        text,
        class: classes,
        alt: class.into(),
        tooltip: escape(&tip.join("\n")),
    }
}

/// What the bar shows while the daemon cannot be reached: nothing.
pub fn offline() -> BarLine {
    empty("offline", "Tannoy daemon is not running")
}

/// What `tannoy art` prints: the cached cover, a fallback, or nothing
/// (waybar hides an image module whose path does not exist). Never a cover
/// while do not disturb hides titles.
pub fn art_path(snap: &Snapshot, cfg: &Config) -> Option<String> {
    if !cfg.art.enabled || snap.hidden {
        return None;
    }
    let p = snap.active_player()?;
    if p.status == Status::Stopped || p.track.is_empty() {
        return None;
    }
    if let Some(a) = &snap.art {
        return Some(a.clone());
    }
    match cfg.art.fallback.as_str() {
        "" | "none" => None,
        "player" => {
            let names = tannoy_core::art::icon_names(p.desktop_entry.as_deref(), &p.key, &p.name);
            tannoy_core::art::player_icon(&names).map(|p| p.display().to_string())
        }
        path => {
            let p = tannoy_core::paths::expand(path);
            p.exists().then(|| p.display().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tannoy_core::model::Player;
    use tannoy_core::mpris::{apply_player, apply_root, props_from_busctl};

    fn chrome() -> Snapshot {
        let mut p = Player::new("org.mpris.MediaPlayer2.chromium.instance910");
        apply_player(
            &mut p,
            &props_from_busctl(include_str!(
                "../../tannoy-core/fixtures/chrome-player.json"
            )),
            10.0,
        );
        apply_root(
            &mut p,
            &props_from_busctl(include_str!("../../tannoy-core/fixtures/chrome-root.json")),
        );
        Snapshot {
            active: Some(p.bus.clone()),
            players: vec![p],
            art: Some("/c/art/abc.png".into()),
            ..Default::default()
        }
    }

    #[test]
    fn paused_podcast() {
        let l = line(&chrome(), &Config::default());
        assert_eq!(l.text, "StarTalk · Why We Haven\u{2019}t Reached\u{2026}");
        assert_eq!(l.class, vec!["paused", "with-art"]);
        assert_eq!(
            l.tooltip,
            "Why We Haven\u{2019}t Reached Superintelligence, with David Krakauer\nStarTalk\nChrome (paused)"
        );
    }

    #[test]
    fn icons_and_markup() {
        let mut s = chrome();
        s.players[0].status = Status::Playing;
        s.players[0].track.title = "Tom & Jerry".into();
        let mut cfg = Config::default();
        cfg.bar.icon_playing = "\u{f04b}".into();
        let l = line(&s, &cfg);
        assert_eq!(l.text, "\u{f04b} StarTalk · Tom &amp; Jerry");
        assert_eq!(l.class[0], "playing");
    }

    #[test]
    fn do_not_disturb_hides_title_and_art() {
        let mut s = chrome();
        s.dnd = true;
        s.hidden = true;
        let cfg = Config::default();
        let l = line(&s, &cfg);
        assert_eq!(l.text, cfg.bar.icon_hidden);
        assert_eq!(l.class, vec!["hidden"]);
        assert!(!l.tooltip.contains("StarTalk"));
        assert!(!l.tooltip.contains("Superintelligence"));
        assert_eq!(art_path(&s, &cfg), None);
        assert!(l.tooltip.starts_with("Hidden while do not disturb is on"));
    }

    #[test]
    fn a_screen_share_hides_like_do_not_disturb() {
        let mut s = chrome();
        s.sharing = true;
        s.hidden = true;
        let cfg = Config::default();
        let l = line(&s, &cfg);
        assert_eq!(l.class, vec!["hidden"]);
        assert!(l.tooltip.starts_with("Hidden while the screen is shared"));
        assert!(!l.tooltip.contains("StarTalk"));
        assert_eq!(art_path(&s, &cfg), None);
    }

    #[test]
    fn nothing_playing_hides_the_module() {
        let l = line(&Snapshot::default(), &Config::default());
        assert_eq!(l.text, "");
        assert_eq!(l.class, vec!["stopped"]);
        let mut s = chrome();
        s.art = None;
        assert_eq!(line(&s, &Config::default()).class, vec!["paused"]);
        s.players[0].status = Status::Stopped;
        assert_eq!(line(&s, &Config::default()).text, "");
        assert_eq!(art_path(&s, &Config::default()), None);
    }

    #[test]
    fn art_and_fallbacks() {
        let s = chrome();
        let mut cfg = Config::default();
        assert_eq!(art_path(&s, &cfg).as_deref(), Some("/c/art/abc.png"));
        let mut s = s;
        s.art = None;
        assert_eq!(art_path(&s, &cfg), None);
        cfg.art.fallback = "/nonexistent.png".into();
        assert_eq!(art_path(&s, &cfg), None);
        cfg.art.enabled = false;
        s.art = Some("/c/art/abc.png".into());
        assert_eq!(art_path(&s, &cfg), None);
    }

    #[test]
    fn labels() {
        assert_eq!(label("", "Title", " · "), "Title");
        assert_eq!(label("Artist", " ", " · "), "Artist");
    }
}
