pub mod error;
pub mod parser;
pub mod translate;

pub use error::DslParseError;
pub use parser::{parse_dsl, DslCompOp, DslExpr, DslQuantifier, DslTemporalOp, Parser, Tokenizer};
pub use translate::Translator;

use racon_core::net::PetriNet;
use racon_ctl::CTLFormula;

/// Parse a CTL DSL query string into a formal CTLFormula AST.
/// Validates place names against the given PetriNet.
pub fn parse_ctl_query(query: &str, net: &PetriNet) -> Result<CTLFormula, DslParseError> {
    let dsl_expr = parser::parse_dsl(query)?;
    Translator::translate(&dsl_expr, net)
}
