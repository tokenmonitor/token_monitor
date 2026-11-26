//! LDML-style date pattern rendering: repeat counts, quoting and pass-through.

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use tokenmon_core::util::Format;

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
        .single()
        .expect("valid instant")
}

fn utc() -> FixedOffset {
    FixedOffset::east_opt(0).expect("utc")
}

fn render(pattern: &str) -> String {
    Format::date(at(2026, 7, 6, 18, 5, 9), pattern, utc())
}

/// Year patterns: two digits for `yy`, zero padded otherwise.
#[test]
fn test_year_widths() {
    assert_eq!(render("yy"), "26");
    assert_eq!(render("y"), "2026");
    assert_eq!(render("yyyy"), "2026");
    assert_eq!(render("yyyyyy"), "002026");
    assert_eq!(Format::date(at(2005, 1, 1, 0, 0, 0), "yy", utc()), "05");
}

/// Month patterns: numeric, abbreviated, full and the narrow letter.
#[test]
fn test_month_widths() {
    assert_eq!(render("M"), "7");
    assert_eq!(render("MM"), "07");
    assert_eq!(render("MMM"), "Jul");
    assert_eq!(render("MMMM"), "July");
    assert_eq!(render("MMMMM"), "J");
}

/// Weekday patterns: abbreviated up to three letters, full, narrow.
#[test]
fn test_weekday_widths() {
    assert_eq!(render("E"), "Mon");
    assert_eq!(render("EEE"), "Mon");
    assert_eq!(render("EEEE"), "Monday");
    assert_eq!(render("EEEEE"), "M");
}

/// Day, hour, minute and second pad to two digits and never beyond.
#[test]
fn test_time_fields_pad_and_cap_at_two_digits() {
    assert_eq!(render("d"), "6");
    assert_eq!(render("dd"), "06");
    assert_eq!(render("dddd"), "06");
    assert_eq!(render("h:mm:ss a"), "6:05:09 PM");
    assert_eq!(render("H:m:s"), "18:5:9");
    assert_eq!(render("HHHH"), "18");
    assert_eq!(render("hh"), "06");
}

/// Noon and midnight are 12 on the 12-hour clock, and AM/PM flips at noon.
#[test]
fn test_twelve_hour_clock_edges() {
    assert_eq!(Format::date(at(2026, 1, 1, 0, 0, 0), "h a", utc()), "12 AM");
    assert_eq!(
        Format::date(at(2026, 1, 1, 12, 0, 0), "h a", utc()),
        "12 PM"
    );
    assert_eq!(
        Format::date(at(2026, 1, 1, 11, 59, 0), "h a", utc()),
        "11 AM"
    );
}

/// Quoted text is literal; doubled quotes are an apostrophe, inside or
/// outside quotes; an unterminated quote runs to the end.
#[test]
fn test_quoting_rules() {
    assert_eq!(render("'at' h"), "at 6");
    assert_eq!(render("h''m"), "6'5");
    assert_eq!(render("'it''s' yy"), "it's 26");
    assert_eq!(render("'open h"), "open h");
    assert_eq!(render("''"), "'");
}

/// Unsupported letters repeat as written and punctuation passes through.
#[test]
fn test_unsupported_letters_and_punctuation_pass_through() {
    assert_eq!(render("zzz"), "zzz");
    assert_eq!(render("Q-w"), "Q-w");
    assert_eq!(render("d/M, y!"), "6/7, 2026!");
    assert_eq!(render(""), "");
}

/// The offset shifts the calendar day, not just the hour.
#[test]
fn test_offset_moves_the_date() {
    let west = FixedOffset::west_opt(8 * 3600).expect("offset");
    assert_eq!(
        Format::date(at(2026, 7, 6, 3, 0, 0), "dd HH", west),
        "05 19"
    );
}
