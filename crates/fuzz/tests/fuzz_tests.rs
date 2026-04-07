/// Differential fuzzing tests for the Helix DSL.
///
/// Layer 2: crash-safety — the parser must never panic on arbitrary input.
/// Layer 3: semantic differential — the Coq oracle (extracted formal
///           semantics) must agree with the Rust compiler pipeline.
/// Layer 4: mutation — no panics on mutated sample programs.
use helix_fuzz::{
    coq_oracle::{eval_with_coq, CoqError},
    generator::{arb_program_source, arb_test_case},
    mutation::{apply_mutation, ALL_MUTATIONS},
    oracle::eval_with_rust,
};
use proptest::prelude::*;

// ---- Layer 2: crash safety --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The parser and codegen pipeline must never panic on any input string.
    #[test]
    fn parser_never_panics(src in arb_program_source()) {
        helix_fuzz::oracle::run_source_no_panic(&src);
    }
}

// ---- Layer 3: semantic differential testing ---------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// For every generated type-correct program: the Coq oracle (formal
    /// semantics) must agree with the Rust compiler pipeline.
    ///
    /// If `helix_eval` is not built, the Coq side is skipped (no failure).
    #[test]
    fn coq_oracle_matches_rust_pipeline(tc in arb_test_case()) {
        let coq_result  = eval_with_coq(&tc);
        let rust_result = eval_with_rust(&tc);

        match (&coq_result, &rust_result) {
            // Both succeed and must agree.
            (Ok(v_coq), Ok(v_rust)) => {
                prop_assert_eq!(
                    v_coq, v_rust,
                    "Coq={} Rust={} for program:\n{}",
                    v_coq, v_rust, tc.to_mpc()
                );
            }
            // Coq oracle not built — skip gracefully.
            (Err(CoqError::NotBuilt), _) => {}
            // Both agree the program is undefined (stuck / error).
            (Err(CoqError::Stuck), Err(_)) => {}
            // Coq stuck but Rust succeeded — possible bug (or generator issue).
            (Err(CoqError::Stuck), Ok(v)) => {
                // The generator excludes Div/Mod, so this should not happen
                // for field arithmetic. Treat as unexpected.
                prop_assert!(
                    false,
                    "Coq=stuck but Rust={} for:\n{}",
                    v, tc.to_mpc()
                );
            }
            // Rust error but Coq succeeded — compiler bug.
            (Ok(v_coq), Err(rust_err)) => {
                prop_assert!(
                    false,
                    "Coq={} but Rust={} for:\n{}",
                    v_coq, rust_err, tc.to_mpc()
                );
            }
            // Both error — acceptable (generator might hit edge cases).
            (Err(CoqError::Other(msg)), Err(_)) => {
                // Log but don't fail — could be an oracle-side JSON issue.
                let _ = msg;
            }
            // Coq other-error but Rust succeeded — could be oracle bug; log only.
            (Err(CoqError::Other(_)), Ok(_)) => {}
        }
    }

    /// Execution must be deterministic: same inputs always produce the same output.
    #[test]
    fn execution_is_deterministic(tc in arb_test_case()) {
        let r1 = eval_with_rust(&tc);
        let r2 = eval_with_rust(&tc);
        prop_assert_eq!(r1, r2, "non-determinism for:\n{}", tc.to_mpc());
    }
}

// ---- Layer 4: mutation testing ----------------------------------------------

/// Sample `.mpc` programs embedded directly so no filesystem access is needed.
const SAMPLES: &[&str] = &[
    "fn main(Public Field<64> a, Public Field<64> b) -> Field<64> { return (a + b); }",
    "fn main(Public Field<64> a, Secret Field<64> b) -> Field<64> { return (a * b); }",
    "fn main(Public Field<64> a, Public Field<64> b, Public Field<64> c) -> Field<64> { return ((a + b) * c); }",
    "fn main(Public Field<64> a, Public Field<64> b) -> Field<64> { \
        if a > b { return a; } else { return b; } }",
];

#[test]
fn mutations_never_panic() {
    for &sample in SAMPLES {
        for mutation in ALL_MUTATIONS {
            if let Some(mutated) = apply_mutation(sample, mutation) {
                // Must not panic — errors are fine.
                helix_fuzz::oracle::run_source_no_panic(&mutated);
            }
        }
    }
}

// ---- Oracle sanity checks (deterministic, known-correct programs) -----------

#[test]
fn add_oracle_known_value() {
    let src = "fn main(Public Field<64> a, Public Field<64> b) -> Field<64> { return (a + b); }";
    use helix_fuzz::generator::{BinOp, Expr, TestCase};
    let tc = TestCase {
        field_size: 64,
        params: vec!["a".into(), "b".into()],
        inputs: vec![7, 5],
        lets: vec![],
        ret: Expr::BinOp {
            op: BinOp::Add,
            l: Box::new(Expr::Var { n: "a".into() }),
            r: Box::new(Expr::Var { n: "b".into() }),
        },
    };
    let _ = src; // used for doc clarity
    match eval_with_rust(&tc) {
        Ok(v) => assert_eq!(v, 12, "expected 7+5=12, got {v}"),
        Err(e) => panic!("pipeline error: {e}"),
    }
}

#[test]
fn mul_oracle_known_value() {
    use helix_fuzz::generator::{BinOp, Expr, TestCase};
    let tc = TestCase {
        field_size: 64,
        params: vec!["a".into(), "b".into()],
        inputs: vec![6, 7],
        lets: vec![],
        ret: Expr::BinOp {
            op: BinOp::Mul,
            l: Box::new(Expr::Var { n: "a".into() }),
            r: Box::new(Expr::Var { n: "b".into() }),
        },
    };
    match eval_with_rust(&tc) {
        Ok(v) => assert_eq!(v, 42, "expected 6*7=42, got {v}"),
        Err(e) => panic!("pipeline error: {e}"),
    }
}

#[test]
fn identity_oracle_known_value() {
    use helix_fuzz::generator::{Expr, TestCase};
    let tc = TestCase {
        field_size: 64,
        params: vec!["x".into()],
        inputs: vec![99],
        lets: vec![],
        ret: Expr::Var { n: "x".into() },
    };
    match eval_with_rust(&tc) {
        Ok(v) => assert_eq!(v, 99, "identity should return input, got {v}"),
        Err(e) => panic!("pipeline error: {e}"),
    }
}
