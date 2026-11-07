use racon_core::graph::NodeId;
use racon_core::net::{PlaceId, TransitionId};
use std::fmt;

// ---------------------------------------------------------------------------
// CTL Abstract Syntax Tree
// ---------------------------------------------------------------------------

/// A CTL formula over a Petri net's state space.
#[derive(Debug, Clone, PartialEq)]
pub enum CTLFormula {
    // Constants
    True,
    False,

    // Atomic proposition (linear constraint over marking)
    AtomicProposition(AtomicPredicate),

    // Boolean connectives
    Not(Box<CTLFormula>),
    And(Box<CTLFormula>, Box<CTLFormula>),
    Or(Box<CTLFormula>, Box<CTLFormula>),
    Implies(Box<CTLFormula>, Box<CTLFormula>),

    // Unary temporal operators
    EX(Box<CTLFormula>),
    AX(Box<CTLFormula>),
    EF(Box<CTLFormula>),
    AF(Box<CTLFormula>),
    EG(Box<CTLFormula>),
    AG(Box<CTLFormula>),

    // Binary temporal operators
    EU(Box<CTLFormula>, Box<CTLFormula>), // E[φ1 U φ2]
    AU(Box<CTLFormula>, Box<CTLFormula>), // A[φ1 U φ2]
}

impl fmt::Display for CTLFormula {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::True => write!(formatter, "TRUE"),
            Self::False => write!(formatter, "FALSE"),
            Self::AtomicProposition(predicate) => write!(formatter, "{predicate}"),
            Self::Not(inner) => write!(formatter, "!({inner})"),
            Self::And(left, right) => write!(formatter, "({left}) AND ({right})"),
            Self::Or(left, right) => write!(formatter, "({left}) OR ({right})"),
            Self::Implies(left, right) => write!(formatter, "({left}) -> ({right})"),
            Self::EX(inner) => write!(formatter, "EX({inner})"),
            Self::AX(inner) => write!(formatter, "AX({inner})"),
            Self::EF(inner) => write!(formatter, "EF({inner})"),
            Self::AF(inner) => write!(formatter, "AF({inner})"),
            Self::EG(inner) => write!(formatter, "EG({inner})"),
            Self::AG(inner) => write!(formatter, "AG({inner})"),
            Self::EU(left, right) => write!(formatter, "E[({left}) U ({right})]"),
            Self::AU(left, right) => write!(formatter, "A[({left}) U ({right})]"),
        }
    }
}

impl fmt::Display for AtomicPredicate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, term) in self.terms.iter().enumerate() {
            if index > 0 {
                write!(formatter, " + ")?;
            }
            match term.coefficient {
                1 => write!(formatter, "p{}", term.place_id.0)?,
                -1 => write!(formatter, "-p{}", term.place_id.0)?,
                coefficient => write!(formatter, "{}*p{}", coefficient, term.place_id.0)?,
            }
        }
        write!(formatter, " {} {}", self.operator, self.constant)
    }
}

impl fmt::Display for ComparisonOp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self {
            Self::Eq => "=",
            Self::Neq => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        };
        formatter.write_str(symbol)
    }
}

// ---------------------------------------------------------------------------
// Atomic Predicate
// ---------------------------------------------------------------------------

/// A linear arithmetic constraint: sum(a_i * m(p_i)) op c
#[derive(Debug, Clone, PartialEq)]
pub struct AtomicPredicate {
    pub terms: Vec<PredicateTerm>,
    pub operator: ComparisonOp,
    pub constant: i64,
}

/// A single term in a linear constraint: coefficient * marking[place_id]
#[derive(Debug, Clone, PartialEq)]
pub struct PredicateTerm {
    pub place_id: PlaceId,
    pub coefficient: i64,
}

/// Comparison operators for atomic predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
}

// ---------------------------------------------------------------------------
// Witness / Counterexample
// ---------------------------------------------------------------------------

/// A witness path demonstrating satisfaction of a CTL formula.
#[derive(Debug, Clone)]
pub struct PathWitness {
    pub description: String,
    pub nodes: Vec<NodeId>,
    pub transitions: Vec<TransitionId>,
    /// For EG / AG, the path may contain a cycle (infinite path)
    pub is_infinite: bool,
    pub cycle_start: Option<usize>,
}

/// A counterexample path demonstrating violation of a CTL formula.
#[derive(Debug, Clone)]
pub struct PathCounterexample {
    pub description: String,
    pub nodes: Vec<NodeId>,
    pub transitions: Vec<TransitionId>,
    pub violation_point: NodeId,
    pub is_infinite: bool,
}

/// Result of evaluating a CTL formula over a state space.
#[derive(Debug, Clone)]
pub struct CTLResult {
    pub formula: CTLFormula,
    pub satisfied: bool,
    pub satisfying_states: Vec<NodeId>,
    pub total_states: usize,
    pub evaluation_time_ms: u64,
    pub witness: Option<PathWitness>,
    pub counterexample: Option<PathCounterexample>,
}
