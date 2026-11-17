//! Presentation helpers.
//!
//! Every function here is pure: same input, same output, no clock reads, no
//! locale lookup. That keeps them testable and keeps the pages deterministic.

use chrono::{DateTime, Datelike, Duration, FixedOffset};

/// Human byte size in Arabic units. Deliberately decimal-free under 1 KiB so
/// a 119-byte test message reads as `119 بايت`, not `0.1 ك.ب`.
pub fn bytes(n: i64) -> String {
    let n = n.max(0);
    if n < 1024 {
        return format!("{n} بايت");
    }
    let kib = n as f64 / 1024.0;
    if kib < 1024.0 {
        return format!("{kib:.1} ك.ب");
    }
    let mib = kib / 1024.0;
    if mib < 1024.0 {
        return format!("{mib:.1} م.ب");
    }
    format!("{:.2} ج.ب", mib / 1024.0)
}

/// Compact timestamp for list rows: today shows the clock, yesterday shows
/// `أمس`, this year shows `day/month`, older shows the full date.
pub fn stamp(ts: DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> String {
    let day = ts.date_naive();
    let today = now.date_naive();

    if day == today {
        ts.format("%H:%M").to_string()
    } else if day + Duration::days(1) == today {
        "أمس".to_string()
    } else if day.year() == today.year() {
        ts.format("%d/%m").to_string()
    } else {
        ts.format("%d/%m/%Y").to_string()
    }
}

/// Full timestamp for the reading pane.
pub fn full_stamp(ts: DateTime<FixedOffset>) -> String {
    ts.format("%Y-%m-%d %H:%M").to_string()
}

/// First visible character, used for the avatar. ASCII is uppercased so
/// `aicha` becomes `A`; Arabic has no case and is returned unchanged.
pub fn initial(name: &str) -> String {
    match name.trim().chars().next() {
        Some(ch) => ch.to_uppercase().collect(),
        None => "؟".to_string(),
    }
}

/// Everything before the `@`. Used only as a fallback display label when the
/// message carries no `From:` display name, and it is labelled as an address
/// underneath so it can never be mistaken for a stored name.
pub fn local_part(address: &str) -> &str {
    address.split('@').next().unwrap_or(address)
}

/// Everything after the last `@`, or an empty string when the address has no
/// domain. Display only: it is never used to make an authorization decision.
pub fn domain_part(address: &str) -> &str {
    address.rsplit_once('@').map_or("", |(_, domain)| domain)
}

/// Deterministic avatar colour class from a stable seed (the address).
pub fn avatar_class(seed: &str) -> &'static str {
    const CLASSES: [&str; 6] = [
        "avatar--0",
        "avatar--1",
        "avatar--2",
        "avatar--3",
        "avatar--4",
        "avatar--5",
    ];
    let sum: u32 = seed.bytes().map(u32::from).sum();
    CLASSES[(sum % CLASSES.len() as u32) as usize]
}

/// Collapses whitespace and truncates on a character boundary. Never splits a
/// multi-byte character, never emits control characters.
pub fn snippet(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut last_was_space = false;
    let mut count = 0usize;

    for ch in text.chars() {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        if ch.is_whitespace() {
            if !out.is_empty() && !last_was_space {
                out.push(' ');
                last_was_space = true;
                count += 1;
            }
            continue;
        }
        if count >= max_chars {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push('…');
            return out;
        }
        out.push(ch);
        last_was_space = false;
        count += 1;
    }

    while out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Used vs. quota as a percentage in `0.0..=100.0`, safe when quota is zero.
pub fn pct(used: i64, quota: i64) -> f64 {
    if quota <= 0 {
        return 0.0;
    }
    ((used.max(0) as f64 / quota as f64) * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(raw: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(raw).expect("timestamp")
    }

    #[test]
    fn byte_sizes_keep_small_values_honest() {
        assert_eq!(bytes(0), "0 بايت");
        assert_eq!(bytes(119), "119 بايت");
        assert_eq!(bytes(2048), "2.0 ك.ب");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 م.ب");
    }

    #[test]
    fn timestamps_are_relative_to_the_supplied_now() {
        let now = at("2026-11-07T12:00:00+01:00");
        assert_eq!(stamp(at("2026-11-07T10:42:00+01:00"), now), "10:42");
        assert_eq!(stamp(at("2026-11-06T23:10:00+01:00"), now), "أمس");
        assert_eq!(stamp(at("2026-11-04T09:00:00+01:00"), now), "04/11");
        assert_eq!(stamp(at("2025-11-04T09:00:00+01:00"), now), "04/11/2025");
    }

    #[test]
    fn snippets_collapse_and_truncate_on_char_boundaries() {
        assert_eq!(snippet("  مرحبا   بالعالم  ", 40), "مرحبا بالعالم");
        assert_eq!(snippet("مرحبا بالعالم", 5), "مرحبا…");
        assert_eq!(snippet("a\nb\tc", 20), "a b c");
    }

    #[test]
    fn initials_handle_arabic_and_ascii() {
        assert_eq!(initial("aicha"), "A");
        assert_eq!(initial("عائشة"), "ع");
        assert_eq!(initial("   "), "؟");
    }

    #[test]
    fn avatar_classes_are_stable() {
        assert_eq!(avatar_class("aicha@uca.ma"), avatar_class("aicha@uca.ma"));
    }

    #[test]
    fn address_parts_split_on_the_at_sign() {
        assert_eq!(local_part("aicha@uca.ma"), "aicha");
        assert_eq!(domain_part("aicha@uca.ma"), "uca.ma");
        assert_eq!(domain_part("aicha"), "");
        assert_eq!(domain_part("a@b@uca.ma"), "uca.ma");
    }

    #[test]
    fn percent_is_clamped() {
        assert_eq!(pct(0, 100), 0.0);
        assert_eq!(pct(150, 100), 100.0);
        assert_eq!(pct(10, 0), 0.0);
    }
}
