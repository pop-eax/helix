use std::{fs, path::PathBuf};

use clap::Parser;
use ir::lir::{PartyId, Program, WireId};
use runtime::execute_program;
use serde::{Deserialize, Serialize};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "runner", about = "Helix MPC circuit runner")]
struct Cli {
    /// Path to the compiled circuit (.ir file).
    circuit: PathBuf,

    /// Comma-separated input values, e.g. "1,2,3".
    /// For networked backends use '_' for inputs you don't own: "1,_,_".
    /// Cannot be combined with --inputs-file.
    #[arg(short, long, conflicts_with = "inputs_file")]
    inputs: Option<String>,

    /// Path to a TOML file describing inputs and backend configuration.
    /// Cannot be combined with --inputs.
    #[arg(long, conflicts_with = "inputs")]
    inputs_file: Option<PathBuf>,

    /// Backend to use.  Overrides the value in --inputs-file if both are given.
    #[arg(short, long)]
    backend: Option<String>,

    /// Bit-width for Yao backend (default 8).
    /// Overrides the value in --inputs-file.
    #[arg(long)]
    bits: Option<usize>,

    /// Number of parties for BGW.  Overrides --inputs-file.
    #[arg(long)]
    parties: Option<usize>,

    /// Threshold for BGW.  Overrides --inputs-file.
    #[arg(long)]
    threshold: Option<usize>,

    /// This party's ID (0-based) — required for networked backends.
    /// Overrides --inputs-file.
    #[arg(long)]
    my_id: Option<usize>,

    /// Comma-separated list of party addresses (host:port), one per party —
    /// required for networked backends (e.g. "127.0.0.1:7000,127.0.0.1:7001").
    /// Overrides --inputs-file.
    #[arg(long)]
    party_addrs: Option<String>,

    /// Enable SHA256 commitment verification for input shares (BGW backends only).
    #[arg(long)]
    commit: bool,
}

// ── TOML config structs ───────────────────────────────────────────────────────

