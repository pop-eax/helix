#![no_main]

use libfuzzer_sys::fuzz_target;

/// Crash-safety fuzzer for the full Helix DSL compiler pipeline.
///
/// Feeds arbitrary bytes through parse → type check → HIR codegen.
/// None of these stages may panic — errors must be graceful [Err(...)] returns.
///
/// Run with:
///   cargo fuzz run fuzz_pipeline -- -max_total_time=60
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // parse_and_codegen runs: parse_program → build_ast → type_check → codegen.
        // Must not panic under any input.
        let _ = frontend::parse_and_codegen(s);
    }
});
