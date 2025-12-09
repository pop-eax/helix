# Helix - Multi-Party Computation Framework

Helix is a framework for Multi-Party Computation (MPC) that provides a domain-specific language (DSL) for writing secure computation programs. The DSL compiles through multiple intermediate representations (IR) to gate-level circuits that can be executed by various MPC backends.

## Features

- **DSL for MPC**: Write secure computation programs in a high-level language
- **Type System**: Static type checking with support for public and secret values
- **Two-Level IR**: High-Level IR (HIR) for optimizations and Low-Level IR (LIR) for backend compilation
- **Virtual Machine**: Minimal instruction set for gate-level operations
- **Backend Abstraction**: Pluggable backends (currently includes a clear/non-cryptographic backend)
- **Compiler Toolchain**: Full compilation pipeline from source to executable circuits

## Project Structure

```
helix/
├── crates/
│   ├── frontend/      # Parser, AST, type checker, and HIR codegen
│   ├── ir/            # HIR, LIR, and lowering passes
│   ├── backend/       # VM, backends, compiler, and executor
│   ├── common/        # Shared utilities
│   └── crypto/        # Cryptographic primitives (future)
├── bin/
│   └── compiler/      # Helix compiler CLI (helixc)
└── tests/
    └── samples/       # Example MPC programs
```

## Installation

### Prerequisites

- Rust 1.70+ (edition 2021)
- Cargo

### Building

```bash
# Clone the repository
git clone https://github.com/pop-eax/helix.git
cd helix

# Build the project
cargo build

# Build in release mode
cargo build --release
```

### Installing the Compiler

```bash
# Or use directly
cargo run --bin helixc -- --help
```

## Quick Start

### Writing Your First Program

Create a file `hello.mpc`:

```rust
// Simple addition function
fn add(Public Field<64> a, Public Field<64> b) -> Field<64> {
    return a + b;
}
```

### Compiling

```bash
# Compile to IR
cargo run --bin helixc -- compile hello.mpc

# View AST
cargo run --bin helixc -- ast hello.mpc

# View HIR
cargo run --bin helixc -- hir hello.mpc

# View LIR
cargo run --bin helixc -- lir hello.mpc

# Compile to VM instructions
cargo run --bin helixc -- vm hello.mpc

# Execute with inputs
cargo run --bin helixc -- execute hello.mpc --inputs "5,10"
```

## Language Reference

### Types

- **Field<N>**: Finite field element with N-bit size (e.g., `Field<64>`)
- **Visibility Modifiers**:
  - `Public`: Value is known to all parties
  - `Secret`: Value is secret-shared or encrypted

### Functions

```rust
fn function_name(Visibility Type param1, Visibility Type param2) -> ReturnType {
    // function body
    return expression;
}
```

### Statements

- **Variable Declaration**: `let Visibility Type name = expression;`
- **Assignment**: `lvalue = expression;`
- **Return**: `return expression;`
- **Conditional**: `if (condition) { ... } else { ... }`
- **Loops**: `for (let Visibility Type name = start; name < end; name = name + 1) { ... }`

### Expressions

- **Arithmetic**: `+`, `-`, `*`, `/`, `%`
- **Comparison**: `==`, `!=`, `<`, `<=`, `>`, `>=`
- **Boolean**: `&&`, `||`, `!`
- **Function Calls**: `function_name(arg1, arg2)`

### Example Programs

#### Addition

```rust
fn add(Public Field<64> a, Public Field<64> b) -> Field<64> {
    return a + b;
}
```

#### Multiplication with Secret Input

```rust
fn multiply(Public Field<64> x, Secret Field<64> y) -> Field<64> {
    return x * y;
}
```

#### Conditional Logic

```rust
fn max(Public Field<64> a, Public Field<64> b) -> Field<64> {
    if (a > b) {
        return a;
    } else {
        return b;
    }
}
```

#### Structs

