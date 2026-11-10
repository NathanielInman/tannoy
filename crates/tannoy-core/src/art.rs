//! Cover art: `mpris:artUrl` is a `file://` path (Chrome writes a temporary
//! file, local players point at the album folder) or an `https://` URL
//! (Spotify's `i.scdn.co`). Either way it is copied into
//! `$XDG_CACHE_HOME/tannoy/art/` under a name derived from the URL, so the
//! bar's image module and the drawer read a stable file that outlives the
//! player's own temporary one.
//!
//! Fetching runs on a helper thread in the daemon with a timeout; nothing
//! here is ever called from the bar's stream.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Larger than this is not cover art.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Files kept in the cache; older ones are pruned after each fetch.
pub const KEEP: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Http(String),
}

/// What an `artUrl` points at, or `None` for a scheme Tannoy cannot read.
pub fn source(url: &str) -> Option<Source> {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("file://") {
        // file:///path or file://localhost/path
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        if !path.starts_with('/') {
            return None;
        }
        return Some(Source::File(PathBuf::from(percent_decode(path))));
    }
    if url.starts_with('/') {
        return Some(Source::File(PathBuf::from(url)));
    }
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Some(Source::Http(url.to_string()));
    }
    None
}

/// Decodes `%20`-style escapes; a broken escape is kept as written.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// FNV-1a, so a URL maps to the same file name in every version.
pub fn hash(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The image type from its first bytes, as a file extension. Anything else
/// (an HTML error page, say) is not art.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

/// The cache key: the URL, plus size and modification time for a local
/// file, since a player may rewrite the same path for the next track.
pub fn key(url: &str) -> String {
    match source(url) {
        Some(Source::File(p)) => {
            let stamp = std::fs::metadata(&p)
                .ok()
                .map(|m| {
                    let t = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_nanos())
                        .unwrap_or_default();
                    format!("{}:{t}", m.len())
                })
                .unwrap_or_default();
            hash(&format!("{url}#{stamp}"))
        }
        _ => hash(url),
    }
}

/// An already cached copy of this URL's art.
pub fn cached(dir: &Path, url: &str) -> Option<PathBuf> {
    let k = key(url);
    ["png", "jpg", "webp", "gif", "bmp"]
        .iter()
        .map(|e| dir.join(format!("{k}.{e}")))
        .find(|p| p.exists())
}

fn read_limited(r: impl std::io::Read) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = vec![];
    r.take(MAX_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_BYTES {
        return Err(anyhow!("larger than {} MB", MAX_BYTES / 1024 / 1024));
    }
    Ok(buf)
}

fn download(url: &str, timeout: Duration) -> Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("tannoy/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut resp = agent.get(url).call()?;
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()?;
    Ok(bytes)
}

/// Copies or downloads the art into `dir` and returns the cached path.
/// Blocking: call it from a helper thread.
pub fn fetch(dir: &Path, url: &str, timeout: Duration) -> Result<PathBuf> {
    if let Some(p) = cached(dir, url) {
        return Ok(p);
    }
    let bytes = match source(url).ok_or_else(|| anyhow!("unsupported art URL {url}"))? {
        Source::File(p) => {
            read_limited(std::fs::File::open(&p).with_context(|| p.display().to_string())?)?
        }
        Source::Http(u) => download(&u, timeout)?,
    };
    let ext = sniff(&bytes).ok_or_else(|| anyhow!("{url} is not an image"))?;
    std::fs::create_dir_all(dir)?;
    let k = key(url);
    let path = dir.join(format!("{k}.{ext}"));
    let tmp = dir.join(format!(".{k}.tmp"));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    prune(dir, KEEP);
    Ok(path)
}

/// Keeps the newest `keep` files.
pub fn prune(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            if !m.is_file() {
                return None;
            }
            Some((m.modified().ok()?, e.path()))
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}

/// Icon names to try for a player's app icon.
pub fn icon_names(desktop_entry: Option<&str>, key: &str, name: &str) -> Vec<String> {
    let mut v = vec![];
    if let Some(e) = desktop_entry {
        v.push(e.to_string());
    }
    v.push(key.to_string());
    let n = name.to_ascii_lowercase().replace(' ', "-");
    if !v.contains(&n) {
        v.push(n);
    }
    // Chrome registers as `chromium` and names itself "Chrome".
    if key == "chromium" || name == "Chrome" {
        v.push("google-chrome".into());
        v.push("chromium".into());
    }
    v.dedup();
    v
}

/// A player's app icon from the hicolor theme or pixmaps, for
/// `art.fallback = "player"`.
pub fn player_icon(names: &[String]) -> Option<PathBuf> {
    let roots = ["/usr/share/icons/hicolor", "/usr/local/share/icons/hicolor"];
    let sizes = ["256x256", "128x128", "512x512", "96x96", "64x64", "48x48"];
    let home_icons = crate::paths::home().join(".local/share/icons/hicolor");
    for n in names {
        for root in roots
            .iter()
            .map(PathBuf::from)
            .chain(std::iter::once(home_icons.clone()))
        {
            for s in sizes {
                let p = root.join(s).join("apps").join(format!("{n}.png"));
                if p.exists() {
                    return Some(p);
                }
            }
        }
        let p = PathBuf::from("/usr/share/pixmaps").join(format!("{n}.png"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources() {
        assert_eq!(
            source("file:///tmp/.com.google.Chrome.i7T45d"),
            Some(Source::File("/tmp/.com.google.Chrome.i7T45d".into()))
        );
        assert_eq!(
            source("file:///home/n/Music/Daft%20Punk/cover.jpg"),
            Some(Source::File("/home/n/Music/Daft Punk/cover.jpg".into()))
        );
        assert_eq!(
            source("file://localhost/a.png"),
            Some(Source::File("/a.png".into()))
        );
        assert_eq!(
            source("https://i.scdn.co/image/ab67616d0000b273"),
            Some(Source::Http(
                "https://i.scdn.co/image/ab67616d0000b273".into()
            ))
        );
        assert_eq!(source("data:image/png;base64,AAAA"), None);
        assert_eq!(source(""), None);
    }

    #[test]
    fn decoding() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
    }

    #[test]
    fn sniffing() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 13, 10]), Some("png"));
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(sniff(b"RIFF\x10\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"<!doctype html>"), None);
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(hash(""), "cbf29ce484222325");
        assert_eq!(hash("a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn copies_a_local_file_and_rejects_non_images() {
        let dir = std::env::temp_dir().join(format!("tannoy-art-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("cover one.png");
        std::fs::write(&src, [0x89, b'P', b'N', b'G', 0, 0, 0, 0]).unwrap();
        let url = format!("file://{}", dir.join("cover%20one.png").display());
        let cache = dir.join("cache");
        let got = fetch(&cache, &url, Duration::from_secs(1)).unwrap();
        assert_eq!(got.extension().unwrap(), "png");
        assert_eq!(cached(&cache, &url), Some(got.clone()));
        let bad = dir.join("x.png");
        std::fs::write(&bad, b"not an image").unwrap();
        let err = fetch(
            &cache,
            &format!("file://{}", bad.display()),
            Duration::from_secs(1),
        );
        assert!(err.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pruning_keeps_the_newest() {
        let dir = std::env::temp_dir().join(format!("tannoy-prune-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("{i}.png")), [i]).unwrap();
        }
        prune(&dir, 3);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chrome_icon_names() {
        let n = icon_names(None, "chromium", "Chrome");
        assert!(n.contains(&"google-chrome".to_string()));
        assert_eq!(n[0], "chromium");
    }
}
