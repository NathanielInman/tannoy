//! Screen shares, from Hyprland's event socket (`.socket2.sock`). Hyprland
//! writes `screencast>>STATE,KIND` when a client starts or stops copying a
//! monitor or a window, and newer versions follow each one with
//! `screencastv2>>STATE,KIND,TARGET`, which names the monitor or window. A
//! recording can send a pair per monitor, so shares are counted, not
//! toggled. A single screenshot sends nothing.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Hyprland's event socket, when Tannoy runs under Hyprland.
pub fn socket() -> Option<PathBuf> {
    bar_anchor::hypr::socket()
        .map(|p| p.with_file_name(".socket2.sock"))
        .filter(|p| p.exists())
}

/// The shares running now, fed one event line at a time.
#[derive(Debug, Default)]
pub struct Shares {
    /// `screencastv2` lines have been seen; the older lines are then
    /// ignored, since every share would otherwise count twice.
    v2: bool,
    /// `KIND,TARGET` of each share, from `screencastv2`.
    active: BTreeSet<String>,
    /// Starts minus stops, from `screencast` on a Hyprland without v2.
    count: u32,
}

impl Shares {
    pub fn sharing(&self) -> bool {
        if self.v2 {
            !self.active.is_empty()
        } else {
            self.count > 0
        }
    }

    /// Applies one line from the event socket. Returns whether the screen
    /// is shared when that changed, and `None` otherwise.
    pub fn apply(&mut self, line: &str) -> Option<bool> {
        let (event, data) = line.trim_end().split_once(">>")?;
        let mut fields = data.splitn(3, ',');
        let on = match fields.next()? {
            "1" => true,
            "0" => false,
            _ => return None,
        };
        let before = self.sharing();
        match event {
            "screencastv2" => {
                let kind = fields.next().unwrap_or_default();
                let target = fields.next().unwrap_or_default();
                let key = format!("{kind},{target}");
                self.v2 = true;
                if on {
                    self.active.insert(key);
                } else {
                    self.active.remove(&key);
                }
            }
            "screencast" if !self.v2 => {
                self.count = if on {
                    self.count + 1
                } else {
                    self.count.saturating_sub(1)
                };
            }
            _ => return None,
        }
        let after = self.sharing();
        (after != before).then_some(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &str) -> Vec<bool> {
        let mut s = Shares::default();
        lines.lines().filter_map(|l| s.apply(l)).collect()
    }

    #[test]
    fn recorded_recording() {
        // wf-recorder for 3 seconds on two monitors, then grim, as
        // .socket2.sock saw them: one share from the first start to the
        // last stop, and nothing for the screenshot.
        assert_eq!(
            run(include_str!("../fixtures/hyprland-screencast.txt")),
            vec![true, false]
        );
    }

    #[test]
    fn a_share_lasts_until_its_last_target_stops() {
        let mut s = Shares::default();
        assert_eq!(s.apply("screencast>>1,monitor"), Some(true));
        assert_eq!(s.apply("screencastv2>>1,monitor,DP-3"), None);
        assert_eq!(s.apply("screencastv2>>1,window,55d1a2c0"), None);
        assert_eq!(s.apply("screencast>>0,monitor"), None);
        assert_eq!(s.apply("screencastv2>>0,monitor,DP-3"), None);
        assert!(s.sharing());
        assert_eq!(s.apply("screencastv2>>0,window,55d1a2c0"), Some(false));
    }

    #[test]
    fn older_hyprland_counts_the_plain_event() {
        assert_eq!(
            run("screencast>>1,0\nscreencast>>1,1\nscreencast>>0,1\nscreencast>>0,0\n"),
            vec![true, false]
        );
    }

    #[test]
    fn other_lines_and_stray_stops() {
        assert_eq!(
            run(
                "workspace>>2\nactivewindow>>kitty,~\nscreencast>>0,monitor\nscreencastv2>>0,monitor,DP-2\nscreencast>>x\n"
            ),
            Vec::<bool>::new()
        );
    }
}
