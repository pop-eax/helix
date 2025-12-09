pub mod ast;
pub mod codegen;
pub mod display;
pub mod parser;
pub mod type_checker;

pub use ast::Program;
pub use codegen::{codegen, CodegenError};
pub use display::display_program;
pub use parser::{build_ast, parse_program};
pub use type_checker::{type_check, TypeError, TypeCheckResult};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrontendError {
    #[error("Parse error: {0}")]
    ParseError(#[from] pest::error::Error<parser::Rule>),
    
    #[error("AST build error: {0}")]
    AstBuildError(String),
    
    #[error("Type check error: {0}")]
    TypeCheckError(#[from] TypeError),
    
    #[error("Codegen error: {0}")]
    CodegenError(#[from] CodegenError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Lowering error: {0}")]
    LoweringError(String),
}

/// Parse source code, build AST, and type check it
pub fn parse_and_check(source: &str) -> Result<Program, FrontendError> {
    let pairs = parse_program(source)?;
    let program = build_ast(pairs).map_err(FrontendError::AstBuildError)?;
    type_check(&program)?;
    Ok(program)
}

/// Parse source code, build AST, type check, and generate HIR
pub fn parse_and_codegen(source: &str) -> Result<ir::hir::HirProgram, FrontendError> {
    let program = parse_and_check(source)?;
    let hir = codegen(&program)?;
    Ok(hir)
}

