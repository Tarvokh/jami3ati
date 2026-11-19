//! Presentation helpers.
//!
//! Every function here is pure: same input, same output, no clock reads. The
//! locale is an explicit argument rather than a global, so a French page and an
//! Arabic page can be rendered from the same data in one test and the output
//! stays deterministic.

use chrono::{DateTime, Datelike, Duration, FixedOffset};

use crate::i18n::Locale;

/// Human byte size in the locale's units. Deliberately decimal-free under
/// 1 KiB so a 119-byte test message reads as `119 بايت`, not `0.1 ك.ب`.
pub fn bytes(locale: Locale, n: i64) -> String {
    let n = n.max(0);
    let (unit, kibi, mebi, gibi) = match locale {
        Locale::Ar => ("بايت", "ك.ب", "م.ب", "ج.ب"),
        Locale::Fr => ("octets", "Ko", "Mo", "Go"),
        Locale::En => ("bytes", "KB", "MB", "GB"),
    };
    if n < 1024 {
        // French and English inflect the unit; Arabic does not.
        let unit = match (locale, n) {
            (Locale::Fr, 1) => "octet",
            (Locale::En, 1) => "byte",
            _ => unit,
        };
        return format!("{n} {unit}");
    }
    let kib = n as f64 / 1024.0;
    if kib < 1024.0 {
        return format!("{kib:.1} {kibi}");
    }
    let mib = kib / 1024.0;
    if mib < 1024.0 {
        return format!("{mib:.1} {mebi}");
    }
    format!("{:.2} {gibi}", mib / 1024.0)
}

/// Compact timestamp for list rows: today shows the clock, yesterday shows the
/// locale's word for it, this year shows a short date, older a full date.
pub fn stamp(locale: Locale, ts: DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> String {
    let day = ts.date_naive();
    let today = now.date_naive();

    if day == today {
        ts.format("%H:%M").to_string()
    } else if day + Duration::days(1) == today {
        match locale {
            Locale::Ar => "أمس".to_string(),
            Locale::Fr => "hier".to_string(),
            Locale::En => "yesterday".to_string(),
        }
    } else if day.year() == today.year() {
        short_date(locale, ts)
    } else {
        long_date(locale, ts)
    }
}

/// Full timestamp for the reading pane and the session list.
pub fn full_stamp(locale: Locale, ts: DateTime<FixedOffset>) -> String {
    match locale {
        Locale::Ar => ts.format("%Y-%m-%d %H:%M").to_string(),
        Locale::Fr => ts.format("%d/%m/%Y %H:%M").to_string(),
        Locale::En => ts.format("%d %b %Y %H:%M").to_string(),
    }
}

/// Day and month without a year. English uses a month abbreviation because a
/// numeric `11/04` is read as April by half the audience and November by the
/// other half; the abbreviation cannot be misread.
fn short_date(locale: Locale, ts: DateTime<FixedOffset>) -> String {
    match locale {
        Locale::Ar | Locale::Fr => ts.format("%d/%m").to_string(),
        Locale::En => ts.format("%d %b").to_string(),
    }
}

/// Day, month and year.
fn long_date(locale: Locale, ts: DateTime<FixedOffset>) -> String {
    match locale {
        Locale::Ar | Locale::Fr => ts.format("%d/%m/%Y").to_string(),
        Locale::En => ts.format("%d %b %Y").to_string(),
    }
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
        assert_eq!(bytes(Locale::Ar, 0), "0 بايت");
        assert_eq!(bytes(Locale::Ar, 119), "119 بايت");
        assert_eq!(bytes(Locale::Ar, 2048), "2.0 ك.ب");
        assert_eq!(bytes(Locale::Ar, 5 * 1024 * 1024), "5.0 م.ب");
        assert_eq!(bytes(Locale::Fr, 1), "1 octet");
        assert_eq!(bytes(Locale::Fr, 119), "119 octets");
        assert_eq!(bytes(Locale::Fr, 2048), "2.0 Ko");
        assert_eq!(bytes(Locale::En, 1), "1 byte");
        assert_eq!(bytes(Locale::En, 119), "119 bytes");
        assert_eq!(bytes(Locale::En, 5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn timestamps_are_relative_to_the_supplied_now() {
        let now = at("2026-11-07T12:00:00+01:00");
        assert_eq!(
            stamp(Locale::Ar, at("2026-11-07T10:42:00+01:00"), now),
            "10:42"
        );
        assert_eq!(
            stamp(Locale::Ar, at("2026-11-06T23:10:00+01:00"), now),
            "أمس"
        );
        assert_eq!(
            stamp(Locale::Ar, at("2026-11-04T09:00:00+01:00"), now),
            "04/11"
        );
        assert_eq!(
            stamp(Locale::Ar, at("2025-11-04T09:00:00+01:00"), now),
            "04/11/2025"
        );

        assert_eq!(
            stamp(Locale::Fr, at("2026-11-06T23:10:00+01:00"), now),
            "hier"
        );
        assert_eq!(
            stamp(Locale::En, at("2026-11-06T23:10:00+01:00"), now),
            "yesterday"
        );
        assert_eq!(
            stamp(Locale::En, at("2026-11-04T09:00:00+01:00"), now),
            "04 Nov"
        );
        assert_eq!(
            stamp(Locale::En, at("2025-11-04T09:00:00+01:00"), now),
            "04 Nov 2025"
        );
    }

    #[test]
    fn full_stamps_follow_the_locale() {
        let ts = at("2026-11-04T09:42:00+01:00");
        assert_eq!(full_stamp(Locale::Ar, ts), "2026-11-04 09:42");
        assert_eq!(full_stamp(Locale::Fr, ts), "04/11/2026 09:42");
        assert_eq!(full_stamp(Locale::En, ts), "04 Nov 2026 09:42");
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
