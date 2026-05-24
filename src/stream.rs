//! Token cursor helpers.
//!
//! Functions in this module never own state: they take a slice and a
//! position and return either a new position or a value.  Threading the
//! cursor functionally keeps the parser purely composable.

use crate::error::Error;
use ecma_lex_cat::token::{Token, TokenKind};
use ecma_syntax_cat::identifier::{Identifier, PrivateIdentifier};
use ecma_syntax_cat::span::{Position, Span};

/// Look-ahead result.
pub enum Peek<'a> {
    /// EOF (no more tokens) or only the synthetic `Eof` marker remains.
    Eof,
    /// A real token.
    Token(&'a Token),
}

/// Peek at the token at `pos`, treating the synthetic `Eof` marker the
/// same as actually running out of tokens.
#[must_use]
pub fn peek(tokens: &[Token], pos: usize) -> Peek<'_> {
    tokens
        .get(pos)
        .filter(|t| !matches!(t.value(), TokenKind::Eof))
        .map_or(Peek::Eof, Peek::Token)
}

/// Span of the token at `pos`, or a synthetic zero-width span at EOF.
#[must_use]
pub fn span_at(tokens: &[Token], pos: usize) -> Span {
    tokens.get(pos).map_or(Span::synthetic(), Token::span)
}

/// Span of the previous token (used for end-of-input error positions).
#[must_use]
#[allow(dead_code)]
pub fn span_before(tokens: &[Token], pos: usize) -> Span {
    pos.checked_sub(1).and_then(|prev| tokens.get(prev)).map_or(
        Span::new(Position::synthetic(), Position::synthetic()),
        Token::span,
    )
}

/// If the token at `pos` matches `expected`, return the position after it.
/// Otherwise return [`Error::UnexpectedToken`] or [`Error::UnexpectedEof`].
///
/// `expected` must be a unit-data variant (the comparison uses
/// `TokenKind`'s `PartialEq`).  For variants carrying data (identifier,
/// number, etc.) use the dedicated `expect_*` helpers.
///
/// # Errors
///
/// See variant descriptions above.
pub fn expect_kind(
    tokens: &[Token],
    pos: usize,
    expected: &TokenKind,
    name: &'static str,
) -> Result<usize, Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof { expected: name }),
        Peek::Token(tok) => {
            if tok.value() == expected {
                Ok(pos + 1)
            } else {
                Err(Error::UnexpectedToken {
                    at: tok.span(),
                    expected: name,
                    found: format!("{}", tok.value()),
                })
            }
        }
    }
}

/// Whether the token at `pos` is `kind`.  Returns `false` at EOF.
#[must_use]
pub fn is_kind(tokens: &[Token], pos: usize, kind: &TokenKind) -> bool {
    matches!(peek(tokens, pos), Peek::Token(tok) if tok.value() == kind)
}

/// Consume the token at `pos` if it equals `kind`, returning `Some(new_pos)`.
/// Returns `None` if it does not match or at EOF.
#[must_use]
#[allow(dead_code)]
pub fn consume_if(tokens: &[Token], pos: usize, kind: &TokenKind) -> Option<usize> {
    is_kind(tokens, pos, kind).then_some(pos + 1)
}

/// Eat a semicolon, with limited automatic-semicolon-insertion: a `;` is
/// optional immediately before `}` or EOF; required everywhere else.
///
/// # Errors
///
/// [`Error::UnexpectedToken`] when a `;` is required but not present.
pub fn eat_semicolon(tokens: &[Token], pos: usize) -> Result<usize, Error> {
    match peek(tokens, pos) {
        Peek::Eof => Ok(pos),
        Peek::Token(tok) => match tok.value() {
            TokenKind::Semicolon => Ok(pos + 1),
            TokenKind::RBrace => Ok(pos),
            _other => Err(Error::UnexpectedToken {
                at: tok.span(),
                expected: "`;` or `}`",
                found: format!("{}", tok.value()),
            }),
        },
    }
}

/// Expect an identifier token; return the constructed `Identifier` and the
/// position after it.
///
/// # Errors
///
/// [`Error::UnexpectedToken`] if the next token is not an identifier.
/// [`Error::Syntax`] if the identifier text fails validation.
pub fn expect_identifier(tokens: &[Token], pos: usize) -> Result<(Identifier, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "identifier",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::Identifier(name) => {
                let id = Identifier::new(name.clone())?;
                Ok((id, pos + 1))
            }
            _other => Err(Error::UnexpectedToken {
                at: tok.span(),
                expected: "identifier",
                found: format!("{}", tok.value()),
            }),
        },
    }
}

