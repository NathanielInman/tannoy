//! The default output device, for the drawer's output row. The drawer reads
//! it with `wpctl` when it opens (never the daemon); these parse that
//! output.

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sink {
    /// `node.description`, else `node.nick`, else `node.name`.
    pub name: String,
    pub bluetooth: bool,
    /// 0.0 to 1.0 (or above, when boosted).
    pub volume: Option<f32>,
    pub muted: bool,
}

/// Reads `wpctl inspect @DEFAULT_AUDIO_SINK@`.
pub fn parse_inspect(text: &str) -> Sink {
    let mut desc = None;
    let mut nick = None;
    let mut name = None;
    let mut bluetooth = false;
    for line in text.lines() {
        let l = line.trim().trim_start_matches('*').trim();
        let Some((k, v)) = l.split_once(" = ") else {
            continue;
        };
        let v = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "node.description" => desc = Some(v),
            "node.nick" => nick = Some(v),
            "node.name" => name = Some(v),
            "device.api" => bluetooth |= v == "bluez5",
            _ => {}
        }
    }
    Sink {
        name: desc.or(nick).or(name).unwrap_or_default(),
        bluetooth,
        ..Default::default()
    }
}

/// Reads `wpctl get-volume`: `Volume: 0.35` or `Volume: 0.35 [MUTED]`.
pub fn parse_volume(text: &str) -> Option<(f32, bool)> {
    let rest = text.trim().strip_prefix("Volume:")?.trim();
    let mut it = rest.split_whitespace();
    let v: f32 = it.next()?.parse().ok()?;
    Some((v, rest.contains("[MUTED]")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_speaker() {
        let s = parse_inspect(include_str!("../fixtures/wpctl-inspect-bluetooth.txt"));
        assert_eq!(s.name, "Razer Leviathan");
        assert!(s.bluetooth);
        assert_eq!(
            parse_volume(include_str!("../fixtures/wpctl-volume.txt")),
            Some((0.35, false))
        );
    }

    #[test]
    fn wired_and_muted() {
        let s = parse_inspect(
            "id 52, type PipeWire:Interface:Node\n    device.api = \"alsa\"\n  * node.name = \"alsa_output.usb\"\n    node.nick = \"USB Audio\"\n",
        );
        assert_eq!(s.name, "USB Audio");
        assert!(!s.bluetooth);
        assert_eq!(parse_volume("Volume: 0.80 [MUTED]\n"), Some((0.8, true)));
        assert_eq!(parse_volume("nothing"), None);
    }
}
