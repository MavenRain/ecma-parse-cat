//! Top-level program parsing.

use crate::error::Error;
use crate::module::parse_module_item;
use crate::statement::parse_statement;
use crate::stream::{Peek, peek, span_at};
use ecma_lex_cat::token::Token;
use ecma_syntax_cat::module::ModuleItem;
use ecma_syntax_cat::program::{Program, ProgramKind};
use ecma_syntax_cat::span::{Position, Span};
use ecma_syntax_cat::statement::Statement;

/// Parse the entire token sequence as a Script (no `import`/`export` allowed).
///
/// # Errors
///
/// See [`Error`].
pub fn parse_script(tokens: &[Token]) -> Result<Program, Error> {
    let (statements, _) = collect_script_statements(tokens, 0, Vec::new())?;
    let span = program_span(tokens);
    Ok(Program::new(ProgramKind::script(statements), span))
}

fn collect_script_statements(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Statement>,
) -> Result<(Vec<Statement>, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Ok((acc, pos)),
        Peek::Token(_) => {
            let (stmt, after_stmt) = parse_statement(tokens, pos)?;
            let extended: Vec<Statement> = acc.into_iter().chain(std::iter::once(stmt)).collect();
            collect_script_statements(tokens, after_stmt, extended)
        }
    }
}

/// Parse the entire token sequence as a Module (`import`/`export` allowed).
///
/// # Errors
///
/// See [`Error`].
pub fn parse_module(tokens: &[Token]) -> Result<Program, Error> {
    let (items, _) = collect_module_items(tokens, 0, Vec::new())?;
    let span = program_span(tokens);
    Ok(Program::new(ProgramKind::module(items), span))
}

fn collect_module_items(
    tokens: &[Token],
    pos: usize,
    acc: Vec<ModuleItem>,
) -> Result<(Vec<ModuleItem>, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Ok((acc, pos)),
        Peek::Token(_) => {
            let (item, after_item) = parse_module_item(tokens, pos)?;
            let extended: Vec<ModuleItem> = acc.into_iter().chain(std::iter::once(item)).collect();
            collect_module_items(tokens, after_item, extended)
        }
    }
}

fn program_span(tokens: &[Token]) -> Span {
    let start = tokens
        .first()
        .map_or(Position::synthetic(), |t| t.span().start());
    let end = tokens.last().map_or(Position::synthetic(), |_| {
        span_at(tokens, tokens.len() - 1).end()
    });
    Span::new(start, end)
}
