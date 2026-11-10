//! swaync's do not disturb. Its control center emits `SubscribeV2`
//! (count, dnd, cc_open, inhibited) on every change, and older versions
//! `Subscribe` (count, dnd, cc_open); `swaync-client -swb` follows the same
//! signals. Recorded with `busctl --user monitor --json=short`.

use crate::mpris::MValue;

/// Do not disturb from one of swaync's signals, or `None` for another
/// signal.
pub fn dnd_from_signal(member: &str, args: &[MValue]) -> Option<bool> {
    match member {
        "SubscribeV2" | "Subscribe" => args.get(1)?.as_bool(),
        _ => None,
    }
}

/// The arguments of a `busctl --json` signal payload such as
/// `{"type":"ubbb","data":[44,true,false,false]}`, for basic types.
pub fn args_from_busctl(payload: &serde_json::Value) -> Vec<MValue> {
    let ty = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let data = payload
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    ty.chars()
        .zip(data.iter())
        .map(|(t, v)| match t {
            'b' => MValue::Bool(v.as_bool().unwrap_or_default()),
            'u' | 't' | 'q' | 'y' => MValue::UInt(v.as_u64().unwrap_or_default()),
            'i' | 'x' | 'n' => MValue::Int(v.as_i64().unwrap_or_default()),
            's' | 'o' => MValue::Str(v.as_str().unwrap_or_default().to_string()),
            _ => MValue::Other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_toggle() {
        // `swaync-client -dn` then `-df`, as busctl saw them.
        let seen: Vec<Option<bool>> = include_str!("../fixtures/swaync-signals.jsonl")
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .map(|m| {
                let member = m["member"].as_str().unwrap_or_default().to_string();
                dnd_from_signal(&member, &args_from_busctl(&m["payload"]))
            })
            .collect();
        assert_eq!(seen, vec![Some(true), Some(false)]);
    }

    #[test]
    fn older_signal_and_others() {
        let args = [MValue::UInt(3), MValue::Bool(true), MValue::Bool(false)];
        assert_eq!(dnd_from_signal("Subscribe", &args), Some(true));
        assert_eq!(dnd_from_signal("Other", &args), None);
        assert_eq!(dnd_from_signal("SubscribeV2", &[]), None);
    }
}
