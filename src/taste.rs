pub const REASONS: [&str; 6] = [
    "slow_open",
    "face_out",
    "no_payoff",
    "caption_wall",
    "dead_air",
    "boring",
];

pub fn card_line(counts: &[(String, u32)]) -> Option<String> {
    let parts: Vec<String> = counts
        .iter()
        .filter(|(reason, n)| *n >= 3 && REASONS.contains(&reason.as_str()))
        .map(|(reason, n)| format!("{reason} x{n}"))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(format!(
            "User rejects: {}. Do not open that way.",
            parts.join(", ")
        ))
    }
}

/// Keep tags are motion names and score terms the user actually selected.
pub fn keep_line(counts: &[(String, u32)]) -> Option<String> {
    let mut parts: Vec<&str> = counts
        .iter()
        .filter(|(reason, n)| {
            *n >= 3 && !REASONS.contains(&reason.as_str()) && !reason.starts_with("passed")
        })
        .map(|(reason, _)| reason.as_str())
        .collect();
    if parts.is_empty() {
        return None;
    }
    parts.sort_unstable();
    Some(format!("User keeps: {}.", parts.join(", ")))
}

pub fn passed_tag(motion: &str) -> String {
    format!("passed:{motion}")
}

/// A fidget peel is not a taste event.
pub fn fidget_taste_reason() -> Option<String> {
    None
}

pub fn shape_rejected(why: &str, reason: &str) -> bool {
    let why = why.to_ascii_lowercase();
    let spaced = reason.replace('_', " ");
    why.contains(&reason.to_ascii_lowercase()) || why.contains(&spaced)
}

/// UTC calendar day for a unix timestamp.
pub fn utc_date(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn remix_due(open_chores: i64, remix_on_day: bool, has_old_clip: bool) -> bool {
    open_chores == 0 && !remix_on_day && has_old_clip
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_rejects_emit_the_card_two_do_not() {
        assert!(card_line(&[("slow_open".into(), 2)]).is_none());
        let card = card_line(&[("slow_open".into(), 3)]).unwrap();
        assert_eq!(card, "User rejects: slow_open x3. Do not open that way.");
    }

    #[test]
    fn three_keeps_emit_the_line_two_do_not() {
        assert!(keep_line(&[("push_in".into(), 2)]).is_none());
        let line = keep_line(&[("push_in".into(), 3), ("hook".into(), 3), ("passed:hold".into(), 9)]).unwrap();
        assert_eq!(line, "User keeps: hook, push_in.");
    }

    #[test]
    fn unix_epoch_is_the_civil_day() {
        assert_eq!(utc_date(0), "1970-01-01");
    }

    #[test]
    fn remix_only_when_the_inbox_is_empty() {
        assert!(remix_due(0, false, true));
        assert!(!remix_due(1, false, true));
        assert!(!remix_due(0, true, true));
        assert!(!remix_due(0, false, false));
    }
}
