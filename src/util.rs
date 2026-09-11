//! Small formatting helpers.

/// Human-readable byte size, cloc-style (e.g. "48.2 KB").
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// Group-separated integer, e.g. 213355 -> "213,355".
pub fn group_int(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

/// Render a token value for tables: rounds and group-separates; approximate
/// values are prefixed with `~`.
pub fn fmt_tokens(value: f64, approx: bool) -> String {
    let rounded = value.round() as u64;
    let s = group_int(rounded);
    if approx {
        format!("~{s}")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_formatting() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(49357), "48.2 KB");
    }

    #[test]
    fn int_grouping() {
        assert_eq!(group_int(0), "0");
        assert_eq!(group_int(999), "999");
        assert_eq!(group_int(213355), "213,355");
    }

    #[test]
    fn token_formatting() {
        assert_eq!(fmt_tokens(3412.0, false), "3,412");
        assert_eq!(fmt_tokens(3412.4, true), "~3,412");
    }
}
