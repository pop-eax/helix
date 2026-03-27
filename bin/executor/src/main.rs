use std::{collections::HashMap, fs, path::PathBuf};

use clap::Parser;
use ir::lir::{PartyId, Program, WireId};
use runtime::execute_program;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "runner", about = "Helix MPC circuit runner")]
struct Cli {
    circuit: PathBuf,

    #[arg(short, long)]
    inputs: String,

    #[arg(short, long, default_value = "clear")]
    backend: String,

    /// Bit-width for Yao backend (default 8)
    #[arg(long, default_value_t = 8)]
    bits: usize,

    /// Number of parties for BGW
    #[arg(long)]
    parties: Option<usize>,

    /// Threshold for BGW
    #[arg(long)]
    threshold: Option<usize>,

    /// This party's ID (0-based) — required for networked backends
    #[arg(long)]
    my_id: Option<usize>,

    /// Comma-separated list of party addresses (host:port), one per party
    /// — required for networked backends (e.g. "127.0.0.1:7000,127.0.0.1:7001")
    #[arg(long)]
    party_addrs: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Load compiled circuit
    let bytes = fs::read(&cli.circuit)?;
    let program = Program::from_bytes(&bytes)
        .map_err(|e| format!("Failed to deserialize circuit: {e}"))?;

    match cli.backend.as_str() {
        // ---- Single-process backends ----
        "clear" => {
            let outputs = run_single_process_clear(&program, &cli.inputs)?;
            print_outputs(&outputs);
        }
        "yao" => {
            let outputs = run_single_process_yao(&program, &cli.inputs, cli.bits)?;
            print_outputs(&outputs);
        }
        "bgw" => {
            let parties = cli.parties.ok_or("--parties required for bgw")?;
            let threshold = cli.threshold.ok_or("--threshold required for bgw")?;
            let outputs = run_single_process_bgw(&program, &cli.inputs, parties, threshold)?;
            print_outputs(&outputs);
        }

        // ---- Networked 2-party Yao ----
        "yao-2p" => {
            let my_id = cli.my_id.ok_or("--my-id required for yao-2p")?;
            let addrs_str = cli.party_addrs.ok_or("--party-addrs required for yao-2p")?;
            let my_value: u64 = cli.inputs.trim().parse()
                .map_err(|_| format!("--inputs must be a single u64 for yao-2p, got {:?}", cli.inputs))?;
            run_yao_two_party(&program, my_value, cli.bits, my_id, &addrs_str).await?;
        }

        other => return Err(format!("Unknown backend '{other}'. Use: clear, yao, bgw, yao-2p").into()),
    }

    Ok(())
}

// ---- Single-process helpers ----

fn run_single_process_clear(
    program: &Program,
    inputs_str: &str,
) -> Result<Vec<(WireId, u64)>, Box<dyn std::error::Error>> {
    let input_wires = parse_inputs(program, inputs_str)?;
    let mut b = runtime::ClearBackend::new(program.metadata.field_modulus);
    Ok(execute_program(program, &mut b, &input_wires)?)
}

fn run_single_process_yao(
    program: &Program,
    inputs_str: &str,
    bits: usize,
) -> Result<Vec<(WireId, u64)>, Box<dyn std::error::Error>> {
    let input_wires = parse_inputs(program, inputs_str)?;
    let mut b = garbledc::backend::YaoBackend::new(bits);
    Ok(execute_program(program, &mut b, &input_wires)?)
}

fn run_single_process_bgw(
    program: &Program,
    inputs_str: &str,
    parties: usize,
    threshold: usize,
) -> Result<Vec<(WireId, u64)>, Box<dyn std::error::Error>> {
    let input_wires = parse_inputs(program, inputs_str)?;
    let cfg = bgw::BgwConfig { parties, threshold };
    let mut b = bgw::BgwBackend::new(cfg)?;
    Ok(execute_program(program, &mut b, &input_wires)?)
}

fn parse_inputs(
    program: &Program,
    inputs_str: &str,
) -> Result<Vec<(WireId, PartyId, u64)>, Box<dyn std::error::Error>> {
    let input_values: Vec<u64> = inputs_str
        .split(',')
        .map(|s| s.trim().parse::<u64>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("Invalid input: {e}"))?;

    let n_inputs = program.circuit.inputs.len();
    if input_values.len() != n_inputs {
        return Err(format!(
            "Circuit expects {n_inputs} inputs, got {}",
            input_values.len()
        )
        .into());
    }

    Ok(program
        .circuit
        .inputs
        .iter()
        .zip(input_values.iter())
        .enumerate()
        .map(|(i, (inp, &val))| (inp.wire, PartyId(i), val))
        .collect())
}

fn print_outputs(outputs: &[(WireId, u64)]) {
    for (i, (_wire, value)) in outputs.iter().enumerate() {
        println!("output[{i}]: {value}");
    }
}

