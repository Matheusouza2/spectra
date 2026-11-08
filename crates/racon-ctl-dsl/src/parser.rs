use crate::error::DslParseError;

// ---------------------------------------------------------------------------
// DSL Intermediate Representation
// ---------------------------------------------------------------------------

/// Intermediate AST from parsing the DSL query.
/// Translated to formal CTLFormula by the translator.
#[derive(Debug, Clone)]
pub enum DslExpr {
    True,
    False,
    AtomicPlace {
        place_name: String,
        op: Option<DslCompOp>,
        value: Option<u64>,
    },
    PlaceInterval {
        place_name: String,
        min: u64,
        max: u64,
    },
    /// `enabled(t1, t2)`: at least one of the transitions has its input places marked
    /// (and its inhibitor places below their thresholds).
    Enabled(Vec<String>),
    /// Linear atom `Σ cᵢ·pᵢ ⋈ k`, already normalised to places on the left and a constant on
    /// the right (`P1 + P2 >= 2`, `P1 <= P2`, `2*P1 - P2 = 0`). Names are resolved by the translator.
    Linear {
        terms: Vec<(String, i64)>,
        op: DslCompOp,
        constant: i64,
    },
    /// No transition of the net is enabled (the paper's `deadlock` atom).
    Deadlock,
    Not(Box<DslExpr>),
    And(Box<DslExpr>, Box<DslExpr>),
    Or(Box<DslExpr>, Box<DslExpr>),
    Implies(Box<DslExpr>, Box<DslExpr>),
    TemporalUnary {
        quantifier: DslQuantifier,
        temporal_op: DslTemporalOp,
        expr: Box<DslExpr>,
    },
    Until {
        quantifier: DslQuantifier,
        left: Box<DslExpr>,
        right: Box<DslExpr>,
    },
    Before {
        quantifier: DslQuantifier,
        left: Box<DslExpr>,
        right: Box<DslExpr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DslQuantifier {
    Exists,
    ForAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DslTemporalOp {
    Next,
    Eventually,
    Globally,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DslCompOp {
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    LParen,
    RParen,
    Comma,

    // Constants
    True,
    False,

    // Quantifiers
    ForAll,
    Exists,

    // Temporal operators (unary)
    Globally,
    Eventually,
    Next,

    // Temporal operators (binary)
    Until,
    Before,

    // Boolean connectives
    Not,
    And,
    Or,
    Implies,

    // Place
    Place(String),

    // `enabled(t1, t2)` predicate over transitions
    Enabled,

    // `deadlock` atom: no transition is enabled
    Deadlock,

    // Arithmetic
    Plus,
    Minus,
    Star,

    // Comparisons
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,

    // Natural language comparison words
    Is,
    Equals,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
    AtLeast,
    AtMost,

    // Interval
    Between,
    IsBetween,
    InTheRange,

    // Numbers
    Number(u64),

    // Stop words (ignored)
    Path,
    Paths,
    All,
    Steps,
    Step,
    A,
    In,
    The,
    That,
    Where,
    It,
    Remains,
    TrueWord,
    Up,
    Happens,
    Occurs,

    // Identifiers (catch-all)
    Ident(String),

    Eof,
}

/// Simple hand-written tokenizer for the CTL DSL.
pub struct Tokenizer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Tokenizer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.input.get(self.pos).copied();
        if let Some(ch) = c {
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        c
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_word(&mut self) -> String {
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }
        word
    }

    fn read_number(&mut self) -> u64 {
        let mut num = 0u64;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                num = num * 10 + (c as u64 - '0' as u64);
                self.advance();
            } else {
                break;
            }
        }
        num
    }

    fn match_keyword(word: &str) -> Option<Token> {
        match word.to_lowercase().as_str() {
            "enabled" | "habilitada" | "habilitado" => Some(Token::Enabled),
            "deadlock" | "bloqueio" | "impasse" => Some(Token::Deadlock),
            "true" | "t" | "verdadeiro" => Some(Token::True),
            "false" | "falso" => Some(Token::False),
            "not" | "nao" | "não" | "never" | "!" => Some(Token::Not),
            "and" | "e" | "&" | "&&" => Some(Token::And),
            "or" | "ou" | "v" | "||" => Some(Token::Or),
            "implies" | "se" | "whenever" | "if" => Some(Token::Implies),
            "sempre" | "always" | "globally" | "invariantly" | "g" | "forever" => {
                Some(Token::Globally)
            }
            "eventualmente" | "eventually" | "finally" | "no-futuro" => Some(Token::Eventually),
            "proximo" | "próximo" | "next" | "x" => Some(Token::Next),
            "ate" | "até" | "until" | "u" => Some(Token::Until),
            "antes" | "before" | "b" => Some(Token::Before),
            "existe" | "exists" | "there" | "é-possível" | "it" => Some(Token::Exists),
            "para" | "for-all" | "para-todos" | "for" | "forall" | "universally" | "on" => {
                Some(Token::ForAll)
            }
            "is" | "é" | "igual" => Some(Token::Is),
            "equals" | "equal-to" => Some(Token::Equals),
            "greater-than" | "maior-que" => Some(Token::GreaterThan),
            "less-than" | "menor-que" => Some(Token::LessThan),
            "greater-or-equal" | "maior-ou-igual" | "maiou-ou-igual-a" => {
                Some(Token::GreaterOrEqual)
            }
            "less-or-equal" | "menor-ou-igual" | "menor-ou-igual-a" => Some(Token::LessOrEqual),
            "at-least" | "no-minimo" | "mínimo" => Some(Token::AtLeast),
            "at-most" | "no-maximo" | "máximo" => Some(Token::AtMost),
            "between" | "entre" => Some(Token::Between),
            "path" | "paths" | "caminho" | "caminhos" => Some(Token::Path),
            "all" | "todos" | "todo" | "toda" | "todas" => Some(Token::All),
            "steps" | "step" | "passo" => Some(Token::Steps),
            "in" | "no" | "na" | "em" => Some(Token::In),
            "the" | "o" | "os" | "as" => Some(Token::The),
            "that" | "que" => Some(Token::That),
            "where" | "onde" => Some(Token::Where),
            "remains" | "continua" | "continue" => Some(Token::Remains),
            "up" => Some(Token::Up),
            "happens" | "ocorre" | "acontece" => Some(Token::Happens),
            // "and" is already matched, but "and also" needs special handling
            _ => None,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<(Token, usize, usize)>, DslParseError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace();
            let line = self.line;
            let col = self.col;

            match self.peek() {
                None => {
                    tokens.push((Token::Eof, line, col));
                    break;
                }
                Some('(') => {
                    self.advance();
                    tokens.push((Token::LParen, line, col));
                }
                Some(')') => {
                    self.advance();
                    tokens.push((Token::RParen, line, col));
                }
                Some(',') => {
                    self.advance();
                    tokens.push((Token::Comma, line, col));
                }
                Some('=') => {
                    self.advance();
                    tokens.push((Token::Eq, line, col));
                }
                Some('>') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push((Token::Ge, line, col));
                    } else {
                        tokens.push((Token::Gt, line, col));
                    }
                }
                Some('<') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push((Token::Le, line, col));
                    } else if self.peek() == Some('>') {
                        self.advance();
                        tokens.push((Token::Neq, line, col));
                    } else {
                        tokens.push((Token::Lt, line, col));
                    }
                }
                Some('!') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push((Token::Neq, line, col));
                    } else {
                        tokens.push((Token::Not, line, col));
                    }
                }
                Some('&') => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                    }
                    tokens.push((Token::And, line, col));
                }
                Some('|') => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                    }
                    tokens.push((Token::Or, line, col));
                }
                Some('-') => {
                    self.advance();
                    tokens.push((Token::Minus, line, col));
                }
                Some('+') => {
                    self.advance();
                    tokens.push((Token::Plus, line, col));
                }
                Some('*') => {
                    self.advance();
                    tokens.push((Token::Star, line, col));
                }
                Some(c) if c.is_ascii_digit() => {
                    let n = self.read_number();
                    tokens.push((Token::Number(n), line, col));
                }
                // Quoted name: always a place/transition, never a keyword ("B", `x`, "In").
                Some(q) if q == '"' || q == '`' => {
                    self.advance();
                    let mut name = String::new();
                    loop {
                        match self.advance() {
                            Some(c) if c == q => break,
                            Some(c) => name.push(c),
                            None => {
                                return Err(DslParseError::SyntaxError {
                                    line,
                                    col,
                                    message: "Unterminated quoted name".to_string(),
                                })
                            }
                        }
                    }
                    if name.is_empty() {
                        return Err(DslParseError::SyntaxError {
                            line,
                            col,
                            message: "Empty quoted name".to_string(),
                        });
                    }
                    tokens.push((Token::Place(name), line, col));
                }
                Some(c) if c.is_alphabetic() || c == '_' => {
                    let word = self.read_word();
                    // A reserved word directly followed by a comparison operator
                    // ("B = 1", "x >= 2") can only be a place name.
                    let before_comparison = {
                        let mut i = self.pos;
                        while self.input.get(i).is_some_and(|c| c.is_whitespace()) {
                            i += 1;
                        }
                        matches!(self.input.get(i), Some('=' | '<' | '>'))
                            || (self.input.get(i) == Some(&'!')
                                && self.input.get(i + 1) == Some(&'='))
                    };
                    if before_comparison
                        && !matches!(
                            Self::match_keyword(&word),
                            None | Some(Token::Is | Token::Equals)
                        )
                    {
                        tokens.push((Token::Place(word), line, col));
                    } else if let Some(keyword) = Self::match_keyword(&word) {
                        tokens.push((keyword, line, col));
                    } else {
                        tokens.push((Token::Place(word), line, col));
                    }
                }
                Some(c) => {
                    return Err(DslParseError::SyntaxError {
                        line,
                        col,
                        message: format!("Unexpected character: '{}'", c),
                    });
                }
            }
        }

        Ok(tokens)
    }
}

