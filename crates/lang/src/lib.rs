pub mod ast;
pub mod colorwork_dsl;
pub mod error;
pub mod eval;
pub mod lexer;
pub mod parser;
pub mod raw_def;

pub use ast::Pattern;
pub use colorwork_dsl::color_grid_to_dsl;
pub use error::ParseError;
pub use raw_def::{RawAttachRef, RawOp};

use abyssal_thread_core::StitchGraph;

/// Convenience: parse DSL source straight to a `StitchGraph`.
pub fn compile(src: &str) -> Result<StitchGraph, ParseError> {
    let pattern = parser::parse(src)?;
    eval::eval(&pattern)
}
