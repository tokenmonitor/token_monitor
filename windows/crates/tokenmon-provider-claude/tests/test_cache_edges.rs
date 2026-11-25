//! Edge cases of the incremental transcript cache: change detection, tally
//! limits, walk limits and the public tally helpers.
#![allow(clippy::unwrap_used)]

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::Write;
use std::path::Path;

use chrono::{FixedOffset, TimeZone, Utc};
use tokenmon_core::usage::Calendar;
use tokenmon_provider_claude::cache::{
    DailyTokenTally, MAX_ADDED_LOG, MAX_FILE_BYTES, MAX_FILES, daily_tokens_from_entries,
};
use tokenmon_provider_claude::{ClaudeCodeFileCache, DailyTokens, Entry};

fn line(n: u32) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"2026-10-10T10:00:00Z","requestId":"r{n}","message":{{"id":"m{n}","model":"claude-opus-4-1","usage":{{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":300,"cache_creation_input_tokens":40}}}}}}"#
    )
}

fn count(days: &DailyTokens) -> i64 {
    days.values()
        .flat_map(|m| m.values())
        .map(|t| t.response_count)
        .sum()
}

fn total(cache: &ClaudeCodeFileCache, dir: &Path) -> i64 {
    cache.update(dir);
    count(&cache.daily_tokens(&Calendar::utc()))
}

fn append(path: &Path, content: &str) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open for append")
        .write_all(content.as_bytes())
        .expect("append");
}

fn entry(key: Option<&str>) -> Entry {
    Entry {
        date: Utc.with_ymd_and_hms(2026, 10, 10, 23, 30, 0).unwrap(),
        model: "claude-opus-4-1".to_owned(),
        input_tokens: 1,
        output_tokens: 2,
        cache_read_tokens: 3,
        cache_write_5m_tokens: 4,
        cache_write_1h_tokens: 5,
        dedupe_key: key.map(str::to_owned),
    }
}

/// A tally counts keyless entries every time and keyed entries once, and the
/// free function agrees with feeding the tally by hand.
#[test]
fn test_tally_dedupes_keyed_entries_and_counts_keyless_ones() {
    let utc = Calendar::utc();
    let entries = [entry(Some("a")), entry(Some("a")), entry(None), entry(None)];
    let mut tally = DailyTokenTally::default();
    assert!(tally.days().is_empty());
    for e in &entries {
        tally.add(e, &utc);
    }
    assert_eq!(count(tally.days()), 3);
    assert_eq!(&daily_tokens_from_entries(&entries, &utc), tally.days());
    let tokyo = Calendar::fixed(FixedOffset::east_opt(9 * 3600).unwrap());
    let shifted = daily_tokens_from_entries(&entries[..1], &tokyo);
    assert_eq!(
        shifted.keys().next().unwrap().to_string(),
        "2026-10-11",
        "23:30 UTC is already the next day in Tokyo"
    );
}

/// Asking for more distinct calendars than the cache keeps tallies for still
/// yields exact totals for every one of them.
#[test]
fn test_more_calendars_than_tally_slots_stay_exact() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.jsonl"), format!("{}\n", line(1))).unwrap();
    let cache = ClaudeCodeFileCache::new();
    cache.update(dir.path());
    for hours in 0..7 {
        let cal = Calendar::fixed(FixedOffset::east_opt(hours * 3600).unwrap());
        assert_eq!(count(&cache.daily_tokens(&cal)), 1, "offset {hours}");
    }
    assert_eq!(count(&cache.daily_tokens(&Calendar::utc())), 1);
}

/// A tally rebuilt after a cached file vanished from disk (without a
/// directory update) skips that file instead of failing.
#[test]
fn test_rebuild_skips_a_file_deleted_behind_the_caches_back() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("gone.jsonl");
    fs::write(&gone, format!("{}\n", line(1))).unwrap();
    fs::write(dir.path().join("kept.jsonl"), format!("{}\n", line(2))).unwrap();
    let cache = ClaudeCodeFileCache::new();
    cache.update(dir.path());
    fs::remove_file(&gone).unwrap();
    assert_eq!(count(&cache.daily_tokens(&Calendar::utc())), 1);
}

