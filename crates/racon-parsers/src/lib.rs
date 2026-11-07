//! RACoN Parsers — PNML, PNT, and MercuryScript format parsers.
//!
//! Each parser implements a trait that consumes raw text input and
//! produces a validated `PetriNet` from `racon-core`.

pub mod mercury;
pub mod pnml;
pub mod pnt;

// Re-export traits, implementations, and configs
pub use mercury::{MercuryParseResult, MercuryParser, MercuryParserConfig, MercuryParserImpl};
pub use pnml::{PnmlParser, PnmlParserConfig, PnmlParserImpl};
pub use pnt::{petri_net_to_ina_string, PntParser, PntParserConfig, PntParserImpl};
