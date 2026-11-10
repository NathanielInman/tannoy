//! Everything Tannoy knows that is not D-Bus or drawing: where files live,
//! the config, the model, MPRIS parsing, the player pick, cover art, the
//! socket protocol and the waybar reader.


pub mod config;
pub mod model;
pub mod mpris;
pub mod paths;
pub mod pick;
pub mod protocol;
pub mod text;
pub mod transport;

pub use bar_anchor::{Anchor, Edge};

/// Unix time in seconds.
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}
