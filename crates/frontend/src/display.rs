// Display/formatting utilities for AST

use crate::ast::Program;
use serde_json;

/// Display the AST as a formatted JSON string
pub fn display_program(program: &Program) -> String {
    serde_json::to_string_pretty(program)
        .unwrap_or_else(|e| format!("Error serializing AST to JSON: {}", e))
}

