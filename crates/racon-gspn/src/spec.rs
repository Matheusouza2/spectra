use racon_core::net::TransitionId;
use std::collections::HashMap;

/// GSPN specification: maps each transition to its GSPN type.
#[derive(Debug, Clone)]
pub struct GspnSpec {
    pub transition_types: HashMap<TransitionId, TransitionType>,
}

/// Classification of a transition in GSPN semantics.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionType {
    /// Timed transition with exponentially distributed firing delay (rate λ).
    Timed { rate: f64 },
    /// Immediate transition (zero firing time).
    Immediate { weight: f64, priority: u32 },
}

impl Default for TransitionType {
    fn default() -> Self {
        TransitionType::Immediate {
            weight: 1.0,
            priority: 0,
        }
    }
}

/// Errors specific to GSPN analysis.
#[derive(Debug, Clone)]
pub enum GspnError {
    NoTimedTransitions,
    AllVanishingMarkings,
    NoConvergence { iterations: u32, residual: f64 },
    SingularMatrix,
    StateSpaceTooLarge { states: usize, max: usize },
    InvalidRate(String),
    InvalidWeight(String),
    InvalidPriority(String),
    MissingTransitionType(String),
}

impl std::fmt::Display for GspnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTimedTransitions => write!(f, "net has no timed transitions"),
            Self::AllVanishingMarkings => write!(f, "all markings are vanishing"),
            Self::NoConvergence {
                iterations,
                residual,
            } => write!(
                f,
                "steady-state solver did not converge after {} iterations (residual={})",
                iterations, residual
            ),
            Self::SingularMatrix => write!(f, "CTMC generator matrix is singular"),
            Self::StateSpaceTooLarge { states, max } => {
                write!(f, "state space too large: {} states (max {})", states, max)
            }
            Self::InvalidRate(s) => write!(f, "invalid rate: {}", s),
            Self::InvalidWeight(s) => write!(f, "invalid weight: {}", s),
            Self::InvalidPriority(s) => write!(f, "invalid priority: {}", s),
            Self::MissingTransitionType(s) => {
                write!(f, "missing GSPN type for transition: {}", s)
            }
        }
    }
}

impl std::error::Error for GspnError {}
