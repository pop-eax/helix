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

        // ---- Networked n-party BGW ----
        "bgw-np" => {
            let my_id = cli.my_id.ok_or("--my-id required for bgw-np")?;
            let parties = cli.parties.ok_or("--parties required for bgw-np")?;
            let threshold = cli.threshold.ok_or("--threshold required for bgw-np")?;
            let addrs_str = cli.party_addrs.ok_or("--party-addrs required for bgw-np")?;
            // Inputs are comma-separated: use '_' for inputs owned by other parties.
            // Example: "-i 1,2,_,_" means this party owns the first two input wires.
            let my_input_spec = cli.inputs.trim().to_string();
            run_bgw_networked(&program, &my_input_spec, my_id, parties, threshold, &addrs_str).await?;
        }

        // ---- Networked 2-party Yao ----
        "yao-2p" => {
            let my_id = cli.my_id.ok_or("--my-id required for yao-2p")?;
            let addrs_str = cli.party_addrs.ok_or("--party-addrs required for yao-2p")?;
            let my_value: u64 = cli.inputs.trim().parse()
                .map_err(|_| format!("--inputs must be a single u64 for yao-2p, got {:?}", cli.inputs))?;
            run_yao_two_party(&program, my_value, cli.bits, my_id, &addrs_str).await?;
        }

        other => return Err(format!("Unknown backend '{other}'. Use: clear, yao, bgw, yao-2p, bgw-np").into()),
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

// ---- n-party BGW (networked) ----

async fn run_bgw_networked(
    program: &Program,
    input_spec: &str,
    my_id: usize,
    parties: usize,
    threshold: usize,
    addrs_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let addrs: Vec<&str> = addrs_str.split(',').collect();
    if addrs.len() != parties {
        return Err(format!(
            "bgw-np requires exactly {parties} party addresses, got {}",
            addrs.len()
        )
        .into());
    }

    let config = net::NetworkConfig::from_addrs(addrs.iter().copied(), my_id);
    eprintln!("[party {my_id}] connecting to network…");
    let network = net::connect(config).await?;
    eprintln!("[party {my_id}] connected");

    let backend = bgw::BgwNetBackend::new(my_id, parties, threshold)
        .map_err(|e| format!("bgw backend: {e}"))?;

    // Parse comma-separated inputs where '_' means "I don't own this wire".
    // E.g., "1,2,_,_" = this party owns input[0]=1, input[1]=2.
    // Ownership: position i is owned by the party that provides a non-'_' value there.
    // If multiple parties could own the same position, last non-'_' wins; in practice
    // exactly one party should provide each position.
    let spec_parts: Vec<&str> = input_spec.split(',').map(str::trim).collect();
    let n_inputs = program.circuit.inputs.len();
    if spec_parts.len() != n_inputs {
        return Err(format!(
            "input spec has {} entries but circuit has {n_inputs} inputs \
             (use '_' for inputs you don't own, e.g. \"1,2,_,_\")",
            spec_parts.len()
        )
        .into());
    }

    // Ownership: use the party annotation stored in the circuit when present;
    // fall back to the deterministic formula floor(i * parties / n_inputs) otherwise.
    // All parties compute the same owner for each position without communication.
    let owner_of = |i: usize, inp: &ir::lir::Input| -> usize {
        inp.party.map(|p| p.0).unwrap_or_else(|| i * parties / n_inputs)
    };

    let mut inputs: Vec<runtime::InputAssignment> = program
        .circuit
        .inputs
        .iter()
        .enumerate()
        .map(|(i, inp)| runtime::InputAssignment {
            wire: inp.wire,
            owner: owner_of(i, inp),
            value: None,
        })
        .collect();

    for (i, part) in spec_parts.iter().enumerate() {
        if *part != "_" {
            let v: u64 = part.parse().map_err(|_| {
                format!("invalid input at position {i}: expected u64 or '_', got {part:?}")
            })?;
            inputs[i].value = Some(v);
            if inputs[i].owner != my_id {
                let source = if program.circuit.inputs[i].party.is_some() {
                    "circuit party annotation"
                } else {
                    "ownership formula floor(i * parties / n_inputs)"
                };
                return Err(format!(
                    "input {i} is owned by party {} (from {source}), not party {my_id}; \
                     remove this value or move it to the correct party",
                    inputs[i].owner
                )
                .into());
            }
        }
    }

    // Verify that every input owned by this party was explicitly provided.
    for (i, assignment) in inputs.iter().enumerate() {
        if assignment.owner == my_id && assignment.value.is_none() {
            return Err(format!(
                "input {i} is owned by this party ({my_id}), but was marked '_'; \
                 you must provide a value for every input you own"
            )
            .into());
        }
    }

    let mut runner = runtime::Runner::new(network, backend, program.clone(), &inputs)?;
    let outputs = runner.run().await?;
    for (i, (_wire, value)) in outputs.iter().enumerate() {
        println!("output[{i}]: {value}");
    }
    Ok(())
}

// ---- 2-party Yao (networked) ----

