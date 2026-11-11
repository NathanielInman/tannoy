//! What the daemon knows and how it changes, without D-Bus, threads, sockets
//! or the clock. The daemon tells the [`Model`] what happened (a player
//! appeared, a property changed, the config was saved) and carries out the
//! [`Effect`]s that [`Model::refresh`] returns.

use crate::config::Config;
use crate::model::{Player, Snapshot};
use crate::mpris::{self, Props};
use std::collections::{BTreeMap, HashMap};

/// Something the daemon has to do after a refresh.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// The active cover is not cached: copy or download it, then report
    /// back with [`Model::art_fetched`].
    FetchArt(String),
    /// A new line for the bar subscribers.
    Bar(String),
    /// What `tannoy art` prints changed: signal waybar's image module.
    SignalWaybar,
}

#[derive(Debug, Default)]
pub struct Model {
    pub cfg: Config,
    pub cfg_error: Option<String>,
    /// By well-known bus name.
    pub players: BTreeMap<String, Player>,
    /// Unique name (`:1.39`) to well-known name. Signals carry the former.
    owners: HashMap<String, String>,
    pub picked: Option<String>,
    pub dnd: bool,
    pub sharing: bool,
    /// The art URL of the active track and its cached copy.
    art_url: Option<String>,
    art_path: Option<String>,
    /// What `tannoy art` prints.
    art_line: Option<String>,
    bar_line: String,
    /// App icons for `art.fallback = "player"`, by player key. Finding one
    /// searches the icon theme on disk, so it happens once per player.
    icons: HashMap<String, Option<String>>,
    /// The last snapshot made by [`Model::refresh`].
    pub snap: Snapshot,
}

impl Model {
    pub fn new(cfg: Config, cfg_error: Option<String>, picked: Option<String>, dnd: bool) -> Model {
        Model {
            cfg,
            cfg_error,
            picked,
            dnd,
            ..Default::default()
        }
    }

    /// The bar line as last sent (empty before the first refresh).
    pub fn bar_line(&self) -> &str {
        &self.bar_line
    }

    /// What `tannoy art` prints now.
    pub fn art_line(&self) -> Option<&str> {
        self.art_line.as_deref()
    }

    /// A player's name gained an owner: map its unique name and start
    /// tracking it. False when the config ignores it.
    pub fn track(&mut self, bus: &str, owner: &str) -> bool {
        if self.cfg.ignored(mpris::suffix(bus)) {
            return false;
        }
        self.owners.insert(owner.to_string(), bus.to_string());
        self.players
            .entry(bus.to_string())
            .or_insert_with(|| Player::new(bus));
        true
    }

    /// Both interfaces were read (`GetAll`) for a tracked player.
    pub fn loaded(&mut self, bus: &str, player: Option<&Props>, root: Option<&Props>, now: f64) {
        if let Some(p) = self.players.get_mut(bus) {
            if let Some(props) = player {
                mpris::apply_player(p, props, now);
            }
            if let Some(props) = root {
                mpris::apply_root(p, props);
            }
        }
    }

    /// A player's name lost its owner. True when it was tracked.
    pub fn forget(&mut self, bus: &str) -> bool {
        self.owners.retain(|_, b| b != bus);
        self.players.remove(bus).is_some()
    }

    /// PropertiesChanged from `sender` on one of the MPRIS interfaces.
    /// Returns the player's bus name when it was applied.
    pub fn properties(
        &mut self,
        sender: &str,
        iface: &str,
        props: &Props,
        now: f64,
    ) -> Option<String> {
        let bus = self.owners.get(sender)?.clone();
        let p = self.players.get_mut(&bus)?;
        match iface {
            mpris::PLAYER_IFACE => {
                mpris::apply_player(p, props, now);
            }
            mpris::ROOT_IFACE => mpris::apply_root(p, props),
            _ => return None,
        }
        Some(bus)
    }

    /// `Seeked` from `sender`. True when it was a tracked player.
    pub fn seeked(&mut self, sender: &str, pos: i64, now: f64) -> bool {
        match self.owners.get(sender).cloned() {
            Some(bus) => self.position(&bus, pos, now),
            None => false,
        }
    }

