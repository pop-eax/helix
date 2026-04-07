#![no_main]

use libfuzzer_sys::fuzz_target;

/// Crash-safety fuzzer for the Helix DSL parser.
///
/// libFuzzer drives this with arbitrary byte sequences.  The parser must
/// never panic — it may return errors, but must not crash or abort.
///
/// Run with:
///   cargo fuzz run fuzz_parser -- -max_total_time=60
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Must not panic under any input.
        let _ = frontend::parse_program(s);
    }
});