/// Message sent from garbler (party 0) to evaluator (party 1).
///
/// The evaluator's active input labels are NOT included here — they are
/// delivered privately via OT (see `ot_ciphertexts`).
#[derive(Serialize, Deserialize)]
struct GarblerMsg {
    garbled_circuit: garbledc::circuit::Circuit,
    /// Garbler's own active input labels (one per bit wire).
    garbler_active_labels: HashMap<String, u128>,
    /// One decode bit (lsb of label₀) per output bit wire.
    output_label_pairs: HashMap<String, u8>,
    /// OT round 3: encrypted label pairs for every evaluator input bit.
    /// Index order matches the evaluator's OT A-point messages.
    ot_ciphertexts: Vec<(u128, u128)>,
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

/// Party 0 — builds and garbles the circuit, runs OT for party 1's inputs.
async fn garbler_run(
    program: &Program,
    my_value: u64,
    bits: usize,
    network: &mut net::Network,
) -> Result<(), Box<dyn std::error::Error>> {
    use runtime::{compile_to_vm_instructions, vm::{VMState, Backend}};
    use garbledc::ot::OTSender;

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

    // Register evaluator's wire so its labels exist in the circuit.
    let eval_wire = program.circuit.inputs[1].wire;
    backend.register_evaluator_wire(eval_wire);

    // ---- OT for evaluator's inputs ----
    //
    // For each bit of the evaluator's wire, we have (label₀, label₁).
    // OT lets the evaluator obtain label_{σ} without us learning σ.

    let ot_messages: Vec<(u128, u128)> = (0..bits)
        .map(|bit_idx| {
            let [l0, l1] = backend
                .wire_label_pair(eval_wire, bit_idx)
                .expect("evaluator wire labels must exist");
            (l0, l1)
        })
        .collect();

    // Round 1: generate and send A-points.
    let (ot_sender, a_bytes) = OTSender::setup(bits);
    network.send(1, &a_bytes).await?;
    eprintln!("[garbler] OT round 1 sent ({} A-points)", bits);

    // Round 2: receive B-points from evaluator.
    let b_bytes: Vec<[u8; 32]> = network.recv(1).await?;
    eprintln!("[garbler] OT round 2 received");

    // Round 3: compute and bundle ciphertexts with the garbled circuit.
    let ot_ciphertexts = ot_sender.respond(&b_bytes, &ot_messages);

    // Garble the circuit; finalize_garbler returns only the garbler's own
    // active labels (evaluator's come from OT).
    let (garbled_circuit, garbler_active_labels, output_label_pairs) =
        backend.finalize_garbler();

    network
        .send(
            1,
            &GarblerMsg {
                garbled_circuit,
                garbler_active_labels,
                output_label_pairs,
                ot_ciphertexts,
            },
        )
        .await?;
    eprintln!("[garbler] sent garbled circuit bundle");

    // Receive the decoded result from the evaluator.
    let result: u64 = network.recv(1).await?;
    println!("output[0]: {result}");
    Ok(())
}

/// Party 1 — participates in OT to obtain its input labels, then evaluates.
async fn evaluator_run(
    program: &Program,
    my_value: u64,
    bits: usize,
    network: &mut net::Network,
) -> Result<(), Box<dyn std::error::Error>> {
    use garbledc::ot::OTReceiver;

    // ---- OT for our own inputs ----
    //
    // Derive choice bits from our plaintext input (LSB first).
    let choices: Vec<bool> = (0..bits).map(|i| (my_value >> i) & 1 == 1).collect();

    // Round 2: receive A-points from garbler.
    let a_bytes: Vec<[u8; 32]> = network.recv(0).await?;
    eprintln!("[evaluator] OT round 1 received");

    // Compute B-points based on our choice bits.
    let (ot_receiver, b_bytes) = OTReceiver::choose(&a_bytes, &choices);
    network.send(0, &b_bytes).await?;
    eprintln!("[evaluator] OT round 2 sent");

    // Receive the garbled circuit bundle (which includes OT round 3).
    let msg: GarblerMsg = network.recv(0).await?;
    eprintln!("[evaluator] received garbled circuit ({} gates)", msg.garbled_circuit.gates.len());

    // Decrypt our input labels via OT round 3.
    let my_labels: Vec<u128> = ot_receiver.finish(&msg.ot_ciphertexts);

    // Build the evaluator's input wire name → active label map.
    let eval_wire = program.circuit.inputs[1].wire;
    let eval_active: HashMap<String, u128> = (0..bits)
        .map(|i| (format!("w{}_b{}", eval_wire.0, i), my_labels[i]))
        .collect();

    // Merge garbler's labels and our OT labels for evaluation.
    let mut active_labels = msg.garbler_active_labels;
    active_labels.extend(eval_active);

    // Evaluate the garbled circuit.
    let results = msg.garbled_circuit.evaluate(active_labels);

    // Decode each output wire (bit by bit, then reconstruct u64).
    let mut output_values: Vec<u64> = Vec::new();
    for &out_wire in &program.circuit.outputs {
        let mut value = 0u64;
        for bit_idx in 0..bits {
            let bit_name = format!("w{}_b{}", out_wire.0, bit_idx);
            if let (Some(&active), Some(&decode_bit)) =
                (results.get(&bit_name), msg.output_label_pairs.get(&bit_name))
            {
                let bit = ((active & 1) ^ (decode_bit as u128)) as u64;
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
