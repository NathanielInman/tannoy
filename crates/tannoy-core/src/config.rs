//! `~/.config/tannoy/config.toml`. Every key is optional; an empty or
//! missing file gives a working setup.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub bar: BarConfig,
    pub dnd: DndConfig,
    pub art: ArtConfig,
    pub players: PlayersConfig,
    pub drawer: DrawerConfig,
    pub output: OutputConfig,
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct BarConfig {
    /// Longest bar text in characters (artist, separator and title). Longer
    /// text ends at a word boundary with a `…`.
    pub max_chars: usize,
    /// Between the artist and the title.
    pub separator: String,
    /// Glyph before the text while playing; empty for none.
    pub icon_playing: String,
    /// Glyph before the text while paused; empty for none.
    pub icon_paused: String,
    /// What the bar shows instead of the title while do not disturb hides it.
    pub icon_hidden: String,
    /// The layer-shell namespace of the bar surface, used to find its edge.
    pub namespace: String,
}

impl Default for BarConfig {
    fn default() -> Self {
        BarConfig {
            max_chars: 40,
            separator: " \u{b7} ".into(),
            icon_playing: String::new(),
            icon_paused: String::new(),
            icon_hidden: "\u{f075a}".into(),
            namespace: "waybar".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DndConfig {
    /// While swaync's do not disturb is on, keep the title, artist and cover
    /// off the bar (and out of the drawer), so they stay off screen shares.
    pub hide: bool,
    /// Text after `bar.icon_hidden` while hidden.
    pub text: String,
    /// Hide the title and cover while the screen is shared (seen through
    /// Hyprland), as do not disturb does.
    pub screen_share: bool,
    /// The mako mode that means do not disturb (mako has no switch of its
    /// own; users define a mode in its config).
    pub mako_mode: String,
}

impl Default for DndConfig {
    fn default() -> Self {
        DndConfig {
            hide: true,
            text: String::new(),
            screen_share: true,
            mako_mode: "do-not-disturb".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ArtConfig {
    /// Fetch and cache cover art for `tannoy art` and the drawer.
    pub enabled: bool,
    /// Waybar's image module redraws on `SIGRTMIN+signal`, which the daemon
    /// sends when the art changes. 0 sends nothing.
    pub signal: u8,
    /// When the track has no art: `none` hides the image module, `player`
    /// shows the player's app icon, anything else is an image path.
    pub fallback: String,
    /// Seconds an https fetch may take.
    pub timeout: u64,
}

impl Default for ArtConfig {
    fn default() -> Self {
        ArtConfig {
            enabled: true,
            signal: 12,
            fallback: "none".into(),
            timeout: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PlayersConfig {
    /// Players never tracked, by the part of the bus name after
    /// `org.mpris.MediaPlayer2.` (`firefox`, `chromium`). `playerctld` is
    /// always ignored: it mirrors another player.
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DrawerConfig {
    /// Panel width in logical pixels.
    pub width: f64,
    /// Largest height as a fraction of the monitor.
    pub max_height: f64,
    /// Gap between the bar and the panel.
    pub gap: f64,
    /// Seconds the podcast skip buttons jump.
    pub skip: u32,
    /// When the skip buttons show: `auto` (the track looks like a podcast),
    /// `always` or `never`.
    pub skips: String,
}

impl Default for DrawerConfig {
    fn default() -> Self {
        DrawerConfig {
            width: 400.0,
            max_height: 0.85,
            gap: 8.0,
            skip: 15,
            skips: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct OutputConfig {
    /// Run by the drawer's output button (through `sh -c`). Empty hides the
    /// button.
    pub command: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig {
            command: "~/.config/waybar/scripts/audio-switch.sh output".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    /// `waybar` reads the bar's style.css for font and colors; `none` uses
    /// the built-in theme.
    pub from: String,
    /// Overrides the accent color (scrub bar, picked player).
    pub accent: Option<String>,
    /// Overrides the font family.
    pub font: Option<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            from: "waybar".into(),
            accent: None,
            font: None,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str(text).map_err(|e| e.to_string().trim().replace('\n', " "))
    }

    /// The config and, when the file is broken, why (the defaults are used).
    pub fn load() -> (Config, Option<String>) {
        let path = crate::paths::config_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => match Config::parse(&text) {
                Ok(c) => (c, None),
                Err(e) => (Config::default(), Some(format!("{}: {e}", path.display()))),
            },
            Err(_) => (Config::default(), None),
        }
    }

    /// Whether a bus-name suffix (`chromium.instance910`) is ignored.
    pub fn ignored(&self, suffix: &str) -> bool {
        let key = crate::mpris::player_key(suffix);
        key == "playerctld"
            || self
                .players
                .ignore
                .iter()
                .any(|i| i.eq_ignore_ascii_case(&key) || i.eq_ignore_ascii_case(suffix))
    }

    /// The output command with a leading `~/` expanded.
    pub fn output_command(&self) -> Option<String> {
        let c = self.output.command.trim();
        if c.is_empty() {
            return None;
        }
        Some(match c.strip_prefix("~/") {
            Some(rest) => format!("{}/{rest}", crate::paths::home().display()),
            None => c.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_default() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn partial_tables() {
        let c = Config::parse("[bar]\nmax_chars = 30\n[art]\nsignal = 9\n").unwrap();
        assert_eq!(c.bar.max_chars, 30);
        assert_eq!(c.bar.separator, " \u{b7} ");
        assert_eq!(c.art.signal, 9);
        assert_eq!(c.art.fallback, "none");
        assert!(c.dnd.hide);
        assert!(c.dnd.screen_share);
    }

    #[test]
    fn the_sample_config_is_the_defaults() {
        let sample = include_str!("../../../packaging/examples/config.toml");
        assert_eq!(Config::parse(sample), Ok(Config::default()));
    }

    #[test]
    fn the_readme_config_is_the_defaults() {
        let readme = include_str!("../../../README.md");
        let config = &readme[readme.find("## Config").unwrap()..];
        let start = config.find("```toml\n").unwrap() + "```toml\n".len();
        let block = &config[start..start + config[start..].find("```").unwrap()];
        assert_eq!(Config::parse(block), Ok(Config::default()));
    }

    #[test]
    fn typos_are_errors() {
        assert!(Config::parse("[bar]\nmax_char = 3\n").is_err());
    }

    #[test]
    fn playerctld_is_always_ignored() {
        let mut c = Config::default();
        assert!(c.ignored("playerctld"));
        assert!(!c.ignored("chromium.instance910"));
        c.players.ignore = vec!["Chromium".into()];
        assert!(c.ignored("chromium.instance910"));
        assert!(!c.ignored("spotify"));
    }

    #[test]
    fn output_command_expands_home() {
        let c = Config::default();
        let cmd = c.output_command().unwrap();
        assert!(cmd.ends_with("/.config/waybar/scripts/audio-switch.sh output"));
        assert!(!cmd.starts_with('~'));
        let mut c = Config::default();
        c.output.command = " ".into();
        assert!(c.output_command().is_none());
    }
}