// ---- 2-party Yao (networked) ----

/// Message sent from garbler (party 0) to evaluator (party 1).
#[derive(Serialize, Deserialize)]
struct GarblerMsg {
    garbled_circuit: garbledc::circuit::Circuit,
    /// Active input labels: one selected label per input bit wire.
    active_labels: HashMap<String, u128>,
    /// Both labels per output bit wire so the evaluator can decode.
    output_label_pairs: HashMap<String, [u128; 2]>,
}

async fn run_yao_two_party(
    program: &Program,
    my_value: u64,
    bits: usize,
    my_id: usize,
    addrs_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let addrs: Vec<&str> = addrs_str.split(',').collect();
    if addrs.len() != 2 {
        return Err(format!(
            "yao-2p requires exactly 2 party addresses, got {}",
            addrs.len()
        )
        .into());
    }
    if my_id > 1 {
        return Err(format!("yao-2p only supports parties 0 and 1, got --my-id {my_id}").into());
    }

    let config = net::NetworkConfig::from_addrs(addrs.iter().copied(), my_id);
    eprintln!("[party {my_id}] connecting to network…");
    let mut network = net::connect(config).await?;
    eprintln!("[party {my_id}] connected");

    if my_id == 0 {
        garbler_run(program, my_value, bits, &mut network).await
    } else {
        evaluator_run(program, my_value, bits, &mut network).await
    }
}

/// Party 0 — builds and garbles the circuit, sends it to party 1.
async fn garbler_run(
    program: &Program,
    my_value: u64,
    bits: usize,
    network: &mut net::Network,
) -> Result<(), Box<dyn std::error::Error>> {
    use runtime::{compile_to_vm_instructions, vm::{VMState, Backend}};

    // Build circuit structure by running all gate instructions.
    let mut backend = garbledc::backend::YaoBackend::new(bits);
    let instructions = compile_to_vm_instructions(&program.circuit);
    let n_wires = program
        .circuit
        .gates
        .iter()
        .map(|g| g.output.0)
        .chain(program.circuit.inputs.iter().map(|i| i.wire.0))
        .max()
        .unwrap_or(0)
        + 1;
    let mut state = VMState::new(
        n_wires,
        program.metadata.field_modulus.unwrap_or(2_u64.pow(63) - 1),
    );
    for instr in &instructions {
        backend.execute_instruction(instr, &mut state)?;
    }

    // Set our own input labels (party 0 owns circuit input[0]).
    let my_wire = program.circuit.inputs[0].wire;
    backend.set_input(my_wire, my_value, runtime::Visibility::Secret, &mut state)?;

    // Receive evaluator's plaintext input.
    let eval_value: u64 = network.recv(1).await?;
    eprintln!("[garbler] received evaluator input (simplified no-OT)");

    // Set evaluator's input labels (party 1 owns circuit input[1]).
    let eval_wire = program.circuit.inputs[1].wire;
    backend.assign_input_labels(eval_wire, eval_value);

    // Garble and extract the circuit bundle.
    let (garbled_circuit, active_labels, output_label_pairs) = backend.finalize_garbler();

    // Send everything to the evaluator.
    network
        .send(
            1,
            &GarblerMsg { garbled_circuit, active_labels, output_label_pairs },
        )
        .await?;

    // Receive the decoded result from the evaluator.
    let result: u64 = network.recv(1).await?;
    println!("output[0]: {result}");
    Ok(())
}

/// Party 1 — receives the garbled circuit, evaluates, decodes.
async fn evaluator_run(
    program: &Program,
    my_value: u64,
    bits: usize,
    network: &mut net::Network,
) -> Result<(), Box<dyn std::error::Error>> {
    // Send our plaintext input to the garbler (simplified, no OT).
    network.send(0, &my_value).await?;

    // Receive the garbled circuit bundle.
    let msg: GarblerMsg = network.recv(0).await?;
    eprintln!("[evaluator] received garbled circuit ({} gates)", msg.garbled_circuit.gates.len());

    // Evaluate.
    let results = msg.garbled_circuit.evaluate(msg.active_labels);

    // Decode each output wire (bit by bit, then reconstruct u64).
    let mut output_values: Vec<u64> = Vec::new();
    for &out_wire in &program.circuit.outputs {
        let mut value = 0u64;
        for bit_idx in 0..bits {
            let bit_name = format!("w{}_b{}", out_wire.0, bit_idx);
            if let (Some(&active), Some(&pair)) =
                (results.get(&bit_name), msg.output_label_pairs.get(&bit_name))
            {
                let bit = if active == pair[1] { 1u64 } else { 0u64 };
                value |= bit << bit_idx;
            }
        }
        output_values.push(value);
    }

    // Send decoded result back to garbler.
    network.send(0, &output_values[0]).await?;

    for (i, v) in output_values.iter().enumerate() {
        println!("output[{i}]: {v}");
    }
    Ok(())
}
