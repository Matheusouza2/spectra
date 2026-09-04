use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use racon_core::execution::AnalysisContext;
use racon_core::net::PetriNet;
use racon_invariants::{
    derive_structural_properties, InvariantAnalysisResult, InvariantEngine, InvariantEngineImpl,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvariantKind {
    Place,
    Transition,
    #[default]
    All,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvariantQuery {
    #[serde(default)]
    pub kind: InvariantKind,
    #[serde(default)]
    pub minimal: Option<bool>,
    #[serde(default)]
    pub sub: bool,
    #[serde(default)]
    pub sur: bool,
    #[serde(default)]
    pub positive: bool,
    #[serde(default)]
    pub nonnegative: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum InvariantInteger {
    Number(i64),
    Decimal(String),
}

impl InvariantInteger {
    pub fn from_decimal(value: &str) -> Result<Self, InvariantQueryError> {
        if let Ok(value) = value.parse::<i64>() {
            return Ok(Self::Number(value));
        }

        let digits = value.strip_prefix('-').unwrap_or(value);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvariantQueryError::InvalidInteger(value.to_owned()));
        }

        Ok(Self::Decimal(value.to_owned()))
    }

    pub fn as_decimal(&self) -> String {
        match self {
            Self::Number(value) => value.to_string(),
            Self::Decimal(value) => value.clone(),
        }
    }

    fn is_zero(&self) -> bool {
        matches!(self, Self::Number(0)) || matches!(self, Self::Decimal(value) if value == "0")
    }

    fn is_positive(&self) -> bool {
        match self {
            Self::Number(value) => *value > 0,
            Self::Decimal(value) => value != "0" && !value.starts_with('-'),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvariantTermDto {
    pub node: String,
    pub coefficient: InvariantInteger,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvariantDto {
    #[serde(rename = "type")]
    pub invariant_type: InvariantKind,
    pub minimal: bool,
    pub vector: Vec<InvariantInteger>,
    pub support: Vec<String>,
    pub terms: Vec<InvariantTermDto>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantQueryError {
    NotImplemented { filter: &'static str },
    Computation(String),
    InvalidInteger(String),
}

impl InvariantQueryError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotImplemented { .. } => "NOT_IMPLEMENTED",
            Self::Computation(_) => "INVARIANT_COMPUTATION_FAILED",
            Self::InvalidInteger(_) => "INVALID_INTEGER",
        }
    }
}

impl fmt::Display for InvariantQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented { filter } => {
                write!(formatter, "NOT_IMPLEMENTED: invariant filter '{filter}'")
            }
            Self::Computation(message) => {
                write!(formatter, "invariant computation failed: {message}")
            }
            Self::InvalidInteger(value) => {
                write!(formatter, "invalid integer coefficient: {value}")
            }
        }
    }
}

impl std::error::Error for InvariantQueryError {}

#[derive(Debug, Default)]
pub struct InvariantCache {
    value: RwLock<Option<Arc<InvariantAnalysisResult>>>,
}

impl InvariantCache {
    pub fn cached(&self) -> Option<Arc<InvariantAnalysisResult>> {
        self.value
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn get_or_compute(
        &self,
        net: &PetriNet,
        context: &AnalysisContext,
    ) -> Result<Arc<InvariantAnalysisResult>, InvariantQueryError> {
        if let Some(cached) = self.cached() {
            return Ok(cached);
        }

        let mut cache = self
            .value
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cache.as_ref() {
            return Ok(Arc::clone(cached));
        }

        let engine = InvariantEngineImpl;
        let p_semiflows = engine
            .compute_p_semiflows(net, context)
            .map_err(|error| InvariantQueryError::Computation(error.to_string()))?;
        let t_semiflows = engine
            .compute_t_semiflows(net, context)
            .map_err(|error| InvariantQueryError::Computation(error.to_string()))?;
        let (is_conservative, is_cpi, is_cti) =
            derive_structural_properties(net, &p_semiflows, &t_semiflows);

        let result = Arc::new(InvariantAnalysisResult {
            minimal_p_semiflows: p_semiflows
                .iter()
                .map(|invariant| invariant.coefficients.clone())
                .collect(),
            minimal_t_semiflows: t_semiflows
                .iter()
                .map(|invariant| invariant.coefficients.clone())
                .collect(),
            p_semiflows,
            t_semiflows,
            is_conservative,
            is_cpi,
            is_cti,
            structural_bounds: HashMap::new(),
            structural_bounded_by_inequality: None,
        });
        *cache = Some(Arc::clone(&result));

        Ok(result)
    }
}

pub(crate) fn query_invariants(
    net: &PetriNet,
    cache: &InvariantCache,
    query: &InvariantQuery,
    context: &AnalysisContext,
) -> Result<Vec<InvariantDto>, InvariantQueryError> {
    validate_query(query)?;
    let analysis = cache.get_or_compute(net, context)?;
    let mut result = Vec::new();

    if matches!(query.kind, InvariantKind::Place | InvariantKind::All) {
        for invariant in &analysis.p_semiflows {
            if let Some(invariant) = invariant_dto(
                InvariantKind::Place,
                invariant.is_minimal,
                &invariant.coefficients,
                net.places().iter().map(|place| place.name.as_str()),
                query,
            )? {
                result.push(invariant);
            }
        }
    }

    if matches!(query.kind, InvariantKind::Transition | InvariantKind::All) {
        for invariant in &analysis.t_semiflows {
            if let Some(invariant) = invariant_dto(
                InvariantKind::Transition,
                invariant.is_minimal,
                &invariant.coefficients,
                net.transitions()
                    .iter()
                    .map(|transition| transition.name.as_str()),
                query,
            )? {
                result.push(invariant);
            }
        }
    }

    Ok(result)
}

fn validate_query(query: &InvariantQuery) -> Result<(), InvariantQueryError> {
    if query.sub {
        return Err(InvariantQueryError::NotImplemented { filter: "sub" });
    }
    if query.sur {
        return Err(InvariantQueryError::NotImplemented { filter: "sur" });
    }
    Ok(())
}

fn invariant_dto<'a, I, T>(
    invariant_type: InvariantKind,
    minimal: bool,
    coefficients: &[T],
    names: I,
    query: &InvariantQuery,
) -> Result<Option<InvariantDto>, InvariantQueryError>
where
    I: IntoIterator<Item = &'a str>,
    T: ToString,
{
    if query.minimal.is_some_and(|expected| expected != minimal) {
        return Ok(None);
    }

    let names: Vec<_> = names.into_iter().collect();
    let decimal_coefficients: Vec<_> = coefficients.iter().map(ToString::to_string).collect();
    let vector: Vec<_> = decimal_coefficients
        .iter()
        .map(|coefficient| InvariantInteger::from_decimal(coefficient))
        .collect::<Result<_, _>>()?;
    if query.positive && vector.iter().any(|coefficient| !coefficient.is_positive()) {
        return Ok(None);
    }

    let terms: Vec<_> = names
        .iter()
        .zip(vector.iter())
        .filter(|(_, coefficient)| !coefficient.is_zero())
        .map(|(name, coefficient)| InvariantTermDto {
            node: (*name).to_owned(),
            coefficient: coefficient.clone(),
        })
        .collect();
    let support = terms.iter().map(|term| term.node.clone()).collect();

    Ok(Some(InvariantDto {
        invariant_type,
        minimal,
        vector,
        support,
        terms,
    }))
}
