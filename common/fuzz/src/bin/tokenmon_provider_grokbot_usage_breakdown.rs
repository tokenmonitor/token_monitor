//! Invariants: `GrokbotUsageBreakdown::parse` never panics on any JSON object;
//! on `Some`, every slice has a finite positive value, sources are unique and
//! the slices run most used first (ties in source order); and
//! `access_token_from_cookie_header` only ever returns a three-part token.
#![no_main]

use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use tokenmon_provider_grokbot::{GrokbotUsageBreakdown, access_token_from_cookie_header};

fuzz_target!(|data: &[u8]| {
    if let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(data)
        && let Some(breakdown) = GrokbotUsageBreakdown::parse(&root)
    {
        let mut seen = Vec::new();
        for slice in &breakdown.slices {
            assert!(slice.value.is_finite() && slice.value > 0.0);
            assert!(!seen.contains(&slice.source), "duplicate source");
            seen.push(slice.source);
        }
        for pair in breakdown.slices.windows(2) {
            assert!(
                pair[0].value > pair[1].value
                    || (pair[0].value.total_cmp(&pair[1].value).is_eq()
                        && pair[0].source < pair[1].source),
                "slices out of order"
            );
        }
    }
    if let Ok(text) = std::str::from_utf8(data)
        && let Some(token) = access_token_from_cookie_header(text)
    {
        assert_eq!(token.split('.').count(), 3);
    }
});
