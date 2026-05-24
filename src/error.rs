//! Parser error type.

use ecma_lex_cat::error::Error as LexError;
use ecma_syntax_cat::error::Error as SyntaxError;
use ecma_syntax_cat::span::Span;

/// All errors the parser can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Lexing the source failed.
    Lex(LexError),
    /// Constructing an AST node from valid-shaped input failed (e.g. an
    /// identifier reserved by ecma-syntax-cat's validation).
    Syntax(SyntaxError),
    /// Encountered a token that does not satisfy the current production.
    UnexpectedToken {
        /// Source span of the offending token.
        at: Span,
        /// Short description of what was expected.
        expected: &'static str,
        /// String rendering of the token actually found.
        found: String,
    },
    /// Input ended while a production was still demanding more tokens.
    UnexpectedEof {
        /// What was expected.
        expected: &'static str,
    },
    /// An assignment target was not a valid lvalue (identifier, member,
    /// or destructuring pattern).
    InvalidAssignmentTarget {
        /// Where the bad target appeared.
        at: Span,
    },
    /// An expression appeared before `=>` that cannot be reinterpreted
    /// as an arrow parameter list.
    InvalidArrowParameter {
        /// Where the bad parameter appeared.
        at: Span,
    },
    /// A class body contained a member the parser cannot classify.
    InvalidClassMember {
        /// Where the bad member appeared.
        at: Span,
    },
}

impl From<LexError> for Error {
    fn from(value: LexError) -> Self {
        Self::Lex(value)
    }
}

impl From<SyntaxError> for Error {
    fn from(value: SyntaxError) -> Self {
        Self::Syntax(value)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lex(e) => write!(f, "lex error: {e}"),
            Self::Syntax(e) => write!(f, "syntax error: {e}"),
            Self::UnexpectedToken {
                at,
                expected,
                found,
            } => write!(f, "{at}: unexpected token {found:?}; expected {expected}"),
            Self::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input; expected {expected}")
            }
            Self::InvalidAssignmentTarget { at } => {
                write!(f, "{at}: not a valid assignment target")
            }
            Self::InvalidArrowParameter { at } => {
                write!(f, "{at}: not a valid arrow parameter")
            }
            Self::InvalidClassMember { at } => {
                write!(f, "{at}: not a valid class member")
            }
        }
    }
}

impl std::error::Error for Error {}