/// New entries from two files that together exceed the entry log make the
/// tallies rebuild from disk, with exact totals.
#[test]
fn test_two_files_that_overflow_the_log_together_rebuild_exactly() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("seed.jsonl"), format!("{}\n", line(0))).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 1);
    let per_file = u32::try_from(MAX_ADDED_LOG / 2 + 10).unwrap();
    for (f, base) in [("one.jsonl", 1u32), ("two.jsonl", 1_000_000u32)] {
        let mut body = String::new();
        for i in 0..per_file {
            body.push_str(&line(base + i));
            body.push('\n');
        }
        fs::write(dir.path().join(f), body).unwrap();
    }
    assert_eq!(total(&cache, dir.path()), 1 + 2 * i64::from(per_file));
}

/// A file whose size changed but whose modified time did not is still noticed
/// and the appended entry counted.
#[test]
fn test_growth_with_unchanged_mtime_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jsonl");
    fs::write(&f, format!("{}\n", line(1))).unwrap();
    let before = fs::metadata(&f).unwrap().modified().unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 1);
    append(&f, &format!("{}\n", line(2)));
    File::options()
        .write(true)
        .open(&f)
        .unwrap()
        .set_times(FileTimes::new().set_modified(before))
        .unwrap();
    assert_eq!(total(&cache, dir.path()), 2);
}

/// A file whose last parsed bytes did not end in a newline (whole JSON, no
/// terminator) is parsed again in full when it grows, never double counted.
#[test]
fn test_unterminated_final_line_is_reparsed_when_the_file_grows() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jsonl");
    fs::write(&f, line(1)).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 1);
    append(&f, &format!("\n{}\n", line(2)));
    assert_eq!(total(&cache, dir.path()), 2);
}

/// A file with nothing parsed yet (only a partial line) is parsed from the
/// start once it grows.
#[test]
fn test_a_file_with_nothing_parsed_yet_is_parsed_from_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jsonl");
    fs::write(&f, r#"{"type":"assis"#).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 0);
    fs::write(&f, format!("{}\n", line(1))).unwrap();
    assert_eq!(total(&cache, dir.path()), 1);
}

/// A file that shrank is parsed again from scratch.
#[test]
fn test_a_shrunk_file_is_reparsed() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jsonl");
    fs::write(&f, format!("{}\n{}\n{}\n", line(1), line(2), line(3))).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 3);
    fs::write(&f, format!("{}\n", line(1))).unwrap();
    assert_eq!(total(&cache, dir.path()), 1);
}

/// A file rewritten in place to something longer whose old tail no longer
/// matches is parsed again in full, not appended to.
#[test]
fn test_a_rewritten_longer_file_is_reparsed_not_appended() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jsonl");
    fs::write(&f, format!("{}\n", line(1))).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 1);
    fs::write(&f, format!("{}\n{}\n{}\n", line(7), line(8), line(9))).unwrap();
    assert_eq!(total(&cache, dir.path()), 3);
}

/// A file replaced by a different one (new identity) is parsed again.
#[cfg(unix)]
#[test]
fn test_a_replaced_file_is_reparsed() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jsonl");
    fs::write(&f, format!("{}\n", line(1))).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 1);
    let other = dir.path().join(".replacement");
    fs::write(&other, format!("{}\n{}\n", line(5), line(6))).unwrap();
    fs::rename(&other, &f).unwrap();
    assert_eq!(total(&cache, dir.path()), 2);
}

/// Symlinked transcripts and files over the size cap are not read.
#[cfg(unix)]
#[test]
fn test_symlinks_and_oversized_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("real.jsonl"), format!("{}\n", line(1))).unwrap();
    std::os::unix::fs::symlink(dir.path().join("real.jsonl"), dir.path().join("link.jsonl"))
        .unwrap();
    let huge = File::create(dir.path().join("huge.jsonl")).unwrap();
    huge.set_len(MAX_FILE_BYTES + 1).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 1);
}

/// Directories nested past the depth cap are not walked.
#[test]
fn test_directories_nested_past_the_depth_cap_are_not_walked() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("top.jsonl"), format!("{}\n", line(1))).unwrap();
    let mut deep = dir.path().to_path_buf();
    for _ in 0..70 {
        deep.push("d");
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("deep.jsonl"), format!("{}\n", line(2))).unwrap();
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 1);
}

/// The walk stops collecting at the file cap.
#[test]
fn test_the_walk_stops_at_the_file_cap() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..=MAX_FILES {
        File::create(dir.path().join(format!("{i}.jsonl"))).unwrap();
    }
    let cache = ClaudeCodeFileCache::new();
    assert_eq!(total(&cache, dir.path()), 0);
}
