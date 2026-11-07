use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DslParseError {
    #[error("Syntax error at {line}:{col}: {message}")]
    SyntaxError {
        line: usize,
        col: usize,
        message: String,
    },

    #[error("Query references unknown place: '{0}'")]
    UnknownPlace(String),

    #[error("Query references unknown transition: '{0}'")]
    UnknownTransition(String),

    #[error("Invalid atomic syntax near '{0}'")]
    InvalidAtomicSyntax(String),

    #[error("Semantic error: {0}")]
    SemanticError(String),
}
