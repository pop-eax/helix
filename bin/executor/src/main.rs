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
            let my_input_spec = cli.inputs.trim().to_string();
            run_yao_two_party(&program, &my_input_spec, cli.bits, my_id, &addrs_str).await?;
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

// ---- Shared input-spec parser ----

/// Parse a comma-separated input spec like `"1,2,_,_"` into `InputAssignment`s.
///
/// `_` means "I don't own this wire".  Ownership falls back to
/// `floor(i * n_parties / n_inputs)` when no circuit party annotation is present.
/// Returns an error if a wire owned by another party has a value, or a wire owned
/// by this party is marked `_`.
fn parse_input_spec(
    program: &Program,
    spec: &str,
    my_id: usize,
    n_parties: usize,
) -> Result<Vec<runtime::InputAssignment>, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = spec.split(',').map(str::trim).collect();
    let n_inputs = program.circuit.inputs.len();
    if parts.len() != n_inputs {
        return Err(format!(
            "input spec has {} entries but circuit has {n_inputs} inputs \
             (use '_' for inputs you don't own)",
            parts.len()
        )
        .into());
    }

    let owner_of = |i: usize, inp: &ir::lir::Input| -> usize {
        inp.party
            .map(|p| p.0)
            .unwrap_or_else(|| i * n_parties / n_inputs)
    };

    let mut assignments: Vec<runtime::InputAssignment> = program
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

    for (i, part) in parts.iter().enumerate() {
        if *part != "_" {
            let v: u64 = part.parse().map_err(|_| {
                format!("invalid input at position {i}: expected u64 or '_', got {part:?}")
            })?;
            assignments[i].value = Some(v);
            if assignments[i].owner != my_id {
                let source = if program.circuit.inputs[i].party.is_some() {
                    "circuit party annotation"
                } else {
                    "ownership formula floor(i * n_parties / n_inputs)"
                };
                return Err(format!(
                    "input {i} is owned by party {} (from {source}), not party {my_id}; \
                     remove this value or move it to the correct party",
                    assignments[i].owner
                )
                .into());
            }
        }
    }

    for (i, a) in assignments.iter().enumerate() {
        if a.owner == my_id && a.value.is_none() {
            return Err(format!(
                "input {i} is owned by this party ({my_id}), but was marked '_'; \
                 you must provide a value for every input you own"
            )
            .into());
        }
    }

    Ok(assignments)
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
    let mut network = net::connect(config).await?;
    eprintln!("[party {my_id}] connected");

    // Offline phase — trusted dealer (party 0) generates all Beaver triples
    // with OsRng, Shamir-shares each one, and sends each party only their slice.
    // No party other than the dealer ever sees the full triple.
    let n_muls = bgw::count_multiplications(&program);
    let my_triple_blob: Vec<u8> = if my_id == 0 {
        let blobs = bgw::dealer_generate_triple_blobs(n_muls, parties, threshold);
        for (j, blob) in blobs.iter().enumerate() {
            if j != my_id {
                network.send(j, blob).await?;
            }
        }
        blobs.into_iter().next().unwrap_or_default()
    } else {
        network.recv(0).await?
    };
    eprintln!("[party {my_id}] offline phase complete ({n_muls} triples)");

    let triple_shares = bgw::parse_triple_blob(&my_triple_blob)
        .map_err(|e| format!("triple blob: {e}"))?;
    let backend = bgw::BgwNetBackend::new(my_id, parties, threshold, triple_shares)
        .map_err(|e| format!("bgw backend: {e}"))?;

    let inputs = parse_input_spec(program, input_spec, my_id, parties)?;

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
    input_spec: &str,
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

    let inputs = parse_input_spec(program, input_spec, my_id, 2)?;

    if my_id == 0 {
        garbler_run(program, &inputs, bits, &mut network).await
    } else {
        evaluator_run(program, &inputs, bits, &mut network).await
    }
}

