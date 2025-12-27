// Backend crate - Virtual Machine and Backend implementations

pub mod clear;
pub mod compiler;
pub mod executor;
pub mod vm;

pub use clear::ClearBackend;
pub use compiler::compile_to_vm_instructions;
pub use executor::execute_program;
pub use vm::{Backend, BackendError, Instruction, VMState, VisibilityPair, WireValue};
pub use ir::{Visibility};