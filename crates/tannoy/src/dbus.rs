//! The session bus: MPRIS players and the notification daemons' do not
//! disturb. zbus is async; every function here drives its call to the end on
//! the calling thread (`zbus::block_on`) with a 3 second method timeout, so a
//! player that hangs holds up only the helper thread that asked it. Signal
//! streams are plain iterators ([`Signals`]) for the daemon's watcher
//! threads.

use anyhow::{Result, anyhow};
use futures_core::Stream;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::time::Duration;
use tannoy_core::mpris::{MValue, PATH, PLAYER_IFACE, Props};
use zbus::{Connection, DynamicType, MatchRule, Message, MessageStream, OwnedVariant, Variant};

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

pub const SWAYNC: &str = "org.erikreider.swaync.cc";
pub const SWAYNC_PATH: &str = "/org/erikreider/swaync/cc";

/// A session-bus connection whose method calls give up after 3 seconds.
pub fn connect() -> Result<Connection> {
    Ok(zbus::block_on(
        zbus::connection::Builder::session()
            .method_timeout(Duration::from_secs(3))
            .build(),
    )?)
}

/// One method call, waited for on this thread.
fn call<B>(
    conn: &Connection,
    bus: &str,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<Message>
where
    B: serde::ser::Serialize + DynamicType,
{
    zbus::block_on(conn.call_method(Some(bus), path, Some(iface), method, body))
}

/// The signals a match rule selects, as a blocking iterator: each `next`
/// waits on this thread for the next one. Ends when the connection does.
pub struct Signals(MessageStream);

impl Iterator for Signals {
    type Item = zbus::Result<Message>;

    fn next(&mut self) -> Option<Self::Item> {
        let stream = &mut self.0;
        zbus::block_on(std::future::poll_fn(|cx| {
            Pin::new(&mut *stream).poll_next(cx)
        }))
    }
}

/// Subscribes to the signals `rule` selects, keeping at most `queued` that
/// have not been read yet.
pub fn signals(conn: &Connection, rule: MatchRule<'static>, queued: usize) -> Result<Signals> {
    Ok(Signals(zbus::block_on(MessageStream::for_match_rule(
        rule,
        conn,
        NonZeroUsize::new(queued),
    ))?))
}

/// A D-Bus value as the parsers' [`MValue`].
pub fn mvalue(v: &Variant) -> MValue {
    match v {
        Variant::Bool(b) => MValue::Bool(*b),
        Variant::U8(x) => MValue::UInt(*x as u64),
        Variant::U16(x) => MValue::UInt(*x as u64),
        Variant::U32(x) => MValue::UInt(*x as u64),
        Variant::U64(x) => MValue::UInt(*x),
        Variant::I16(x) => MValue::Int(*x as i64),
        Variant::I32(x) => MValue::Int(*x as i64),
        Variant::I64(x) => MValue::Int(*x),
        Variant::F64(x) => MValue::Float(*x),
        Variant::Str(s) => MValue::Str(s.to_string()),
        Variant::ObjectPath(p) => MValue::Str(p.to_string()),
        Variant::Variant(inner) => mvalue(inner),
        Variant::Array(a) => MValue::Strs(
            a.inner()
                .iter()
                .filter_map(|x| match mvalue(x) {
                    MValue::Str(s) => Some(s),
                    _ => None,
                })
                .collect(),
        ),
        Variant::Dict(d) => MValue::Dict(
            d.iter()
                .filter_map(|(k, v)| match k {
                    Variant::Str(k) => Some((k.to_string(), mvalue(v))),
                    _ => None,
                })
                .collect(),
        ),
        _ => MValue::Other,
    }
}

pub fn props(map: &HashMap<String, OwnedVariant>) -> Props {
    map.iter().map(|(k, v)| (k.clone(), mvalue(v))).collect()
}

pub fn get_all(conn: &Connection, bus: &str, iface: &str) -> Result<Props> {
    let reply = call(conn, bus, PATH, PROPERTIES, "GetAll", &(iface,))?;
    let map: HashMap<String, OwnedVariant> = reply.body().deserialize()?;
    Ok(props(&map))
}

/// The player's position in microseconds. MPRIS never signals it, so the
/// daemon asks while the drawer is open.
pub fn position(conn: &Connection, bus: &str) -> Result<i64> {
    let reply = call(
        conn,
        bus,
        PATH,
        PROPERTIES,
        "Get",
        &(PLAYER_IFACE, "Position"),
    )?;
    let v: OwnedVariant = reply.body().deserialize()?;
    mvalue(&v)
        .as_i64()
        .ok_or_else(|| anyhow!("{bus} sent a position that is not a number"))
}

/// Calls a Player method with no arguments (PlayPause, Next, Previous).
pub fn player_call(conn: &Connection, bus: &str, method: &str) -> Result<()> {
    call(conn, bus, PATH, PLAYER_IFACE, method, &())?;
    Ok(())
}

/// Asks the player to bring its window forward.
pub fn raise(conn: &Connection, bus: &str) -> Result<()> {
    call(
        conn,
        bus,
        PATH,
        tannoy_core::mpris::ROOT_IFACE,
        "Raise",
        &(),
    )?;
    Ok(())
}

/// Sets a writable Player property (`Shuffle`, `LoopStatus`).
pub fn set_player_prop(conn: &Connection, bus: &str, name: &str, value: Variant) -> Result<()> {
    call(
        conn,
        bus,
        PATH,
        PROPERTIES,
        "Set",
        &(PLAYER_IFACE, name, value),
    )?;
    Ok(())
}

pub fn seek(conn: &Connection, bus: &str, offset_us: i64) -> Result<()> {
    call(conn, bus, PATH, PLAYER_IFACE, "Seek", &(offset_us,))?;
    Ok(())
}

pub fn set_position(conn: &Connection, bus: &str, track: &str, pos_us: i64) -> Result<()> {
    let track = zbus::ObjectPath::try_from(track)?;
    call(
        conn,
        bus,
        PATH,
        PLAYER_IFACE,
        "SetPosition",
        &(track, pos_us),
    )?;
    Ok(())
}

fn dbus_call<B>(conn: &Connection, method: &str, body: &B) -> Result<Message>
where
    B: serde::ser::Serialize + DynamicType,
{
    Ok(call(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
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

/// Someone owns the name. Asked before calling a notification daemon,
/// because a call to a name nobody owns can make the bus start the service,
/// and Tannoy never starts other programs.
fn has_owner(conn: &Connection, name: &str) -> bool {
    dbus_call(conn, "NameHasOwner", &(name,))
        .ok()
        .and_then(|m| m.body().deserialize::<bool>().ok())
        .unwrap_or(false)
}

/// swaync's do not disturb; `None` when swaync is not running.
pub fn swaync_dnd(conn: &Connection) -> Option<bool> {
    if !has_owner(conn, SWAYNC) {
        return None;
    }
    call(conn, SWAYNC, SWAYNC_PATH, SWAYNC, "GetDnd", &())
        .ok()?
        .body()
        .deserialize()
        .ok()
}

fn get_prop(conn: &Connection, bus: &str, path: &str, iface: &str, name: &str) -> Option<MValue> {
    let reply = call(conn, bus, path, PROPERTIES, "Get", &(iface, name)).ok()?;
    let v: OwnedVariant = reply.body().deserialize().ok()?;
    Some(mvalue(&v))
}

/// dunst's pause; `None` when the notification daemon is not dunst.
pub fn dunst_paused(conn: &Connection) -> Option<bool> {
    use tannoy_core::dnd::{DUNST_IFACE, DUNST_PATH, NOTIFICATIONS};
    get_prop(conn, NOTIFICATIONS, DUNST_PATH, DUNST_IFACE, "paused")?.as_bool()
}

/// mako's modes that are on; `None` when the notification daemon is not
/// mako.
pub fn mako_modes(conn: &Connection) -> Option<MValue> {
    use tannoy_core::dnd::{MAKO_IFACE, MAKO_PATH, NOTIFICATIONS};
    get_prop(conn, NOTIFICATIONS, MAKO_PATH, MAKO_IFACE, "Modes")
}

/// The running notification daemon and its do not disturb: swaync, dunst
/// or mako, whichever answers. `None` when none of them runs.
pub fn read_dnd(conn: &Connection, mako_mode: &str) -> Option<(&'static str, bool)> {
    if let Some(d) = swaync_dnd(conn) {
        return Some(("swaync", d));
    }
    if !has_owner(conn, tannoy_core::dnd::NOTIFICATIONS) {
        return None;
    }
    if let Some(d) = dunst_paused(conn) {
        return Some(("dunst", d));
    }
    mako_modes(conn).map(|m| ("mako", tannoy_core::dnd::mako_dnd(&m, mako_mode)))
}
