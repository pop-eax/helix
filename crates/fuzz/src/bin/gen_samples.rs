/// Sample generator binary.
///
/// Generates a batch of random test cases and writes them to an output
/// directory as paired `.mpc` and `.json` files.
///
/// Usage:
///   cargo run -p helix-fuzz --bin gen_samples -- --count 100 --out tests/fuzz-samples
use helix_fuzz::generator::{arb_test_case, TestCase};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, TestRng, TestRunner};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let count = parse_flag(&args, "--count").unwrap_or(50);
    let out = parse_flag_str(&args, "--out").unwrap_or_else(|| "tests/fuzz-samples".to_string());

    let out_path = PathBuf::from(&out);
    std::fs::create_dir_all(&out_path).expect("failed to create output directory");

    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::deterministic_rng(proptest::test_runner::RngAlgorithm::ChaCha),
    );

    let strategy = arb_test_case();
    let mut generated = 0usize;

    while generated < count {
        let tree = strategy
            .new_tree(&mut runner)
            .expect("strategy should produce a value");
        let tc: TestCase = tree.current();

        // Validate: try compiling it; skip if the pipeline rejects it
        // (this can happen for edge cases the generator doesn't perfectly avoid)
        if helix_fuzz::oracle::eval_with_rust(&tc).is_err() {
            continue;
        }

        let mpc_path = out_path.join(format!("{generated:04}.mpc"));
        let json_path = out_path.join(format!("{generated:04}.json"));

        std::fs::write(&mpc_path, tc.to_mpc())
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", mpc_path.display()));
        std::fs::write(&json_path, tc.to_json())
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", json_path.display()));

        generated += 1;
        if generated % 10 == 0 {
            eprintln!("Generated {generated}/{count} samples...");
        }
    }

    println!(
        "Done: wrote {count} test cases to {}",
        out_path.display()
    );
}

fn parse_flag(args: &[String], flag: &str) -> Option<usize> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w[1].parse().ok())
}

fn parse_flag_str(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}
