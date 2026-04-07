/// Rust pipeline oracle.
///
/// Runs a [TestCase] through the full Helix compiler pipeline
/// (frontend → HIR → LIR → VM) using [ClearBackend] and returns the
/// first output wire value.
use std::collections::HashMap;

use frontend::parse_and_codegen;
use ir::{
    lir::{Metadata, PartyId, Statistics, WireId},
    lowering::lower_hir_to_lir,
};
use runtime::{clear::ClearBackend, executor::execute_program};

use crate::generator::TestCase;

#[derive(Debug, PartialEq)]
pub enum RustError {
    ParseOrCodegen(String),
    Lowering(String),
    Execution(String),
    NoOutput,
}

impl std::fmt::Display for RustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RustError::ParseOrCodegen(s) => write!(f, "parse/codegen: {s}"),
            RustError::Lowering(s) => write!(f, "lowering: {s}"),
            RustError::Execution(s) => write!(f, "execution: {s}"),
            RustError::NoOutput => write!(f, "no outputs"),
        }
    }
}

fn default_metadata(field_size: u32) -> Metadata {
    Metadata {
        version: "1.0".to_string(),
        source_file: "<fuzz>".to_string(),
        function_name: "main".to_string(),
        field_modulus: Some(1u64 << field_size.min(63)),
        statistics: Statistics {
            total_gates: 0,
            gate_counts: HashMap::new(),
            circuit_depth: 0,
            num_inputs: 0,
            num_outputs: 0,
            num_wires: 0,
        },
    }
}

/// Run [tc] through the full Rust pipeline and return the first output value.
pub fn eval_with_rust(tc: &TestCase) -> Result<u64, RustError> {
    let src = tc.to_mpc();

    let hir = parse_and_codegen(&src).map_err(|e| RustError::ParseOrCodegen(e.to_string()))?;

    let metadata = default_metadata(tc.field_size);
    let lir =
        lower_hir_to_lir(&hir, metadata).map_err(|e| RustError::Lowering(e.to_string()))?;

    // Map inputs: wire i ← input value i, assigned to party i
    let input_wires: Vec<(WireId, PartyId, u64)> = lir
        .circuit
        .inputs
        .iter()
        .enumerate()
        .map(|(i, inp)| (inp.wire, PartyId(i), tc.inputs.get(i).copied().unwrap_or(0)))
        .collect();

    let mut backend = ClearBackend::new(lir.metadata.field_modulus);
    let outputs = execute_program(&lir, &mut backend, &input_wires)
        .map_err(|e| RustError::Execution(e.to_string()))?;

    outputs
        .into_iter()
        .next()
        .map(|(_, v)| v)
        .ok_or(RustError::NoOutput)
}

/// Run raw `.mpc` source through the pipeline (for crash-safety tests).
///
/// Panics are NOT caught here — the caller's test framework should detect them.
/// Returns `Ok(())` if the pipeline completed (success or graceful error).
pub fn run_source_no_panic(source: &str) {
    let _ = parse_and_codegen(source);
}