/// Top-level structure of an inputs TOML file.
///
/// Example (single-process):
/// ```toml
/// [backend]
/// type = "clear"
///
/// [inputs]
/// values = [10, 20, 30]
/// ```
///
/// Example (networked BGW, party 0 of 3):
/// ```toml
/// [backend]
/// type  = "bgw-np"
/// parties   = 3
/// threshold = 2
/// my_id     = 0
///
/// [network]
/// party_addrs = ["127.0.0.1:7000", "127.0.0.1:7001", "127.0.0.1:7002"]
///
/// [inputs]
/// spec = "10,_,_"
/// ```
#[derive(Deserialize, Default)]
#[serde(default)]
struct InputsFile {
    backend: BackendSection,
    network: NetworkSection,
    inputs:  InputsSection,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BackendSection {
    /// "clear" | "yao" | "bgw" | "bgw-np" | "yao-2p"
    #[serde(rename = "type")]
    backend_type: Option<String>,
    bits:      Option<usize>,
    parties:   Option<usize>,
    threshold: Option<usize>,
    my_id:     Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct NetworkSection {
    party_addrs: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct InputsSection {
    /// Simple positional list of u64 values.  Use for single-process backends
    /// or when all inputs belong to this party.
    values: Option<Vec<u64>>,

    /// Full input spec string — same syntax as the --inputs CLI flag.
    /// Use '_' for inputs not owned by this party (networked backends).
    /// If both `values` and `spec` are given, `spec` takes precedence.
    spec: Option<String>,
}

impl InputsSection {
    /// Return the effective input spec string, converting `values` to a
    /// comma-separated string when only `values` is present.
    fn resolve(&self) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(s) = &self.spec {
            return Ok(s.clone());
        }
        if let Some(vs) = &self.values {
            return Ok(vs.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","));
        }
        Err("inputs file must contain either `inputs.values` or `inputs.spec`".into())
    }
}

// ── Resolved config ───────────────────────────────────────────────────────────

/// All settings after merging the TOML file with CLI overrides.
/// CLI flags always win.
struct Config {
    inputs_str: String,
    backend:    String,
    bits:       usize,
    parties:    Option<usize>,
    threshold:  Option<usize>,
    my_id:      Option<usize>,
    party_addrs: Option<String>,
}

impl Config {
    fn build(cli: &Cli) -> Result<Self, Box<dyn std::error::Error>> {
        // Load TOML file if given.
        let file: InputsFile = match &cli.inputs_file {
            Some(path) => {
                let raw = fs::read_to_string(path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                toml::from_str(&raw)
                    .map_err(|e| format!("invalid TOML in {}: {e}", path.display()))?
            }
            None => InputsFile::default(),
        };

        // Resolve the input spec string.  CLI --inputs beats file.
        let inputs_str = if let Some(s) = &cli.inputs {
            s.clone()
        } else {
            file.inputs.resolve()?
        };

        // Merge backend fields: CLI overrides file.
        let backend = cli
            .backend
            .clone()
            .or(file.backend.backend_type)
            .unwrap_or_else(|| "clear".to_string());

        let bits = cli.bits.or(file.backend.bits).unwrap_or(8);

        let parties = cli.parties.or(file.backend.parties);
        let threshold = cli.threshold.or(file.backend.threshold);
        let my_id = cli.my_id.or(file.backend.my_id);

        // Party addresses: CLI --party-addrs (comma-separated string) beats
        // TOML network.party_addrs (Vec<String>).
        let party_addrs = cli.party_addrs.clone().or_else(|| {
            file.network
                .party_addrs
                .map(|v| v.join(","))
        });

        Ok(Config { inputs_str, backend, bits, parties, threshold, my_id, party_addrs })
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if cli.inputs.is_none() && cli.inputs_file.is_none() {
        return Err("one of --inputs or --inputs-file is required".into());
    }

    let bytes = fs::read(&cli.circuit)?;
    let program = Program::from_bytes(&bytes)
        .map_err(|e| format!("failed to deserialize circuit: {e}"))?;

    let cfg = Config::build(&cli)?;

    match cfg.backend.as_str() {
        "clear" => {
            let outputs = run_single_process_clear(&program, &cfg.inputs_str)?;
            print_outputs(&outputs);
        }
        "yao" => {
            let outputs = run_single_process_yao(&program, &cfg.inputs_str, cfg.bits)?;
            print_outputs(&outputs);
        }
        "bgw" => {
            let parties   = cfg.parties  .ok_or("--parties (or backend.parties in TOML) required for bgw")?;
            let threshold = cfg.threshold.ok_or("--threshold (or backend.threshold in TOML) required for bgw")?;
            let outputs = run_single_process_bgw(&program, &cfg.inputs_str, parties, threshold, cli.commit)?;
            print_outputs(&outputs);
        }
        "bgw-np" => {
            let my_id     = cfg.my_id    .ok_or("--my-id (or backend.my_id in TOML) required for bgw-np")?;
            let parties   = cfg.parties  .ok_or("--parties (or backend.parties in TOML) required for bgw-np")?;
            let threshold = cfg.threshold.ok_or("--threshold (or backend.threshold in TOML) required for bgw-np")?;
            let addrs_str = cfg.party_addrs.ok_or("--party-addrs (or network.party_addrs in TOML) required for bgw-np")?;
            run_bgw_networked(&program, &cfg.inputs_str, my_id, parties, threshold, &addrs_str, cli.commit).await?;
        }
        "yao-2p" => {
            let my_id     = cfg.my_id.ok_or("--my-id (or backend.my_id in TOML) required for yao-2p")?;
            let addrs_str = cfg.party_addrs.ok_or("--party-addrs (or network.party_addrs in TOML) required for yao-2p")?;
            run_yao_two_party(&program, &cfg.inputs_str, cfg.bits, my_id, &addrs_str).await?;
        }
        other => return Err(format!("unknown backend '{other}'. Use: clear, yao, bgw, yao-2p, bgw-np").into()),
    }

    Ok(())
}

// ── Single-process helpers ────────────────────────────────────────────────────

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
    commit: bool,
) -> Result<Vec<(WireId, u64)>, Box<dyn std::error::Error>> {
    let input_wires = parse_inputs(program, inputs_str)?;
    let cfg = bgw::BgwConfig { parties, threshold };
    let field = bgw::PrimeField::new(program.metadata.field_modulus.unwrap_or((1u64 << 63) - 1));
    let mut b = bgw::BgwBackend::new(cfg, field)?;
    if commit {
        b = b.with_commits();
    }
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
        .map_err(|e| format!("invalid input: {e}"))?;

    let n_inputs = program.circuit.inputs.len();
    if input_values.len() != n_inputs {
        return Err(format!(
            "circuit expects {n_inputs} inputs, got {}",
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

// ── Shared input-spec parser (networked backends) ─────────────────────────────

/// Parse a comma-separated input spec like `"1,2,_,_"` into `InputAssignment`s.
///
/// `_` means "I don't own this wire".  Ownership falls back to
/// `floor(i * n_parties / n_inputs)` when no circuit party annotation is present.
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
            wire:  inp.wire,
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

// ── n-party BGW (networked) ───────────────────────────────────────────────────

async fn run_bgw_networked(
    program: &Program,
    input_spec: &str,
    my_id: usize,
    parties: usize,
    threshold: usize,
    addrs_str: &str,
    commit: bool,
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

    let field = bgw::PrimeField::new(program.metadata.field_modulus.unwrap_or((1u64 << 63) - 1));
    let n_muls = bgw::count_multiplications(program);
    let my_triple_blob: Vec<u8> = if my_id == 0 {
        let blobs = bgw::dealer_generate_triple_blobs(n_muls, parties, threshold, &field);
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
    let mut backend = bgw::BgwNetBackend::new(my_id, parties, threshold, field, triple_shares)
        .map_err(|e| format!("bgw backend: {e}"))?;
    if commit {
        backend = backend.with_commits();
    }

    let inputs = parse_input_spec(program, input_spec, my_id, parties)?;

    let mut runner = runtime::Runner::new(network, backend, program.clone(), &inputs)?;
    let outputs = runner.run().await?;
    for (i, (_wire, value)) in outputs.iter().enumerate() {
        println!("output[{i}]: {value}");
    }
    Ok(())
}

// ── 2-party Yao (networked) ───────────────────────────────────────────────────

/// Message sent from garbler (party 0) to evaluator (party 1).
#[derive(Serialize, Deserialize)]
struct GarblerMsg {
    garbled_circuit:        garbledc::circuit::Circuit,
    /// Active labels indexed by slot (None for evaluator-owned slots).
    garbler_active_labels:  Vec<Option<u128>>,
    /// Decode table indexed by slot: lsb(label₀) for each output slot.
    output_label_pairs:     Vec<u8>,
    /// Base slot for each evaluator input wire, in the same order as the OT messages.
    eval_wire_base_slots:   Vec<usize>,
    /// Base slot for each LIR output wire, in the same order as program.circuit.outputs.
    output_wire_base_slots: Vec<usize>,
    ot_ciphertexts:         Vec<(u128, u128)>,
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

async fn garbler_run(
    program: &Program,
    inputs: &[runtime::InputAssignment],
    bits: usize,
    network: &mut net::Network,
) -> Result<(), Box<dyn std::error::Error>> {
    use runtime::{compile_to_vm_instructions, vm::{VMState, Backend}};
    use garbledc::ot::OTSender;

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

    for &(wire, value) in &garbler_inputs {
        backend.set_input(wire, value, runtime::Visibility::Secret, &mut state)?;
    }

    let mut ot_messages: Vec<(u128, u128)> = Vec::new();
    let mut eval_wire_base_slots: Vec<usize> = Vec::new();
    for &wire in &eval_wires {
        backend.register_evaluator_wire(wire);
        eval_wire_base_slots.push(backend.wire_base_slot(wire));
        for bit_idx in 0..bits {
            let [l0, l1] = backend
                .wire_label_pair(wire, bit_idx)
                .expect("evaluator wire labels must exist after register_evaluator_wire");
            ot_messages.push((l0, l1));
        }
    }

    let n_ot = ot_messages.len();
    let (ot_sender, a_bytes) = OTSender::setup(n_ot);
    network.send(1, &a_bytes).await?;
    eprintln!("[garbler] OT round 1 sent ({n_ot} A-points)");

    let b_bytes: Vec<[u8; 32]> = network.recv(1).await?;
    eprintln!("[garbler] OT round 2 received");

    let ot_ciphertexts = ot_sender.respond(&b_bytes, &ot_messages);

    let (garbled_circuit, garbler_active_labels, output_label_pairs) =
        backend.finalize_garbler();

    let output_wire_base_slots: Vec<usize> = program.circuit.outputs.iter()
        .map(|&w| backend.wire_base_slot(w))
        .collect();

    network
        .send(
            1,
            &GarblerMsg {
                garbled_circuit,
                garbler_active_labels,
                output_label_pairs,
                eval_wire_base_slots,
                output_wire_base_slots,
                ot_ciphertexts,
            },
        )
        .await?;
    eprintln!("[garbler] sent garbled circuit bundle");

    let results: Vec<u64> = network.recv(1).await?;
    for (i, v) in results.iter().enumerate() {
        println!("output[{i}]: {v}");
    }
    Ok(())
}

async fn evaluator_run(
    program: &Program,
    inputs: &[runtime::InputAssignment],
    bits: usize,
    network: &mut net::Network,
) -> Result<(), Box<dyn std::error::Error>> {
    use garbledc::ot::OTReceiver;

    let eval_inputs: Vec<(WireId, u64)> = inputs
        .iter()
        .filter(|a| a.owner == 1)
        .map(|a| (a.wire, a.value.unwrap()))
        .collect();

    let choices: Vec<bool> = eval_inputs
        .iter()
        .flat_map(|&(_, v)| (0..bits).map(move |i| (v >> i) & 1 == 1))
        .collect();

    let a_bytes: Vec<[u8; 32]> = network.recv(0).await?;
    eprintln!("[evaluator] OT round 1 received ({} A-points)", a_bytes.len());

    let (ot_receiver, b_bytes) = OTReceiver::choose(&a_bytes, &choices);
    network.send(0, &b_bytes).await?;
    eprintln!("[evaluator] OT round 2 sent");

    let msg: GarblerMsg = network.recv(0).await?;
    eprintln!("[evaluator] received garbled circuit ({} gates)", msg.garbled_circuit.gates.len());

    let my_labels: Vec<u128> = ot_receiver.finish(&msg.ot_ciphertexts);

    // Fill evaluator-owned slots into the active-label Vec.
    let mut active_labels = msg.garbler_active_labels;
    for (wire_idx, &base_slot) in msg.eval_wire_base_slots.iter().enumerate() {
        for bit_idx in 0..bits {
            let slot = base_slot + bit_idx;
            if slot < active_labels.len() {
                active_labels[slot] = Some(my_labels[wire_idx * bits + bit_idx]);
            }
        }
    }

    let results = msg.garbled_circuit.evaluate(active_labels);

    // Decode output values using the base slots sent by the garbler.
    let mut output_values: Vec<u64> = Vec::new();
    for &base_slot in &msg.output_wire_base_slots {
        let mut value = 0u64;
        for bit_idx in 0..bits {
            let slot = base_slot + bit_idx;
            if let (Some(active), Some(&decode_bit)) = (
                results.get(slot).copied().flatten(),
                msg.output_label_pairs.get(slot),
            ) {
                value |= (((active & 1) ^ decode_bit as u128) as u64) << bit_idx;
            }
        }
        output_values.push(value);
    }

    network.send(0, &output_values).await?;
    for (i, v) in output_values.iter().enumerate() {
        println!("output[{i}]: {v}");
    }
    Ok(())
}
