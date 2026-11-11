//! Do not disturb from dunst and mako (swaync has its own signals, in
//! `swaync.rs`). All three own `org.freedesktop.Notifications`, so only one
//! runs at a time, and each is told apart by its own interface. Tannoy only
//! reads these; it never sets them.
//!
//! - dunst: the `paused` property of `org.dunstproject.cmd0` on
//!   `/org/freedesktop/Notifications`, announced with PropertiesChanged.
//! - mako: do not disturb is a mode the user defines (usually
//!   `do-not-disturb`). The `Modes` property of `fr.emersion.Mako` on
//!   `/fr/emersion/Mako` is only invalidated when it changes, so it has to
//!   be read again.
//!
//! Recorded with `busctl --json=short monitor` and `get-property` against
//! dunst 1.13.2 and mako 1.11.0 on a private bus.

use crate::mpris::{from_busctl, MValue, Props};

pub const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
pub const DUNST_PATH: &str = "/org/freedesktop/Notifications";
pub const DUNST_IFACE: &str = "org.dunstproject.cmd0";
pub const MAKO_PATH: &str = "/fr/emersion/Mako";
pub const MAKO_IFACE: &str = "fr.emersion.Mako";

/// dunst's pause from a PropertiesChanged signal, or `None` for any other.
pub fn dunst_paused(iface: &str, changed: &Props) -> Option<bool> {
    if iface != DUNST_IFACE {
        return None;
    }
    changed.get("paused")?.as_bool()
}

/// The PropertiesChanged signal says mako's modes changed, so they must be
/// read again (mako sends the name, not the value).
pub fn mako_modes_changed(iface: &str, changed: &Props, invalidated: &[String]) -> bool {
    iface == MAKO_IFACE
        && (invalidated.iter().any(|p| p == "Modes") || changed.contains_key("Modes"))
}

/// Do not disturb under mako: `mode` (`dnd.mako_mode`) is one of the modes
/// that are on.
pub fn mako_dnd(modes: &MValue, mode: &str) -> bool {
    matches!(modes, MValue::Strs(m) if m.iter().any(|x| x == mode))
}

/// A PropertiesChanged payload as busctl prints it (`sa{sv}as`): the
/// interface, the changed values and the invalidated names.
pub fn properties_changed_from_busctl(
    payload: &serde_json::Value,
) -> Option<(String, Props, Vec<String>)> {
    let data = payload.get("data")?.as_array()?;
    let iface = data.first()?.as_str()?.to_string();
    let changed = data
        .get(1)?
        .as_object()?
        .iter()
        .map(|(k, v)| (k.clone(), from_busctl(v)))
        .collect();
    let invalidated = data
        .get(2)?
        .as_array()?
        .iter()
        .filter_map(|s| s.as_str().map(String::from))
        .collect();
    Some((iface, changed, invalidated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(jsonl: &str) -> Vec<(String, Props, Vec<String>)> {
        jsonl
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter_map(|m| properties_changed_from_busctl(&m["payload"]))
            .collect()
    }

    #[test]
    fn recorded_dunst_pause() {
        // `dunstctl set-paused true`, then `false`.
        let seen: Vec<Option<bool>> = signals(include_str!("../fixtures/dunst-signals.jsonl"))
            .iter()
            .map(|(i, c, _)| dunst_paused(i, c))
            .collect();
        assert_eq!(seen, vec![Some(true), Some(false)]);
        assert_eq!(
            dunst_paused("org.mpris.MediaPlayer2.Player", &Props::new()),
            None
        );
    }

    #[test]
    fn recorded_mako_modes() {
        // `makoctl mode -a do-not-disturb`, then `-r`: both only invalidate.
        let sigs = signals(include_str!("../fixtures/mako-signals.jsonl"));
        assert_eq!(sigs.len(), 2);
        assert!(sigs.iter().all(|(i, c, inv)| mako_modes_changed(i, c, inv)));
        assert!(!mako_modes_changed(
            DUNST_IFACE,
            &Props::new(),
            &["Modes".into()]
        ));

        let read = |json: &str| from_busctl(&serde_json::from_str(json).unwrap());
        let on = read(include_str!("../fixtures/mako-modes-dnd.json"));
        let off = read(include_str!("../fixtures/mako-modes-default.json"));
        assert!(mako_dnd(&on, "do-not-disturb"));
        assert!(!mako_dnd(&off, "do-not-disturb"));
        assert!(!mako_dnd(&on, "dnd"));
    }
}