/// Party 0 — builds and garbles the circuit, runs OT for party 1's inputs.
async fn garbler_run(
    program: &Program,
    inputs: &[runtime::InputAssignment],
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

    // Separate garbler-owned (party 0) and evaluator-owned (party 1) wires.
    let garbler_inputs: Vec<(WireId, u64)> = inputs
        .iter()
        .filter(|a| a.owner == 0)
        .map(|a| (a.wire, a.value.unwrap()))
        .collect();
    let eval_wires: Vec<WireId> = inputs
        .iter()
        .filter(|a| a.owner == 1)
        .map(|a| a.wire)
        .collect();

    // Set all garbler-owned input labels.
    for &(wire, value) in &garbler_inputs {
        backend.set_input(wire, value, runtime::Visibility::Secret, &mut state)?;
    }

    // Register all evaluator wires and collect OT messages.
    // Flat order: [bits of wire₀, bits of wire₁, …] LSB-first, matching evaluator.
    let mut ot_messages: Vec<(u128, u128)> = Vec::new();
    for &wire in &eval_wires {
        backend.register_evaluator_wire(wire);
        for bit_idx in 0..bits {
            let [l0, l1] = backend
                .wire_label_pair(wire, bit_idx)
                .expect("evaluator wire labels must exist after register_evaluator_wire");
            ot_messages.push((l0, l1));
        }
    }

    // ---- OT for all evaluator input bits ----
    let n_ot = ot_messages.len();
    let (ot_sender, a_bytes) = OTSender::setup(n_ot);
    network.send(1, &a_bytes).await?;
    eprintln!("[garbler] OT round 1 sent ({n_ot} A-points)");

    let b_bytes: Vec<[u8; 32]> = network.recv(1).await?;
    eprintln!("[garbler] OT round 2 received");

    let ot_ciphertexts = ot_sender.respond(&b_bytes, &ot_messages);

    // Garble and send circuit bundle.
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

    // Receive all decoded output values from the evaluator.
    let results: Vec<u64> = network.recv(1).await?;
    for (i, v) in results.iter().enumerate() {
        println!("output[{i}]: {v}");
    }
    Ok(())
}

/// Party 1 — participates in OT to obtain its input labels, then evaluates.
async fn evaluator_run(
    program: &Program,
    inputs: &[runtime::InputAssignment],
    bits: usize,
    network: &mut net::Network,
) -> Result<(), Box<dyn std::error::Error>> {
    use garbledc::ot::OTReceiver;

    // Collect evaluator-owned wires and values in program-input order.
    // This order must match the garbler's OT message ordering.
    let eval_inputs: Vec<(WireId, u64)> = inputs
        .iter()
        .filter(|a| a.owner == 1)
        .map(|a| (a.wire, a.value.unwrap()))
        .collect();

    // Choice bits: all bits of all evaluator wires, LSB-first per wire.
    let choices: Vec<bool> = eval_inputs
        .iter()
        .flat_map(|&(_, v)| (0..bits).map(move |i| (v >> i) & 1 == 1))
        .collect();

    // ---- OT for all evaluator input bits ----
    let a_bytes: Vec<[u8; 32]> = network.recv(0).await?;
    eprintln!("[evaluator] OT round 1 received ({} A-points)", a_bytes.len());

    let (ot_receiver, b_bytes) = OTReceiver::choose(&a_bytes, &choices);
    network.send(0, &b_bytes).await?;
    eprintln!("[evaluator] OT round 2 sent");

    // Receive garbled circuit bundle (includes OT ciphertexts for our labels).
    let msg: GarblerMsg = network.recv(0).await?;
    eprintln!("[evaluator] received garbled circuit ({} gates)", msg.garbled_circuit.gates.len());

    // Decrypt our input labels.
    let my_labels: Vec<u128> = ot_receiver.finish(&msg.ot_ciphertexts);

    // Reconstruct evaluator's active label map across all owned wires.
    let mut eval_active: HashMap<String, u128> = HashMap::new();
    for (wire_idx, &(wire, _)) in eval_inputs.iter().enumerate() {
        for bit_idx in 0..bits {
            let name = format!("w{}_b{}", wire.0, bit_idx);
            eval_active.insert(name, my_labels[wire_idx * bits + bit_idx]);
        }
    }

    // Merge with garbler's active labels and evaluate.
    let mut active_labels = msg.garbler_active_labels;
    active_labels.extend(eval_active);
    let results = msg.garbled_circuit.evaluate(active_labels);

    // Decode all output wires bit by bit.
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

    // Send all decoded values to garbler and print locally.
    network.send(0, &output_values).await?;
    for (i, v) in output_values.iter().enumerate() {
        println!("output[{i}]: {v}");
    }
    Ok(())
}
