//! Which player the bar shows and the commands go to.
//!
//! 1. The user's pick (`tannoy pick spotify`, or a chip in the drawer) wins
//!    while a player with that key exists, even when it is paused.
//! 2. Otherwise the player that most recently started playing.
//! 3. Otherwise the one that was playing most recently, then a paused one
//!    over a stopped one, then the first.
//!
//! The pick is stored by key (`chromium`, not `chromium.instance910`), so it
//! survives the player restarting under a new instance number.

use crate::model::{Player, Status};
use std::cmp::Ordering;

fn rank(p: &Player) -> (u8, f64) {
    let s = match p.status {
        Status::Playing => 2,
        Status::Paused => 1,
        Status::Stopped => 0,
    };
    (s, p.last_playing)
}

/// Best first: playing, then most recently playing, then paused.
fn better(a: &Player, b: &Player) -> Ordering {
    let (sa, la) = rank(a);
    let (sb, lb) = rank(b);
    if (sa == 2) != (sb == 2) {
        return if sa == 2 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    lb.partial_cmp(&la)
        .unwrap_or(Ordering::Equal)
        .then(sb.cmp(&sa))
}

/// Index of the active player.
pub fn choose(players: &[Player], picked: Option<&str>) -> Option<usize> {
    let mut idx: Vec<usize> = (0..players.len()).collect();
    idx.sort_by(|a, b| better(&players[*a], &players[*b]).then(a.cmp(b)));
    if let Some(k) = picked
        && let Some(i) = idx.iter().find(|i| matches_key(&players[**i], k))
    {
        return Some(*i);
    }
    idx.first().copied()
}

fn matches_key(p: &Player, key: &str) -> bool {
    p.key.eq_ignore_ascii_case(key)
}

/// Resolves what the user typed after `tannoy pick` to a player key:
/// a key, a bus-name suffix, or a name ("Chrome"), whole or as a prefix.
/// `auto` (or nothing) clears the pick.
pub fn resolve(players: &[Player], query: &str) -> Result<Option<String>, String> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() || q == "auto" || q == "none" {
        return Ok(None);
    }
    let q = crate::mpris::player_key(&q);
    let exact = players.iter().find(|p| {
        p.key == q
            || crate::mpris::suffix(&p.bus).eq_ignore_ascii_case(&q)
            || p.name.eq_ignore_ascii_case(&q)
    });
    let found = exact.or_else(|| {
        players
            .iter()
            .find(|p| p.key.starts_with(&q) || p.name.to_ascii_lowercase().starts_with(&q))
    });
    match found {
        Some(p) => Ok(Some(p.key.clone())),
        None => {
            let names: Vec<&str> = players.iter().map(|p| p.key.as_str()).collect();
            if names.is_empty() {
                Err(format!("no player matches `{query}` (none are running)"))
            } else {
                Err(format!(
                    "no player matches `{query}` (running: {})",
                    names.join(", ")
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(bus: &str, status: Status, last: f64) -> Player {
        let mut p = Player::new(bus);
        p.status = status;
        p.last_playing = last;
        p
    }

    fn players() -> Vec<Player> {
        vec![
            player(
                "org.mpris.MediaPlayer2.chromium.instance910",
                Status::Paused,
                50.0,
            ),
            player("org.mpris.MediaPlayer2.spotify", Status::Playing, 40.0),
            player("org.mpris.MediaPlayer2.mpv", Status::Stopped, 0.0),
        ]
    }

    #[test]
    fn playing_beats_recently_paused() {
        assert_eq!(choose(&players(), None), Some(1));
    }

    #[test]
    fn latest_to_start_playing_wins() {
        let mut ps = players();
        ps[0].status = Status::Playing;
        ps[0].last_playing = 60.0;
        ps[1].last_playing = 61.0;
        assert_eq!(choose(&ps, None), Some(1));
        ps[0].last_playing = 62.0;
        assert_eq!(choose(&ps, None), Some(0));
    }

    #[test]
    fn nothing_playing_takes_the_last_one_that_did() {
        let mut ps = players();
        ps[1].status = Status::Paused;
        assert_eq!(choose(&ps, None), Some(0));
        let never = vec![
            player("org.mpris.MediaPlayer2.a", Status::Stopped, 0.0),
            player("org.mpris.MediaPlayer2.b", Status::Paused, 0.0),
        ];
        assert_eq!(choose(&never, None), Some(1));
        assert_eq!(choose(&[], None), None);
    }

    #[test]
    fn the_pick_wins_while_it_exists() {
        let ps = players();
        assert_eq!(choose(&ps, Some("chromium")), Some(0));
        assert_eq!(choose(&ps, Some("mpv")), Some(2));
        // Gone: back to the rules.
        assert_eq!(choose(&ps, Some("vlc")), Some(1));
    }

    #[test]
    fn resolving_a_pick() {
        let mut ps = players();
        ps[0].name = "Chrome".into();
        assert_eq!(resolve(&ps, "chrome"), Ok(Some("chromium".into())));
        assert_eq!(
            resolve(&ps, "chromium.instance910"),
            Ok(Some("chromium".into()))
        );
        assert_eq!(resolve(&ps, "Spot"), Ok(Some("spotify".into())));
        assert_eq!(resolve(&ps, "auto"), Ok(None));
        assert!(
            resolve(&ps, "vlc")
                .unwrap_err()
                .contains("chromium, spotify, mpv")
        );
    }
}
