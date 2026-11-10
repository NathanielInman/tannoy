//! Finding waybar to signal its image module, from `/proc` (no `pkill`).

use std::path::Path;

/// Whether a `/proc/<pid>/status` says the process catches `signal`. A
/// real-time signal kills a process that has no handler for it, so the
/// daemon only signals a waybar that has installed one.
pub fn catches(status: &str, signal: i32) -> bool {
    if signal < 1 {
        return false;
    }
    let Some(mask) = status
        .lines()
        .find_map(|l| l.strip_prefix("SigCgt:"))
        .map(str::trim)
    else {
        return false;
    };
    let Ok(bits) = u64::from_str_radix(mask, 16) else {
        return false;
    };
    bits & (1u64 << (signal - 1)) != 0
}

/// The process's uid from `/proc/<pid>/status`.
pub fn uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Pids of `name` processes owned by `owner`, with their status text.
pub fn find(proc_root: &Path, name: &str, owner: u32) -> Vec<(i32, String)> {
    let Ok(rd) = std::fs::read_dir(proc_root) else {
        return vec![];
    };
    rd.flatten()
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
            if comm.trim() != name {
                return None;
            }
            let status = std::fs::read_to_string(e.path().join("status")).ok()?;
            (uid(&status) == Some(owner)).then_some((pid, status))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // From a running waybar 0.16: it catches SIGRTMIN+1 (35) through 64,
    // but not SIGRTMIN (34) itself.
    const WAYBAR: &str = "Name:\twaybar\nUmask:\t0022\nState:\tS (sleeping)\nUid:\t1000\t1000\t1000\t1000\nSigQ:\t0/247364\nSigPnd:\t0000000000000000\nSigBlk:\t0000000000000000\nSigIgn:\t0000000000001000\nSigCgt:\tfffffffd00010a02\n";

    #[test]
    fn reads_the_handler_mask() {
        assert!(catches(WAYBAR, 34 + 12));
        assert!(catches(WAYBAR, 64));
        assert!(!catches(WAYBAR, 34));
        assert!(catches(WAYBAR, 35));
        assert!(!catches("SigCgt:\t0000000000000000\n", 46));
        assert!(!catches("", 46));
        assert_eq!(uid(WAYBAR), Some(1000));
    }
}
