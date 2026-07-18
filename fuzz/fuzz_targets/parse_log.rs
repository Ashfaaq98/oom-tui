#![no_main]
//! `oom-tui` parses kernel logs it did not produce, from machines it cannot
//! vouch for, often while someone is mid-incident. A panic there is a crash
//! at the worst possible moment, so the parser must survive arbitrary bytes.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let events = oom_tui::parser::parse_log(text);

    // Derived values must hold for anything the parser chose to emit.
    for event in &events {
        if let Some(total) = event.rss_total_kb() {
            let sum = event.anon_rss_kb.unwrap_or(0)
                + event.file_rss_kb.unwrap_or(0)
                + event.shmem_rss_kb.unwrap_or(0);
            assert_eq!(total, sum, "rss_total_kb must equal its parts");
        }
        if let Some(share) = event.rss_share_of_ram() {
            assert!(share.is_finite(), "share of RAM must be a real number");
            assert!(share >= 0.0, "share of RAM must not be negative");
        }
        // Must not panic or misreport when the table is empty.
        let _ = event.victim_was_largest();
        let _ = event.top_consumers(8);
    }
});