// ---------------------------------------------------------------------------
// Parser (Recursive Descent)
// ---------------------------------------------------------------------------

pub struct Parser {
    tokens: Vec<(Token, usize, usize)>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<(Token, usize, usize)>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &(Token, usize, usize) {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .expect("CTL parser requires an EOF token")
    }

    fn advance(&mut self) -> &(Token, usize, usize) {
        let index = self.pos.min(self.tokens.len().saturating_sub(1));
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        &self.tokens[index]
    }

    fn expect(&mut self, expected: &Token) -> Result<(Token, usize, usize), DslParseError> {
        let (ref tok, line, col) = self.peek().clone();
        if std::mem::discriminant(tok) == std::mem::discriminant(expected) {
            Ok(self.advance().clone())
        } else {
            Err(DslParseError::SyntaxError {
                line,
                col,
                message: format!("Expected {:?}, found {:?}", expected, tok),
            })
        }
    }

    fn skip_stop_words(&mut self) {
        loop {
            match self.peek().0 {
                Token::The
                | Token::A
                | Token::That
                | Token::Where
                | Token::Path
                | Token::Paths
                | Token::All
                | Token::Steps
                | Token::Step
                | Token::Up
                | Token::Remains
                | Token::Happens => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    fn skip_leading_path_phrase(&mut self) {
        let is_path_phrase = matches!(self.peek().0, Token::In)
            && matches!(
                self.tokens.get(self.pos + 1).map(|entry| &entry.0),
                Some(Token::All)
            );
        if is_path_phrase {
            self.advance();
        }
    }

    /// Parse the full formula.
    pub fn parse(&mut self) -> Result<DslExpr, DslParseError> {
        let expr = self.parse_implies_expr()?;
        self.expect(&Token::Eof)?;
        Ok(expr)
    }

    fn parse_implies_expr(&mut self) -> Result<DslExpr, DslParseError> {
        let left = self.parse_or_expr()?;
        self.skip_stop_words();
        if matches!(self.peek().0, Token::Implies) {
            self.advance();
            let right = self.parse_implies_expr()?;
            return Ok(DslExpr::Implies(Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    // Precedence: or < and < not < temporal < atomic
    fn parse_or_expr(&mut self) -> Result<DslExpr, DslParseError> {
        let mut left = self.parse_and_expr()?;
        loop {
            self.skip_stop_words();
            match self.peek().0 {
                Token::Or => {
                    self.advance();
                    let right = self.parse_and_expr()?;
                    left = DslExpr::Or(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<DslExpr, DslParseError> {
        let mut left = self.parse_not_expr()?;
        loop {
            self.skip_stop_words();
            match self.peek().0 {
                Token::And => {
                    self.advance();
                    let right = self.parse_not_expr()?;
                    left = DslExpr::And(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_not_expr(&mut self) -> Result<DslExpr, DslParseError> {
        self.skip_stop_words();
        match self.peek().0 {
            // A leading "-" is the legacy spelling of "not"; inside arithmetic it is subtraction.
            Token::Not | Token::Minus => {
                self.advance();
                let inner = self.parse_not_expr()?;
                Ok(DslExpr::Not(Box::new(inner)))
            }
            _ => self.parse_temporal(),
        }
    }

    fn parse_temporal(&mut self) -> Result<DslExpr, DslParseError> {
        self.skip_leading_path_phrase();
        self.skip_stop_words();

        // Check for quantifier prefix: "existe um caminho" / "para todos os caminhos"
        let quantifier = self.try_parse_quantifier();

        // Older prefix form, still accepted so saved queries keep working:
        // "existe caminho até φ e ψ" / "para todo caminho até φ e ψ". Here "e" is only the
        // separator of the two sides, not a conjunction. The documented form is the infix one,
        // "existe caminho (φ) até (ψ)", handled by `parse_until_or_atom_with_q`.
        if quantifier.is_some() && matches!(self.peek().0, Token::Until) {
            self.advance();
            let left = self.parse_not_expr()?;
            self.skip_stop_words();
            self.expect(&Token::And)?;
            let right = self.parse_not_expr()?;
            return Ok(DslExpr::Until {
                quantifier: quantifier.unwrap(),
                left: Box::new(left),
                right: Box::new(right),
            });
        }

        // Check for temporal operator: always, eventually, next (after quantifier)
        match self.peek().0 {
            Token::Globally | Token::Eventually | Token::Next => {
                let op = match self.advance().0 {
                    Token::Globally => DslTemporalOp::Globally,
                    Token::Eventually => DslTemporalOp::Eventually,
                    Token::Next => DslTemporalOp::Next,
                    _ => unreachable!(),
                };

                // Handle "existe um caminho sempre φ" → quantifier already consumed
                // Or "sempre φ" (no quantifier) → default to ForAll
                let q = quantifier.unwrap_or(DslQuantifier::ForAll);
                let inner = self.parse_not_expr()?;
                return Ok(DslExpr::TemporalUnary {
                    quantifier: q,
                    temporal_op: op,
                    expr: Box::new(inner),
                });
            }
            _ => {
                // No temporal operator — try until/before with the quantifier
                self.parse_until_or_atom_with_q(quantifier)
            }
        }
    }

    fn try_parse_quantifier(&mut self) -> Option<DslQuantifier> {
        self.skip_stop_words();
        let _saved = self.pos;

        match self.peek().0 {
            Token::Exists => {
                self.advance();
                // Skip optional "um caminho" / "a path" / "that" / etc.
                self.skip_stop_words();
                Some(DslQuantifier::Exists)
            }
            Token::ForAll => {
                self.advance();
                self.skip_stop_words();
                Some(DslQuantifier::ForAll)
            }
            _ => None,
        }
    }

    fn parse_until_or_atom_with_q(
        &mut self,
        quantifier: Option<DslQuantifier>,
    ) -> Result<DslExpr, DslParseError> {
        self.skip_stop_words();

        // Try to parse left side of until/before
        let left = self.parse_atom()?;

        self.skip_stop_words();
        match self.peek().0 {
            Token::Until => {
                self.advance();
                let right = self.parse_not_expr()?;
                let q = quantifier.unwrap_or(DslQuantifier::Exists);
                Ok(DslExpr::Until {
                    quantifier: q,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            Token::Before => {
                self.advance();
                let right = self.parse_not_expr()?;
                let q = quantifier.unwrap_or(DslQuantifier::Exists);
                Ok(DslExpr::Before {
                    quantifier: q,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            _ => {
                if quantifier.is_some() {
                    return Err(DslParseError::SyntaxError {
                        line: self.peek().1,
                        col: self.peek().2,
                        message: "Quantifier without temporal or until operator".to_string(),
                    });
                }
                Ok(left)
            }
        }
    }

    fn parse_atom(&mut self) -> Result<DslExpr, DslParseError> {
        self.skip_stop_words();

        match self.peek().clone() {
            (Token::LParen, _, _) => {
                self.advance();
                let expr = self.parse_implies_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            (Token::True, _, _) => {
                self.advance();
                Ok(DslExpr::True)
            }
            (Token::False, _, _) => {
                self.advance();
                Ok(DslExpr::False)
            }
            (Token::Enabled, _, _) => {
                self.advance();
                self.parse_enabled()
            }
            (Token::Deadlock, _, _) => {
                self.advance();
                Ok(DslExpr::Deadlock)
            }
            (Token::Number(_), _, _) => self.parse_linear_comparison(),
            (Token::Place(name), line, col) => {
                self.advance();
                self.parse_place_expr(name, line, col)
            }
            (ref tok, line, col) => Err(DslParseError::SyntaxError {
                line,
                col,
                message: format!("Unexpected token: {:?}", tok),
            }),
        }
    }

    /// `enabled(T1, T2 OR T3)`: transition names separated by commas or OR.
    fn parse_enabled(&mut self) -> Result<DslExpr, DslParseError> {
        self.expect(&Token::LParen)?;
        let mut names = Vec::new();
        loop {
            let (token, line, col) = self.advance().clone();
            match token {
                Token::Place(name) => names.push(name),
                other => {
                    return Err(DslParseError::SyntaxError {
                        line,
                        col,
                        message: format!("Expected a transition name inside enabled(...), found {other:?}"),
                    })
                }
            }
            match self.peek().0 {
                Token::Comma | Token::Or => {
                    self.advance();
                }
                _ => break,
            }
        }
        self.expect(&Token::RParen)?;
        Ok(DslExpr::Enabled(names))
    }

    fn parse_place_expr(
        &mut self,
        name: String,
        _line: usize,
        _col: usize,
    ) -> Result<DslExpr, DslParseError> {
        if self.starts_linear_atom() {
            self.pos -= 1; // back to the place token
            return self.parse_linear_comparison();
        }
        self.skip_stop_words();

        // Check for interval: "P1 between 1 and 5"
        match self.peek().0 {
            Token::Between | Token::IsBetween | Token::InTheRange => {
                self.advance();
                let min = match self.advance().0 {
                    Token::Number(n) => n,
                    _ => {
                        return Err(DslParseError::SyntaxError {
                            line: self.peek().1,
                            col: self.peek().2,
                            message: "Expected number after interval keyword".to_string(),
                        })
                    }
                };
                // Skip "and" / "to" / "e" / "a"
                self.skip_stop_words();
                self.advance(); // skip the connector
                let max = match self.advance().0 {
                    Token::Number(n) => n,
                    _ => {
                        return Err(DslParseError::SyntaxError {
                            line: self.peek().1,
                            col: self.peek().2,
                            message: "Expected number for interval max".to_string(),
                        })
                    }
                };
                return Ok(DslExpr::PlaceInterval {
                    place_name: name,
                    min,
                    max,
                });
            }
            _ => {}
        }

        // Check for comparison operators
        let (op, value) = match self.peek().0 {
            Token::Eq
            | Token::Is
            | Token::Equals
            | Token::Lt
            | Token::Gt
            | Token::Le
            | Token::Ge
            | Token::Neq
            | Token::GreaterThan
            | Token::LessThan
            | Token::GreaterOrEqual
            | Token::LessOrEqual
            | Token::AtLeast
            | Token::AtMost => {
                let op_token = self.advance().0.clone();
                let mapped_op = self.map_comparison_token(&op_token)?;

                self.skip_stop_words();
                let val = match self.advance().0 {
                    Token::Number(n) => n,
                    _ => {
                        return Err(DslParseError::SyntaxError {
                            line: self.peek().1,
                            col: self.peek().2,
                            message: "Expected number after comparison operator".to_string(),
                        })
                    }
                };
                (Some(mapped_op), Some(val))
            }
            _ => (None, None), // Bare place name: "P1" → P1 > 0
        };

        Ok(DslExpr::AtomicPlace {
            place_name: name,
            op,
            value,
        })
    }

    fn is_comparison(tok: &Token) -> bool {
        matches!(
            tok,
            Token::Eq
                | Token::Is
                | Token::Equals
                | Token::Lt
                | Token::Gt
                | Token::Le
                | Token::Ge
                | Token::Neq
                | Token::GreaterThan
                | Token::LessThan
                | Token::GreaterOrEqual
                | Token::LessOrEqual
                | Token::AtLeast
                | Token::AtMost
        )
    }

    fn token_at(&self, offset: usize) -> &Token {
        &self.tokens.get(self.pos + offset).unwrap_or_else(|| self.tokens.last().unwrap()).0
    }

    /// After a place name: does an arithmetic atom follow (`P1 + P2 ...`, `P1 <= P2`, `P1 = 1 + P2`)?
    fn starts_linear_atom(&self) -> bool {
        let arith = |t: &Token| matches!(t, Token::Plus | Token::Minus | Token::Star);
        if arith(self.token_at(0)) {
            return true;
        }
        if Self::is_comparison(self.token_at(0)) {
            return match self.token_at(1) {
                Token::Place(_) => true,
                Token::Number(_) => arith(self.token_at(2)),
                _ => false,
            };
        }
        false
    }

    fn syntax_error<T>(&self, message: &str) -> Result<T, DslParseError> {
        let (_, line, col) = self.peek();
        Err(DslParseError::SyntaxError { line: *line, col: *col, message: message.to_string() })
    }

    /// `sum := ['-'] term (('+' | '-') term)*` ; `term := factor ('*' factor)*` with at most one
    /// place per term (the formula must stay linear).
    fn parse_linear_sum(&mut self) -> Result<(Vec<(String, i64)>, i64), DslParseError> {
        let mut places: Vec<(String, i64)> = Vec::new();
        let mut constant: i64 = 0;
        let mut sign: i64 = 1;
        if matches!(self.peek().0, Token::Minus) {
            self.advance();
            sign = -1;
        }
        loop {
            self.skip_stop_words();
            let mut coeff: i64 = sign;
            let mut place: Option<String> = None;
            loop {
                match self.advance().0.clone() {
                    Token::Number(n) => {
                        let n = i64::try_from(n)
                            .or_else(|_| self.syntax_error("Number too large"))?;
                        coeff = coeff
                            .checked_mul(n)
                            .map_or_else(|| self.syntax_error("Arithmetic overflow"), Ok)?;
                    }
                    Token::Place(name) => {
                        if place.is_some() {
                            return self.syntax_error(
                                "Non-linear term: multiply a place only by constants",
                            );
                        }
                        place = Some(name);
                    }
                    _ => {
                        self.pos -= 1;
                        return self.syntax_error("Expected a number or a place in the expression");
                    }
                }
                if matches!(self.peek().0, Token::Star) {
                    self.advance();
                } else {
                    break;
                }
            }
            match place {
                Some(name) => places.push((name, coeff)),
                None => {
                    constant = constant
                        .checked_add(coeff)
                        .map_or_else(|| self.syntax_error("Arithmetic overflow"), Ok)?;
                }
            }
            match self.peek().0 {
                Token::Plus => {
                    self.advance();
                    sign = 1;
                }
                Token::Minus => {
                    self.advance();
                    sign = -1;
                }
                _ => break,
            }
        }
        Ok((places, constant))
    }

    /// `sum ⋈ sum`, normalised to `Σ cᵢ·pᵢ ⋈ k` (right-hand places move to the left).
    fn parse_linear_comparison(&mut self) -> Result<DslExpr, DslParseError> {
        let (mut terms, left_const) = self.parse_linear_sum()?;
        self.skip_stop_words();
        if !Self::is_comparison(&self.peek().0) {
            return self.syntax_error("Expected a comparison operator");
        }
        let op_token = self.advance().0.clone();
        let op = self.map_comparison_token(&op_token)?;
        let (right_terms, right_const) = self.parse_linear_sum()?;
        terms.extend(right_terms.into_iter().map(|(name, c)| (name, -c)));
        let constant = right_const
            .checked_sub(left_const)
            .map_or_else(|| self.syntax_error("Arithmetic overflow"), Ok)?;
        Ok(DslExpr::Linear { terms, op, constant })
    }

    fn map_comparison_token(&self, tok: &Token) -> Result<DslCompOp, DslParseError> {
        match tok {
            Token::Eq | Token::Is | Token::Equals => Ok(DslCompOp::Eq),
            Token::Neq => Ok(DslCompOp::Neq),
            Token::Lt | Token::LessThan => Ok(DslCompOp::Lt),
            Token::Gt | Token::GreaterThan => Ok(DslCompOp::Gt),
            Token::Le | Token::LessOrEqual => Ok(DslCompOp::Le),
            Token::Ge | Token::GreaterOrEqual | Token::AtLeast => Ok(DslCompOp::Ge),
            Token::AtMost => Ok(DslCompOp::Le),
            _ => Err(DslParseError::SyntaxError {
                line: 0,
                col: 0,
                message: format!("Invalid comparison operator: {:?}", tok),
            }),
        }
    }
}

/// Convenience: parse a DSL string into a DslExpr AST.
pub fn parse_dsl(input: &str) -> Result<DslExpr, DslParseError> {
    let mut tokenizer = Tokenizer::new(input);
    let tokens = tokenizer.tokenize()?;
    let mut parser = Parser::new(tokens);
    parser.parse()
}