    /// A position read from the player, or the target of our own seek.
    pub fn position(&mut self, bus: &str, pos: i64, now: f64) -> bool {
        let Some(p) = self.players.get_mut(bus) else {
            return false;
        };
        p.position = pos.max(0) as u64;
        p.position_at = now;
        true
    }

    /// True when it changed.
    pub fn set_dnd(&mut self, dnd: bool) -> bool {
        std::mem::replace(&mut self.dnd, dnd) != dnd
    }

    /// True when it changed.
    pub fn set_sharing(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.sharing, on) != on
    }

    /// `tannoy pick <query>`: the key picked (`None` for auto), or why
    /// nothing matched. The caller saves it.
    pub fn pick(&mut self, query: Option<&str>) -> Result<Option<String>, String> {
        let players: Vec<Player> = self.players.values().cloned().collect();
        let key = crate::pick::resolve(&players, query.unwrap_or("auto"))?;
        self.picked = key.clone();
        Ok(key)
    }

    /// A reread config. False when it parsed to the same thing. Players the
    /// new config ignores are dropped.
    pub fn reload(&mut self, cfg: Config, cfg_error: Option<String>) -> bool {
        if cfg == self.cfg && cfg_error == self.cfg_error {
            return false;
        }
        self.cfg = cfg;
        self.cfg_error = cfg_error;
        self.icons.clear();
        let ignored: Vec<String> = self
            .players
            .keys()
            .filter(|b| self.cfg.ignored(mpris::suffix(b)))
            .cloned()
            .collect();
        for b in ignored {
            self.forget(&b);
        }
        true
    }

    /// A cover finished fetching. False when the active track moved on in
    /// the meantime (the result is dropped).
    pub fn art_fetched(&mut self, url: &str, path: String) -> bool {
        if self.art_url.as_deref() != Some(url) {
            return false;
        }
        self.art_path = Some(path);
        true
    }

    /// Picks the active player, works out the cover and the hiding, makes
    /// the snapshot, and says what changed. `cached` finds a cover already
    /// on disk.
    pub fn refresh(&mut self, now: f64, cached: impl Fn(&str) -> Option<String>) -> Vec<Effect> {
        let mut effects = vec![];
        let players: Vec<Player> = self.players.values().cloned().collect();
        let active =
            crate::pick::choose(&players, self.picked.as_deref(), &self.cfg.players.prefer)
                .map(|i| players[i].bus.clone());

        let want = active
            .as_ref()
            .and_then(|b| self.players.get(b))
            .and_then(|p| p.track.art_url.clone())
            .filter(|_| self.cfg.art.enabled);
        if want != self.art_url {
            self.art_url = want.clone();
            self.art_path = None;
            if let Some(url) = want {
                match cached(&url) {
                    Some(p) => self.art_path = Some(p),
                    None => effects.push(Effect::FetchArt(url)),
                }
            }
        }

        let cfg = &self.cfg;
        let snap = Snapshot {
            version: env!("CARGO_PKG_VERSION").into(),
            now,
            players,
            active,
            picked: self.picked.clone(),
            dnd: self.dnd,
            hidden: (self.dnd && cfg.dnd.hide) || (self.sharing && cfg.dnd.screen_share),
            sharing: self.sharing,
            art: self.art_path.clone(),
            config_error: self.cfg_error.clone(),
        };
        let icons = &mut self.icons;
        let art_line = crate::bar::art_path_with(&snap, cfg, |p| {
            icons
                .entry(p.key.clone())
                .or_insert_with(|| crate::bar::player_icon(p))
                .clone()
        });
        let line = crate::bar::line_with(&snap, cfg, art_line.is_some());
        let bar = serde_json::to_string(&line).unwrap_or_default();
        if bar != self.bar_line {
            self.bar_line = bar.clone();
            effects.push(Effect::Bar(bar));
        }
        if art_line != self.art_line {
            self.art_line = art_line;
            effects.push(Effect::SignalWaybar);
        }
        self.snap = snap;
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::mpris::props_from_busctl;

    const CHROME: &str = "org.mpris.MediaPlayer2.chromium.instance910";
    const FAKE: &str = "org.mpris.MediaPlayer2.tannoyfake";

    fn chrome_props() -> Props {
        props_from_busctl(include_str!("../fixtures/chrome-player.json"))
    }

    fn fake_props() -> Props {
        props_from_busctl(include_str!("../fixtures/fake-player.json"))
    }

    fn bars(effects: &[Effect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Bar(l) => serde_json::from_str::<serde_json::Value>(l)
                    .ok()
                    .and_then(|v| v["text"].as_str().map(String::from)),
                _ => None,
            })
            .collect()
    }

    /// Chrome loaded and refreshed once, with no cover cached.
    fn with_chrome() -> (Model, Vec<Effect>) {
        let mut m = Model::default();
        assert!(m.track(CHROME, ":1.39"));
        m.loaded(CHROME, Some(&chrome_props()), None, 10.0);
        let fx = m.refresh(10.0, |_| None);
        (m, fx)
    }

    #[test]
    fn a_new_player_fetches_its_cover_and_fills_the_bar() {
        let (m, fx) = with_chrome();
        assert!(matches!(&fx[0], Effect::FetchArt(u) if u.starts_with("file://")));
        assert_eq!(
            bars(&fx),
            vec!["StarTalk · Why We Haven\u{2019}t Reached\u{2026}"]
        );
        // No cover yet, so nothing for the image module.
        assert!(!fx.contains(&Effect::SignalWaybar));
        assert_eq!(m.snap.active.as_deref(), Some(CHROME));
    }

    #[test]
    fn an_unchanged_refresh_does_nothing() {
        let (mut m, _) = with_chrome();
        assert_eq!(m.refresh(11.0, |_| None), vec![]);
    }

    #[test]
    fn a_fetched_cover_signals_waybar_once() {
        let (mut m, fx) = with_chrome();
        let Effect::FetchArt(url) = &fx[0] else {
            panic!()
        };
        assert!(m.art_fetched(url, "/c/art/a.png".into()));
        let fx = m.refresh(11.0, |_| None);
        assert!(fx.contains(&Effect::SignalWaybar));
        assert_eq!(m.art_line(), Some("/c/art/a.png"));
        // The bar text is the same; only its class gains with-art.
        assert!(m.bar_line().contains("with-art"));
        assert!(!m.refresh(12.0, |_| None).contains(&Effect::SignalWaybar));
    }

    #[test]
    fn a_late_cover_for_an_old_track_is_dropped() {
        let (mut m, _) = with_chrome();
        assert!(!m.art_fetched("file:///old.png", "/c/art/old.png".into()));
        assert_eq!(m.art_line(), None);
    }

    #[test]
    fn a_cached_cover_needs_no_fetch() {
        let mut m = Model::default();
        m.track(CHROME, ":1.39");
        m.loaded(CHROME, Some(&chrome_props()), None, 10.0);
        let fx = m.refresh(10.0, |_| Some("/c/art/hit.png".into()));
        assert!(!fx.iter().any(|e| matches!(e, Effect::FetchArt(_))));
        assert!(fx.contains(&Effect::SignalWaybar));
        assert_eq!(m.snap.art.as_deref(), Some("/c/art/hit.png"));
    }

    #[test]
    fn signals_find_their_player_by_unique_name() {
        let (mut m, _) = with_chrome();
        let mut props = Props::new();
        props.insert(
            "PlaybackStatus".into(),
            mpris::MValue::Str("Playing".into()),
        );
        assert_eq!(
            m.properties(":1.40", mpris::PLAYER_IFACE, &props, 20.0),
            None
        );
        assert_eq!(
            m.properties(":1.39", mpris::PLAYER_IFACE, &props, 20.0)
                .as_deref(),
            Some(CHROME)
        );
        assert_eq!(m.players[CHROME].status, Status::Playing);
        assert!(m.seeked(":1.39", 5_000_000, 21.0));
        assert_eq!(m.players[CHROME].position, 5_000_000);
        assert!(!m.seeked(":1.40", 1, 21.0));
        assert!(m.forget(CHROME));
        assert!(
            !m.seeked(":1.39", 1, 22.0),
            "the owner goes with the player"
        );
    }

    #[test]
    fn do_not_disturb_and_a_share_hide_titles_and_art() {
        let (mut m, fx) = with_chrome();
        let Effect::FetchArt(url) = &fx[0] else {
            panic!()
        };
        m.art_fetched(url, "/c/art/a.png".into());
        m.refresh(11.0, |_| None);

        assert!(m.set_dnd(true));
        assert!(!m.set_dnd(true));
        let fx = m.refresh(12.0, |_| None);
        assert!(m.snap.hidden);
        assert!(fx.contains(&Effect::SignalWaybar), "the cover goes away");
        assert!(m.bar_line().contains("\"hidden\""));

        m.set_dnd(false);
        m.set_sharing(true);
        m.refresh(13.0, |_| None);
        assert!(m.snap.hidden && m.snap.sharing);

        let mut cfg = Config::default();
        cfg.dnd.screen_share = false;
        assert!(m.reload(cfg, None));
        m.refresh(14.0, |_| None);
        assert!(
            !m.snap.hidden,
            "dnd.screen_share = false lets a share show titles"
        );
        assert_eq!(m.art_line(), Some("/c/art/a.png"));
    }

    #[test]
    fn the_pick_and_the_latest_player() {
        let (mut m, _) = with_chrome();
        m.track(FAKE, ":1.50");
        m.loaded(FAKE, Some(&fake_props()), None, 30.0);
        m.refresh(30.0, |_| None);
        assert_eq!(
            m.snap.active.as_deref(),
            Some(CHROME),
            "both paused: bus order"
        );
        let mut playing = Props::new();
        playing.insert(
            "PlaybackStatus".into(),
            mpris::MValue::Str("Playing".into()),
        );
        m.properties(":1.50", mpris::PLAYER_IFACE, &playing, 30.5);
        m.refresh(30.5, |_| None);
        assert_eq!(m.snap.active.as_deref(), Some(FAKE), "playing beats paused");
        assert_eq!(m.pick(Some("chrom")), Ok(Some("chromium".into())));
        m.refresh(31.0, |_| None);
        assert_eq!(m.snap.active.as_deref(), Some(CHROME));
        assert!(m.pick(Some("vlc")).is_err());
        assert_eq!(
            m.picked.as_deref(),
            Some("chromium"),
            "a failed pick keeps the old one"
        );
        assert_eq!(m.pick(None), Ok(None));
    }

    #[test]
    fn a_reload_drops_ignored_players_and_skips_no_ops() {
        let (mut m, _) = with_chrome();
        assert!(!m.reload(Config::default(), None));
        let mut cfg = Config::default();
        cfg.players.ignore = vec!["chromium".into()];
        assert!(m.reload(cfg, None));
        assert!(m.players.is_empty());
        assert!(!m.seeked(":1.39", 1, 1.0));
        assert!(
            !m.track(CHROME, ":1.41"),
            "an ignored player is not tracked again"
        );
        let fx = m.refresh(15.0, |_| None);
        assert_eq!(bars(&fx), vec![""]);
        assert_eq!(m.snap.active, None);
    }

    #[test]
    fn the_player_icon_is_looked_up_once() {
        let (mut m, _) = with_chrome();
        let mut cfg = Config::default();
        cfg.art.enabled = true;
        cfg.art.fallback = "player".into();
        m.reload(cfg, None);
        m.refresh(1.0, |_| None);
        m.refresh(2.0, |_| None);
        assert_eq!(m.icons.len(), 1, "one entry for chromium, found or not");
        m.reload(Config::default(), None);
        assert!(m.icons.is_empty(), "a reload forgets the icons");
    }

    #[test]
    fn a_broken_config_reaches_the_snapshot() {
        let mut m = Model::default();
        assert!(m.reload(Config::default(), Some("config.toml: bad".into())));
        m.refresh(1.0, |_| None);
        assert_eq!(m.snap.config_error.as_deref(), Some("config.toml: bad"));
    }
}