```rust
struct Point {
    Public Field<64> x;
    Public Field<64> y;
}

fn create_point(Public Field<64> x, Public Field<64> y) -> Point {
    return Point { x: x, y: y };
}
```

## Compiler Commands

The `helixc` compiler provides several commands:

### `compile`
Compile MPC source file to IR (binary format)

```bash
helixc compile input.mpc [-o output.ir]
```

### `vm`
Compile MPC source file to VM instructions (JSON format)

```bash
helixc vm input.mpc [-o output.vm.json]
```

### `execute`
Execute MPC program with given inputs

```bash
helixc execute input.mpc --inputs "5,10,15"
```

### `ast`
Display the Abstract Syntax Tree (JSON format)

```bash
helixc ast input.mpc
```

### `hir`
Display the High-Level Intermediate Representation

```bash
helixc hir input.mpc
```

### `lir`
Display the Low-Level Intermediate Representation

```bash
helixc lir input.mpc
```

### `debug`
Show all intermediate representations (AST, HIR, LIR)

```bash
helixc debug input.mpc
```

## Architecture

### Compilation Pipeline

```
Source Code (.mpc)
    ↓
[Parser] → AST
    ↓
[Type Checker] → Validated AST
    ↓
[Codegen] → HIR (High-Level IR)
    ↓
[Lowering] → LIR (Low-Level IR)
    ↓
[VM Compiler] → VM Instructions
    ↓
[Backend] → Execution
```

### Frontend (`crates/frontend`)

- **Parser**: Pest-based parser for the MPC DSL
- **AST**: Abstract Syntax Tree representation
- **Type Checker**: Static type checking with visibility analysis
- **Codegen**: AST to HIR translation

### Intermediate Representation (`crates/ir`)

- **HIR**: High-Level IR with SSA-like value representation and basic blocks
- **LIR**: Low-Level IR with gate-level circuit representation
- **Lowering**: HIR to LIR conversion pass

### Backend (`crates/backend`)

- **VM**: Virtual machine with minimal instruction set
- **Clear Backend**: Non-cryptographic backend for testing and debugging
- **Compiler**: LIR to VM instructions compilation
- **Executor**: Generic executor for running VM instructions

### VM Instruction Set

The VM provides a minimal set of gate instructions:

**Boolean Gates:**
- `And`, `Xor`, `Not` (OR can be derived from these)

**Arithmetic Gates:**
- `Add`, `Mul`, `Sub`, `Div`, `Mod`

**Constants:**
- `Constant { value, field_size }`

Each instruction includes visibility information for MPC backends.

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p frontend
cargo test -p ir
cargo test -p backend

# Run integration tests
cargo test --test integration_test
```

### Sample Programs

Sample programs are located in `tests/samples/`:

- `add.mpc` - Simple addition
- `multiply.mpc` - Multiplication with secret input
- `arithmetic.mpc` - Complex arithmetic operations
- `comparison.mpc` - Comparison operations
- `conditional.mpc` - Conditional logic
- `loop.mpc` - Loop constructs
- `struct.mpc` - Struct definitions and usage

## Design Decisions

### Party Assignment

Party assignment is handled by the VM/executor at runtime, not during compilation. This allows:
- The same circuit to be executed with different party configurations
- Backend-agnostic LIR representation
- Flexible MPC protocol implementations

### Two-Level IR

- **HIR**: Optimized for high-level transformations and optimizations
- **LIR**: Optimized for backend compilation and gate-level operations

### Visibility in Types

Visibility (Public/Secret) is part of the type system, enabling:
- Static analysis of information flow
- Type checking for MPC correctness
- Clear separation between public and secret values

## Future Work

- [ ] Yao's Garbled Circuits backend
- [ ] BGW (Ben-Or, Goldwasser, Wigderson) arithmetic secret sharing backend
- [ ] More optimization passes (constant folding, dead code elimination, etc.)
- [ ] Array operations
- [ ] More complex control flow optimizations
- [ ] Cryptographic primitives integration
- [ ] Network protocol for distributed execution

## License

MIT

## Acknowledgments