/// Expect an identifier OR a contextual-keyword that may be used as an
/// identifier in this position (e.g. `let` as a variable name in sloppy
/// mode, `await` outside async functions).  For v0, accepts any identifier
/// or `let`/`async`/`get`/`set`/`of`/`as`/`from`/`static`/`yield`/`await`.
///
/// # Errors
///
/// Same as [`expect_identifier`].
pub fn expect_identifier_or_keyword(
    tokens: &[Token],
    pos: usize,
) -> Result<(Identifier, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "identifier",
        }),
        Peek::Token(tok) => {
            let text_option = identifier_text(tok.value());
            match text_option {
                Some(text) => {
                    let id = Identifier::new(text)?;
                    Ok((id, pos + 1))
                }
                None => Err(Error::UnexpectedToken {
                    at: tok.span(),
                    expected: "identifier",
                    found: format!("{}", tok.value()),
                }),
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // exhaustive enumeration over the ~110 TokenKind variants
#[allow(clippy::match_same_arms)] // all non-identifier arms intentionally return None, grouped by category for documentation
fn identifier_text(kind: &TokenKind) -> Option<String> {
    match kind {
        TokenKind::Identifier(name) => Some(name.clone()),
        TokenKind::KwLet => Some("let".to_owned()),
        TokenKind::KwAwait => Some("await".to_owned()),
        TokenKind::KwYield => Some("yield".to_owned()),
        TokenKind::KwStatic => Some("static".to_owned()),
        TokenKind::KwImplements
        | TokenKind::KwInterface
        | TokenKind::KwPackage
        | TokenKind::KwPrivate
        | TokenKind::KwProtected
        | TokenKind::KwPublic => None,
        TokenKind::KwBreak
        | TokenKind::KwCase
        | TokenKind::KwCatch
        | TokenKind::KwClass
        | TokenKind::KwConst
        | TokenKind::KwContinue
        | TokenKind::KwDebugger
        | TokenKind::KwDefault
        | TokenKind::KwDelete
        | TokenKind::KwDo
        | TokenKind::KwElse
        | TokenKind::KwEnum
        | TokenKind::KwExport
        | TokenKind::KwExtends
        | TokenKind::KwFalse
        | TokenKind::KwFinally
        | TokenKind::KwFor
        | TokenKind::KwFunction
        | TokenKind::KwIf
        | TokenKind::KwImport
        | TokenKind::KwIn
        | TokenKind::KwInstanceof
        | TokenKind::KwNew
        | TokenKind::KwNull
        | TokenKind::KwReturn
        | TokenKind::KwSuper
        | TokenKind::KwSwitch
        | TokenKind::KwThis
        | TokenKind::KwThrow
        | TokenKind::KwTrue
        | TokenKind::KwTry
        | TokenKind::KwTypeof
        | TokenKind::KwVar
        | TokenKind::KwVoid
        | TokenKind::KwWhile
        | TokenKind::KwWith => None,
        TokenKind::PrivateIdentifier(_)
        | TokenKind::Number(_)
        | TokenKind::BigInt(_)
        | TokenKind::String(_)
        | TokenKind::RegExp { .. }
        | TokenKind::TemplateNoSubstitution(_)
        | TokenKind::TemplateHead(_)
        | TokenKind::TemplateMiddle(_)
        | TokenKind::TemplateTail(_)
        | TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBracket
        | TokenKind::RBracket
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::Comma
        | TokenKind::Semicolon
        | TokenKind::Colon
        | TokenKind::Dot
        | TokenKind::OptionalChain
        | TokenKind::Spread
        | TokenKind::Arrow
        | TokenKind::Question
        | TokenKind::EqEq
        | TokenKind::EqEqEq
        | TokenKind::BangEq
        | TokenKind::BangEqEq
        | TokenKind::Lt
        | TokenKind::LtEq
        | TokenKind::Gt
        | TokenKind::GtEq
        | TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Percent
        | TokenKind::StarStar
        | TokenKind::PlusPlus
        | TokenKind::MinusMinus
        | TokenKind::Amp
        | TokenKind::Pipe
        | TokenKind::Caret
        | TokenKind::Tilde
        | TokenKind::LtLt
        | TokenKind::GtGt
        | TokenKind::GtGtGt
        | TokenKind::AmpAmp
        | TokenKind::PipePipe
        | TokenKind::QQ
        | TokenKind::Bang
        | TokenKind::Eq
        | TokenKind::PlusEq
        | TokenKind::MinusEq
        | TokenKind::StarEq
        | TokenKind::SlashEq
        | TokenKind::PercentEq
        | TokenKind::StarStarEq
        | TokenKind::LtLtEq
        | TokenKind::GtGtEq
        | TokenKind::GtGtGtEq
        | TokenKind::AmpEq
        | TokenKind::PipeEq
        | TokenKind::CaretEq
        | TokenKind::AmpAmpEq
        | TokenKind::PipePipeEq
        | TokenKind::QQEq
        | TokenKind::Eof => None,
    }
}

/// Expect a private identifier (`#name`).
///
/// # Errors
///
/// [`Error::UnexpectedToken`] if the next token is not a private identifier.
pub fn expect_private_identifier(
    tokens: &[Token],
    pos: usize,
) -> Result<(PrivateIdentifier, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "private identifier",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::PrivateIdentifier(name) => {
                let id = PrivateIdentifier::new(name.clone())?;
                Ok((id, pos + 1))
            }
            _other => Err(Error::UnexpectedToken {
                at: tok.span(),
                expected: "private identifier",
                found: format!("{}", tok.value()),
            }),
        },
    }
}
