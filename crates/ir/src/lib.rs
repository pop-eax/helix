// IR crate - Intermediate Representation for MPC framework
// Contains both High-Level IR (HIR) and Low-Level IR (LIR)

pub mod hir;
pub mod lir;
pub mod lowering;

// Re-export commonly used types
pub use lir::*;
pub use lowering::{lower_hir_to_lir, LoweringError, LoweringResult};
