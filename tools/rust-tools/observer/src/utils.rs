/// Format a byte count as a human-readable string.
pub fn fmt_bytes(b: u64) -> String {
    if b >= 1_000_000_000 {
        format!("{:.1}GB", b as f64 / 1e9)
    } else if b >= 1_000_000 {
        format!("{:.1}MB", b as f64 / 1e6)
    } else if b >= 1_000 {
        format!("{:.0}KB", b as f64 / 1e3)
    } else {
        format!("{b}B")
    }
}

/// Truncate an ID string to at most `max_w` characters, inserting an ellipsis
/// in the middle when the full string doesn't fit.
pub fn short_id(id: &str, max_w: usize) -> String {
    if id.len() <= max_w {
        return id.to_string();
    }
    if max_w < 6 {
        return id[..max_w].to_string();
    }
    let tail = max_w.saturating_sub(5);
    format!("{}…{}", &id[..4], &id[id.len() - tail..])
}
