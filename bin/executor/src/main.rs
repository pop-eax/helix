use clap::Parser;
use ir::lir::{PartyId, Program, WireId};
use runtime::execute_program;
use std::{fs, path::PathBuf};

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
}

fn main() {
    let cli = Cli::parse();
    run(cli).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load compiled circuit
    let bytes = fs::read(&cli.circuit)?;
    let program = Program::from_bytes(&bytes)
        .map_err(|e| format!("Failed to deserialize circuit: {e}"))?;

    // 2. Parse inputs
    let input_values: Vec<u64> = cli
        .inputs
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

    // 3. Build (WireId, PartyId, u64) input list
    let input_wires: Vec<(WireId, PartyId, u64)> = program
        .circuit
        .inputs
        .iter()
        .zip(input_values.iter())
        .enumerate()
        .map(|(i, (inp, &val))| (inp.wire, PartyId(i), val))
        .collect();

    // 4. Dispatch to chosen backend and execute
    let outputs = match cli.backend.as_str() {
        "clear" => {
            let mut b = runtime::ClearBackend::new(program.metadata.field_modulus);
            execute_program(&program, &mut b, &input_wires)?
        }
        "yao" => {
            let mut b = garbledc::backend::YaoBackend::new(cli.bits);
            execute_program(&program, &mut b, &input_wires)?
        }
        "bgw" => {
            let parties = cli.parties.ok_or("--parties required for bgw")?;
            let threshold = cli.threshold.ok_or("--threshold required for bgw")?;
            let cfg = bgw::BgwConfig { parties, threshold };
            let mut b = bgw::BgwBackend::new(cfg)?;
            execute_program(&program, &mut b, &input_wires)?
        }
        other => return Err(format!("Unknown backend '{other}'. Use: clear, yao, bgw").into()),
    };

    // 5. Print results
    for (i, (_wire, value)) in outputs.iter().enumerate() {
        println!("output[{i}]: {value}");
    }

    Ok(())
}
