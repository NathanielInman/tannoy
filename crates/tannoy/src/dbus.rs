//! The session bus: MPRIS players and swaync's do not disturb. Everything
//! here is zbus's blocking API with a method timeout, so a player that hangs
//! cannot hold up anything but the helper thread that asked it.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::time::Duration;
use tannoy_core::mpris::{MValue, Props, PATH, PLAYER_IFACE};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

pub const SWAYNC: &str = "org.erikreider.swaync.cc";
pub const SWAYNC_PATH: &str = "/org/erikreider/swaync/cc";

/// A session-bus connection whose method calls give up after 3 seconds.
pub fn connect() -> Result<Connection> {
    Ok(zbus::blocking::connection::Builder::session()?
        .method_timeout(Duration::from_secs(3))
        .build()?)
}

/// A D-Bus value as the parsers' [`MValue`].
pub fn mvalue(v: &Value) -> MValue {
    match v {
        Value::Bool(b) => MValue::Bool(*b),
        Value::U8(x) => MValue::UInt(*x as u64),
        Value::U16(x) => MValue::UInt(*x as u64),
        Value::U32(x) => MValue::UInt(*x as u64),
        Value::U64(x) => MValue::UInt(*x),
        Value::I16(x) => MValue::Int(*x as i64),
        Value::I32(x) => MValue::Int(*x as i64),
        Value::I64(x) => MValue::Int(*x),
        Value::F64(x) => MValue::Float(*x),
        Value::Str(s) => MValue::Str(s.to_string()),
        Value::ObjectPath(p) => MValue::Str(p.to_string()),
        Value::Value(inner) => mvalue(inner),
        Value::Array(a) => MValue::Strs(
            a.inner()
                .iter()
                .filter_map(|x| match mvalue(x) {
                    MValue::Str(s) => Some(s),
                    _ => None,
                })
                .collect(),
        ),
        Value::Dict(d) => MValue::Dict(
            d.iter()
                .filter_map(|(k, v)| match k {
                    Value::Str(k) => Some((k.to_string(), mvalue(v))),
                    _ => None,
                })
                .collect(),
        ),
        _ => MValue::Other,
    }
}

pub fn props(map: &HashMap<String, OwnedValue>) -> Props {
    map.iter().map(|(k, v)| (k.clone(), mvalue(v))).collect()
}

pub fn get_all(conn: &Connection, bus: &str, iface: &str) -> Result<Props> {
    let reply = conn.call_method(
        Some(bus),
        PATH,
        Some("org.freedesktop.DBus.Properties"),
        "GetAll",
        &(iface,),
    )?;
    let map: HashMap<String, OwnedValue> = reply.body().deserialize()?;
    Ok(props(&map))
}

/// The player's position in microseconds. MPRIS never signals it, so the
/// daemon asks while the drawer is open.
pub fn position(conn: &Connection, bus: &str) -> Result<i64> {
    let reply = conn.call_method(
        Some(bus),
        PATH,
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(PLAYER_IFACE, "Position"),
    )?;
    let v: OwnedValue = reply.body().deserialize()?;
    mvalue(&v)
        .as_i64()
        .ok_or_else(|| anyhow!("{bus} sent a position that is not a number"))
}

/// Calls a Player method with no arguments (PlayPause, Next, Previous).
pub fn player_call(conn: &Connection, bus: &str, method: &str) -> Result<()> {
    conn.call_method(Some(bus), PATH, Some(PLAYER_IFACE), method, &())?;
    Ok(())
}

pub fn seek(conn: &Connection, bus: &str, offset_us: i64) -> Result<()> {
    conn.call_method(Some(bus), PATH, Some(PLAYER_IFACE), "Seek", &(offset_us,))?;
    Ok(())
}

pub fn set_position(conn: &Connection, bus: &str, track: &str, pos_us: i64) -> Result<()> {
    let track = zbus::zvariant::ObjectPath::try_from(track)?;
    conn.call_method(
        Some(bus),
        PATH,
        Some(PLAYER_IFACE),
        "SetPosition",
        &(track, pos_us),
    )?;
    Ok(())
}

fn dbus_call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message>
where
    B: serde::ser::Serialize + zbus::zvariant::DynamicType,
{
    Ok(conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        method,
        body,
    )?)
}

/// The bus still answers. False once the session bus went away (a logout
/// that left the daemon behind).
pub fn bus_alive(conn: &Connection) -> bool {
    dbus_call(conn, "GetId", &()).is_ok()
}

/// Every bus name that is an MPRIS player.
pub fn list_players(conn: &Connection) -> Result<Vec<String>> {
    let names: Vec<String> = dbus_call(conn, "ListNames", &())?.body().deserialize()?;
    Ok(names
        .into_iter()
        .filter(|n| tannoy_core::mpris::is_player_bus(n))
        .collect())
}

/// The unique name (`:1.39`) that owns a well-known name. Signals arrive
/// from the unique name, so this is how they are matched to a player.
pub fn name_owner(conn: &Connection, name: &str) -> Option<String> {
    dbus_call(conn, "GetNameOwner", &(name,))
        .ok()?
        .body()
        .deserialize()
        .ok()
}

/// swaync's do not disturb; `None` when swaync is not running.
pub fn swaync_dnd(conn: &Connection) -> Option<bool> {
    conn.call_method(Some(SWAYNC), SWAYNC_PATH, Some(SWAYNC), "GetDnd", &())
        .ok()?
        .body()
        .deserialize()
        .ok()
}
