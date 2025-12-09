// Helix MPC Compiler CLI

use clap::{Parser, Subcommand};
use frontend::{parse_and_check, parse_and_codegen, FrontendError};
use ir::{lower_hir_to_lir, Metadata, Statistics};
use backend::{compile_to_vm_instructions, execute_program, ClearBackend};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "helixc")]
#[command(about = "Helix MPC Compiler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile MPC source file to IR
    Compile {
        /// Input source file
        input: PathBuf,
        /// Output file (optional, defaults to input with .ir extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compile MPC source file to VM instructions
    Vm {
        /// Input source file
        input: PathBuf,
        /// Output file (optional, defaults to input with .vm.json extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Execute MPC program with given inputs
    Execute {
        /// Input source file
        input: PathBuf,
        /// Input values as comma-separated list (e.g., "5,10")
        #[arg(short, long)]
        inputs: String,
    },
    /// Show AST representation
    Ast {
        /// Input source file
        input: PathBuf,
    },
    /// Show HIR representation
    Hir {
        /// Input source file
        input: PathBuf,
    },
    /// Show LIR representation
    Lir {
        /// Input source file
        input: PathBuf,
    },
    /// Show all intermediate representations
    Debug {
        /// Input source file
        input: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { input, output } => {
            compile(input, output).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
        }
        Commands::Vm { input, output } => {
            compile_to_vm(input, output).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
        }
        Commands::Execute { input, inputs } => {
            execute(input, inputs).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
        }
        Commands::Ast { input } => {
            show_ast(input).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
        }
        Commands::Hir { input } => {
            show_hir(input).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
        }
        Commands::Lir { input } => {
            show_lir(input).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
        }
        Commands::Debug { input } => {
            debug_all(input).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
        }
    }
}

fn compile(input: PathBuf, output: Option<PathBuf>) -> Result<(), FrontendError> {
    let source = fs::read_to_string(&input)?;
    
    // Parse, type check, and generate HIR
    let hir = parse_and_codegen(&source)?;
    
    // Lower to LIR
    let metadata = create_metadata(&input);
    let lir = lower_hir_to_lir(&hir, metadata)
        .map_err(|e| FrontendError::LoweringError(e.to_string()))?;
    
    // Determine output path
    let output_path = output.unwrap_or_else(|| {
        input.with_extension("ir")
    });
    
    // Serialize and write LIR
    let bytes = lir.to_bytes().map_err(|e| FrontendError::IoError(std::io::Error::new(std::io::ErrorKind::Other, format!("Serialization error: {}", e))))?;
    fs::write(&output_path, bytes)?;
    
    println!("Compiled {} to {}", input.display(), output_path.display());
    Ok(())
}

fn show_ast(input: PathBuf) -> Result<(), FrontendError> {
    let source = fs::read_to_string(&input)?;
    let ast = parse_and_check(&source)?;
    
    println!("=== AST ===");
    println!("{}", frontend::display_program(&ast));
    Ok(())
}

fn show_hir(input: PathBuf) -> Result<(), FrontendError> {
    let source = fs::read_to_string(&input)?;
    let hir = parse_and_codegen(&source)?;
    
    println!("=== HIR ===");
    println!("{}", ir::hir_display::display_hir_program(&hir));
    Ok(())
}

fn show_lir(input: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(&input)?;
    let hir = parse_and_codegen(&source)?;
    
    let metadata = create_metadata(&input);
    let lir = lower_hir_to_lir(&hir, metadata)?;
    
    println!("=== LIR ===");
    println!("{}", ir::lir_display::display_lir_program(&lir));
    Ok(())
}

fn debug_all(input: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(&input)?;
    
    // AST
    println!("=== AST ===");
    let ast = parse_and_check(&source)?;
    println!("{}", frontend::display_program(&ast));
    println!();
    
    // HIR
    println!("=== HIR ===");
    let hir = parse_and_codegen(&source)?;
    println!("{}", ir::hir_display::display_hir_program(&hir));
    println!();
    
    // LIR
    println!("=== LIR ===");
    let metadata = create_metadata(&input);
    let lir = lower_hir_to_lir(&hir, metadata).map_err(|e| format!("Lowering error: {}", e))?;
    println!("{}", ir::lir_display::display_lir_program(&lir));
    
    Ok(())
}

fn compile_to_vm(input: PathBuf, output: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(&input).map_err(|e| format!("IO error: {}", e))?;
    let hir = parse_and_codegen(&source).map_err(|e| format!("Frontend error: {}", e))?;
    
    let metadata = create_metadata(&input);
    let lir = lower_hir_to_lir(&hir, metadata).map_err(|e| format!("Lowering error: {}", e))?;
    
    // Compile to VM instructions
    let instructions = compile_to_vm_instructions(&lir.circuit);
    
    // Determine output path
    let output_path = output.unwrap_or_else(|| {
        input.with_extension("vm.json")
    });
    
    // Serialize to JSON
    let json = serde_json::to_string_pretty(&instructions)?;
    fs::write(&output_path, json)?;
    
    println!("Compiled {} to VM instructions: {}", input.display(), output_path.display());
    println!("Total instructions: {}", instructions.len());
    Ok(())
}

fn execute(input: PathBuf, inputs_str: String) -> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(&input).map_err(|e| format!("IO error: {}", e))?;
    let hir = parse_and_codegen(&source).map_err(|e| format!("Frontend error: {}", e))?;
    
    let metadata = create_metadata(&input);
    let lir = lower_hir_to_lir(&hir, metadata).map_err(|e| format!("Lowering error: {}", e))?;
    
    // Parse input values
    let input_values: Vec<u64> = inputs_str
        .split(',')
        .map(|s| s.trim().parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Invalid input format: {}. Expected comma-separated numbers.", e))?;
    
    // Map input values to wire IDs and assign parties
    // For simplicity, assign each input to a different party (party 0, 1, 2, ...)
    // In a real MPC scenario, the user would specify which party owns which input
    let mut input_wires = Vec::new();
    for (i, input_def) in lir.circuit.inputs.iter().enumerate() {
        if i < input_values.len() {
            let party = ir::lir::PartyId(i);
            input_wires.push((input_def.wire, party, input_values[i]));
        } else {
            return Err(format!("Not enough input values. Expected {} inputs, got {}", 
                lir.circuit.inputs.len(), input_values.len()).into());
        }
    }
    
    if input_values.len() > lir.circuit.inputs.len() {
        return Err(format!("Too many input values. Expected {} inputs, got {}", 
            lir.circuit.inputs.len(), input_values.len()).into());
    }
    
    // Create clear backend
    let mut backend = ClearBackend::new(lir.metadata.field_modulus);
    
    // Execute program
    println!("Executing program with inputs: {:?}", input_values);
    let outputs = execute_program(&lir, &mut backend, &input_wires)
        .map_err(|e| format!("Execution error: {}", e))?;
    
    // Display results
    println!("\n=== Execution Results ===");
    for (wire, value) in &outputs {
        // Find the output name if available
        let output_name = lir.circuit.outputs.iter()
            .position(|w| *w == *wire)
            .map(|i| format!("output[{}]", i))
            .unwrap_or_else(|| format!("wire{:?}", wire));
        println!("{}: {}", output_name, value);
    }
    
    Ok(())
}

fn create_metadata(input: &PathBuf) -> Metadata {
    Metadata {
        version: "1.0".to_string(),
        source_file: input.display().to_string(),
        function_name: "main".to_string(), // Default, could be extracted from AST
        field_modulus: Some(2_u64.pow(63) - 1),
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

