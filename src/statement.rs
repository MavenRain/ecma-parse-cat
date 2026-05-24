//! Statement parsing.

use crate::error::Error;
use crate::expression::{parse_assignment_expression, parse_expression};
use crate::pattern::reinterpret_expression_as_pattern;
use crate::stream::{Peek, eat_semicolon, expect_identifier, expect_kind, is_kind, peek, span_at};
use ecma_lex_cat::token::{Token, TokenKind};
use ecma_syntax_cat::declaration::{VariableDeclaration, VariableKind};
use ecma_syntax_cat::span::Span;
use ecma_syntax_cat::statement::{
    CatchClause, ForInit, ForLeft, Statement, StatementKind, SwitchCase,
};

/// Parse a single statement.
///
/// # Errors
///
/// See [`Error`] variants.
pub fn parse_statement(tokens: &[Token], pos: usize) -> Result<(Statement, usize), Error> {
    let start_span = span_at(tokens, pos);
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "statement",
        }),
        Peek::Token(tok) => dispatch_statement(tokens, pos, tok, start_span),
    }
}

fn dispatch_statement(
    tokens: &[Token],
    pos: usize,
    tok: &Token,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    match tok.value() {
        TokenKind::LBrace => parse_block_statement(tokens, pos),
        TokenKind::Semicolon => Ok((Statement::new(StatementKind::Empty, start_span), pos + 1)),
        TokenKind::KwDebugger => parse_debugger(tokens, pos, start_span),
        TokenKind::KwIf => parse_if(tokens, pos, start_span),
        TokenKind::KwSwitch => parse_switch(tokens, pos, start_span),
        TokenKind::KwFor => parse_for(tokens, pos, start_span),
        TokenKind::KwWhile => parse_while(tokens, pos, start_span),
        TokenKind::KwDo => parse_do_while(tokens, pos, start_span),
        TokenKind::KwReturn => parse_return(tokens, pos, start_span),
        TokenKind::KwThrow => parse_throw(tokens, pos, start_span),
        TokenKind::KwTry => parse_try(tokens, pos, start_span),
        TokenKind::KwBreak => parse_break_or_continue(tokens, pos, start_span, true),
        TokenKind::KwContinue => parse_break_or_continue(tokens, pos, start_span, false),
        TokenKind::KwVar | TokenKind::KwConst => {
            parse_variable_declaration_statement(tokens, pos, start_span)
        }
        TokenKind::KwLet => parse_let_or_expression(tokens, pos, start_span),
        TokenKind::KwFunction => {
            let (func, after) = crate::declaration::parse_function_declaration(tokens, pos)?;
            let span = Span::new(start_span.start(), span_at(tokens, after - 1).end());
            Ok((
                Statement::new(StatementKind::FunctionDeclaration(func), span),
                after,
            ))
        }
        TokenKind::KwClass => {
            let (class, after) = crate::declaration::parse_class(tokens, pos)?;
            let span = Span::new(start_span.start(), span_at(tokens, after - 1).end());
            Ok((
                Statement::new(StatementKind::ClassDeclaration(class), span),
                after,
            ))
        }
        TokenKind::Identifier(name) if is_labeled_statement(tokens, pos, name) => {
            parse_labeled(tokens, pos, start_span)
        }
        _other => parse_expression_statement(tokens, pos, start_span),
    }
}

fn is_labeled_statement(tokens: &[Token], pos: usize, _name: &str) -> bool {
    is_kind(tokens, pos + 1, &TokenKind::Colon)
}

fn parse_labeled(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (label, after_label) = expect_identifier(tokens, pos)?;
    let after_colon = expect_kind(tokens, after_label, &TokenKind::Colon, "`:`")?;
    let (body, after_body) = parse_statement(tokens, after_colon)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_body - 1).end());
    Ok((
        Statement::new(
            StatementKind::Labeled {
                label,
                body: Box::new(body),
            },
            span,
        ),
        after_body,
    ))
}

fn parse_debugger(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let after_semi = eat_semicolon(tokens, after_kw)?;
    Ok((
        Statement::new(StatementKind::Debugger, start_span),
        after_semi,
    ))
}

/// Parse a `{ ... }` block body.  Returns the statements and the position
/// after the closing brace.  Used by function bodies, arrow bodies, and
/// nested blocks.
///
/// # Errors
///
/// See [`Error`].
pub fn parse_block_body(tokens: &[Token], pos: usize) -> Result<(Vec<Statement>, usize), Error> {
    let after_open = expect_kind(tokens, pos, &TokenKind::LBrace, "`{`")?;
    collect_block_statements(tokens, after_open, Vec::new())
}

fn collect_block_statements(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Statement>,
) -> Result<(Vec<Statement>, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace) {
        Ok((acc, pos + 1))
    } else {
        let (stmt, after_stmt) = parse_statement(tokens, pos)?;
        let extended: Vec<Statement> = acc.into_iter().chain(std::iter::once(stmt)).collect();
        collect_block_statements(tokens, after_stmt, extended)
    }
}

fn parse_block_statement(tokens: &[Token], pos: usize) -> Result<(Statement, usize), Error> {
    let start_span = span_at(tokens, pos);
    let (body, after_close) = parse_block_body(tokens, pos)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
    Ok((
        Statement::new(StatementKind::Block { body }, span),
        after_close,
    ))
}

fn parse_if(tokens: &[Token], pos: usize, start_span: Span) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let after_lparen = expect_kind(tokens, after_kw, &TokenKind::LParen, "`(`")?;
    let (test, after_test) = parse_expression(tokens, after_lparen)?;
    let after_rparen = expect_kind(tokens, after_test, &TokenKind::RParen, "`)`")?;
    let (consequent, after_conseq) = parse_statement(tokens, after_rparen)?;
    let (alternate, after_alt) = if is_kind(tokens, after_conseq, &TokenKind::KwElse) {
        let (alt_stmt, after_alt_stmt) = parse_statement(tokens, after_conseq + 1)?;
        (Some(Box::new(alt_stmt)), after_alt_stmt)
    } else {
        (None, after_conseq)
    };
    let span = Span::new(start_span.start(), span_at(tokens, after_alt - 1).end());
    Ok((
        Statement::new(
            StatementKind::If {
                test,
                consequent: Box::new(consequent),
                alternate,
            },
            span,
        ),
        after_alt,
    ))
}

fn parse_switch(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let after_lparen = expect_kind(tokens, after_kw, &TokenKind::LParen, "`(`")?;
    let (discriminant, after_disc) = parse_expression(tokens, after_lparen)?;
    let after_rparen = expect_kind(tokens, after_disc, &TokenKind::RParen, "`)`")?;
    let after_lbrace = expect_kind(tokens, after_rparen, &TokenKind::LBrace, "`{`")?;
    let (cases, after_rbrace) = collect_switch_cases(tokens, after_lbrace, Vec::new())?;
    let span = Span::new(start_span.start(), span_at(tokens, after_rbrace - 1).end());
    Ok((
        Statement::new(
            StatementKind::Switch {
                discriminant,
                cases,
            },
            span,
        ),
        after_rbrace,
    ))
}

fn collect_switch_cases(
    tokens: &[Token],
    pos: usize,
    acc: Vec<SwitchCase>,
) -> Result<(Vec<SwitchCase>, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace) {
        Ok((acc, pos + 1))
    } else if is_kind(tokens, pos, &TokenKind::KwCase) {
        let (test, after_test) = parse_expression(tokens, pos + 1)?;
        let after_colon = expect_kind(tokens, after_test, &TokenKind::Colon, "`:`")?;
        let (body, after_body) = collect_case_body(tokens, after_colon, Vec::new())?;
        let extended: Vec<SwitchCase> = acc
            .into_iter()
            .chain(std::iter::once(SwitchCase::new(Some(test), body)))
            .collect();
        collect_switch_cases(tokens, after_body, extended)
    } else if is_kind(tokens, pos, &TokenKind::KwDefault) {
        let after_colon = expect_kind(tokens, pos + 1, &TokenKind::Colon, "`:`")?;
        let (body, after_body) = collect_case_body(tokens, after_colon, Vec::new())?;
        let extended: Vec<SwitchCase> = acc
            .into_iter()
            .chain(std::iter::once(SwitchCase::new(None, body)))
            .collect();
        collect_switch_cases(tokens, after_body, extended)
    } else {
        let span = span_at(tokens, pos);
        Err(Error::UnexpectedToken {
            at: span,
            expected: "`case`, `default`, or `}`",
            found: render_kind(tokens, pos),
        })
    }
}

fn collect_case_body(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Statement>,
) -> Result<(Vec<Statement>, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace)
        || is_kind(tokens, pos, &TokenKind::KwCase)
        || is_kind(tokens, pos, &TokenKind::KwDefault)
    {
        Ok((acc, pos))
    } else {
        let (stmt, after_stmt) = parse_statement(tokens, pos)?;
        let extended: Vec<Statement> = acc.into_iter().chain(std::iter::once(stmt)).collect();
        collect_case_body(tokens, after_stmt, extended)
    }
}

fn render_kind(tokens: &[Token], pos: usize) -> String {
    match peek(tokens, pos) {
        Peek::Eof => "EOF".to_owned(),
        Peek::Token(tok) => format!("{}", tok.value()),
    }
}

fn parse_for(tokens: &[Token], pos: usize, start_span: Span) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let (is_for_await, after_await) = if is_kind(tokens, after_kw, &TokenKind::KwAwait) {
        (true, after_kw + 1)
    } else {
        (false, after_kw)
    };
    let after_lparen = expect_kind(tokens, after_await, &TokenKind::LParen, "`(`")?;
    parse_for_head(tokens, after_lparen, is_for_await, start_span)
}

fn parse_for_head(
    tokens: &[Token],
    pos: usize,
    is_for_await: bool,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::Semicolon) {
        finish_classic_for(tokens, pos + 1, None, start_span)
    } else if starts_declaration(tokens, pos) {
        parse_for_with_declaration(tokens, pos, is_for_await, start_span)
    } else {
        parse_for_with_expression(tokens, pos, is_for_await, start_span)
    }
}

fn starts_declaration(tokens: &[Token], pos: usize) -> bool {
    is_kind(tokens, pos, &TokenKind::KwVar)
        || is_kind(tokens, pos, &TokenKind::KwLet)
        || is_kind(tokens, pos, &TokenKind::KwConst)
}

fn parse_for_with_declaration(
    tokens: &[Token],
    pos: usize,
    is_for_await: bool,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (decl, after_decl) = parse_variable_declaration(tokens, pos)?;
    if is_kind(tokens, after_decl, &TokenKind::KwIn) {
        finish_for_in(
            tokens,
            after_decl + 1,
            ForLeft::Declaration(decl),
            start_span,
        )
    } else if is_in_of_position(tokens, after_decl) {
        finish_for_of(
            tokens,
            after_decl + 1,
            ForLeft::Declaration(decl),
            is_for_await,
            start_span,
        )
    } else {
        let after_semi = expect_kind(tokens, after_decl, &TokenKind::Semicolon, "`;`")?;
        finish_classic_for(
            tokens,
            after_semi,
            Some(ForInit::Declaration(decl)),
            start_span,
        )
    }
}

fn parse_for_with_expression(
    tokens: &[Token],
    pos: usize,
    is_for_await: bool,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (init_expr, after_expr) = parse_expression(tokens, pos)?;
    if is_kind(tokens, after_expr, &TokenKind::KwIn) {
        let pattern = reinterpret_expression_as_pattern(init_expr)?;
        finish_for_in(
            tokens,
            after_expr + 1,
            ForLeft::Pattern(pattern),
            start_span,
        )
    } else if is_in_of_position(tokens, after_expr) {
        let pattern = reinterpret_expression_as_pattern(init_expr)?;
        finish_for_of(
            tokens,
            after_expr + 1,
            ForLeft::Pattern(pattern),
            is_for_await,
            start_span,
        )
    } else {
        let after_semi = expect_kind(tokens, after_expr, &TokenKind::Semicolon, "`;`")?;
        finish_classic_for(
            tokens,
            after_semi,
            Some(ForInit::Expression(init_expr)),
            start_span,
        )
    }
}

fn is_in_of_position(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(name) if name == "of")
    )
}

fn finish_classic_for(
    tokens: &[Token],
    pos: usize,
    init: Option<ForInit>,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (test, after_test) = if is_kind(tokens, pos, &TokenKind::Semicolon) {
        (None, pos)
    } else {
        let (expr, after_expr) = parse_expression(tokens, pos)?;
        (Some(expr), after_expr)
    };
    let after_test_semi = expect_kind(tokens, after_test, &TokenKind::Semicolon, "`;`")?;
    let (update, after_update) = if is_kind(tokens, after_test_semi, &TokenKind::RParen) {
        (None, after_test_semi)
    } else {
        let (expr, after_expr) = parse_expression(tokens, after_test_semi)?;
        (Some(expr), after_expr)
    };
    let after_rparen = expect_kind(tokens, after_update, &TokenKind::RParen, "`)`")?;
    let (body, after_body) = parse_statement(tokens, after_rparen)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_body - 1).end());
    Ok((
        Statement::new(
            StatementKind::For {
                init,
                test,
                update,
                body: Box::new(body),
            },
            span,
        ),
        after_body,
    ))
}

fn finish_for_in(
    tokens: &[Token],
    pos: usize,
    left: ForLeft,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (right, after_right) = parse_expression(tokens, pos)?;
    let after_rparen = expect_kind(tokens, after_right, &TokenKind::RParen, "`)`")?;
    let (body, after_body) = parse_statement(tokens, after_rparen)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_body - 1).end());
    Ok((
        Statement::new(
            StatementKind::ForIn {
                left,
                right,
                body: Box::new(body),
            },
            span,
        ),
        after_body,
    ))
}

fn finish_for_of(
    tokens: &[Token],
    pos: usize,
    left: ForLeft,
    is_await: bool,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (right, after_right) = parse_assignment_expression(tokens, pos)?;
    let after_rparen = expect_kind(tokens, after_right, &TokenKind::RParen, "`)`")?;
    let (body, after_body) = parse_statement(tokens, after_rparen)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_body - 1).end());
    Ok((
        Statement::new(
            StatementKind::ForOf {
                left,
                right,
                body: Box::new(body),
                is_await,
            },
            span,
        ),
        after_body,
    ))
}

fn parse_while(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let after_lparen = expect_kind(tokens, after_kw, &TokenKind::LParen, "`(`")?;
    let (test, after_test) = parse_expression(tokens, after_lparen)?;
    let after_rparen = expect_kind(tokens, after_test, &TokenKind::RParen, "`)`")?;
    let (body, after_body) = parse_statement(tokens, after_rparen)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_body - 1).end());
    Ok((
        Statement::new(
            StatementKind::While {
                test,
                body: Box::new(body),
            },
            span,
        ),
        after_body,
    ))
}

fn parse_do_while(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let (body, after_body) = parse_statement(tokens, after_kw)?;
    let after_while = expect_kind(tokens, after_body, &TokenKind::KwWhile, "keyword `while`")?;
    let after_lparen = expect_kind(tokens, after_while, &TokenKind::LParen, "`(`")?;
    let (test, after_test) = parse_expression(tokens, after_lparen)?;
    let after_rparen = expect_kind(tokens, after_test, &TokenKind::RParen, "`)`")?;
    let after_semi = eat_semicolon(tokens, after_rparen)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_rparen - 1).end());
    Ok((
        Statement::new(
            StatementKind::DoWhile {
                body: Box::new(body),
                test,
            },
            span,
        ),
        after_semi,
    ))
}

fn parse_return(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let (argument, after_arg) = if can_start_return_argument(tokens, after_kw) {
        let (expr, after_expr) = parse_expression(tokens, after_kw)?;
        (Some(expr), after_expr)
    } else {
        (None, after_kw)
    };
    let after_semi = eat_semicolon(tokens, after_arg)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_arg - 1).end());
    Ok((
        Statement::new(StatementKind::Return { argument }, span),
        after_semi,
    ))
}

fn can_start_return_argument(tokens: &[Token], pos: usize) -> bool {
    match peek(tokens, pos) {
        Peek::Eof => false,
        Peek::Token(tok) => !matches!(
            tok.value(),
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ),
    }
}

fn parse_throw(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let (argument, after_arg) = parse_expression(tokens, after_kw)?;
    let after_semi = eat_semicolon(tokens, after_arg)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_arg - 1).end());
    Ok((
        Statement::new(StatementKind::Throw { argument }, span),
        after_semi,
    ))
}

fn parse_try(tokens: &[Token], pos: usize, start_span: Span) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let (block, after_block) = parse_block_body(tokens, after_kw)?;
    let (handler, after_handler) = if is_kind(tokens, after_block, &TokenKind::KwCatch) {
        let (clause, after_clause) = parse_catch_clause(tokens, after_block)?;
        (Some(clause), after_clause)
    } else {
        (None, after_block)
    };
    let (finalizer, after_final) = if is_kind(tokens, after_handler, &TokenKind::KwFinally) {
        let after_finally = after_handler + 1;
        let (body, after_body) = parse_block_body(tokens, after_finally)?;
        (Some(body), after_body)
    } else {
        (None, after_handler)
    };
    let span = Span::new(start_span.start(), span_at(tokens, after_final - 1).end());
    Ok((
        Statement::new(
            StatementKind::Try {
                block,
                handler,
                finalizer,
            },
            span,
        ),
        after_final,
    ))
}

fn parse_catch_clause(tokens: &[Token], pos: usize) -> Result<(CatchClause, usize), Error> {
    let after_kw = pos + 1;
    let (param, after_param) = if is_kind(tokens, after_kw, &TokenKind::LParen) {
        let after_lparen = after_kw + 1;
        let (pattern, after_pattern) = parse_binding_pattern(tokens, after_lparen)?;
        let after_rparen = expect_kind(tokens, after_pattern, &TokenKind::RParen, "`)`")?;
        (Some(pattern), after_rparen)
    } else {
        (None, after_kw)
    };
    let (body, after_body) = parse_block_body(tokens, after_param)?;
    Ok((CatchClause::new(param, body), after_body))
}

fn parse_break_or_continue(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
    is_break: bool,
) -> Result<(Statement, usize), Error> {
    let after_kw = pos + 1;
    let (label, after_label) = if can_start_label(tokens, after_kw) {
        let (id, after_id) = expect_identifier(tokens, after_kw)?;
        (Some(id), after_id)
    } else {
        (None, after_kw)
    };
    let after_semi = eat_semicolon(tokens, after_label)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_label - 1).end());
    let kind = if is_break {
        StatementKind::Break { label }
    } else {
        StatementKind::Continue { label }
    };
    Ok((Statement::new(kind, span), after_semi))
}

fn can_start_label(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(_))
    )
}

fn parse_let_or_expression(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    if looks_like_let_declaration(tokens, pos) {
        parse_variable_declaration_statement(tokens, pos, start_span)
    } else {
        parse_expression_statement(tokens, pos, start_span)
    }
}

fn parse_variable_declaration_statement(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (decl, after) = parse_variable_declaration(tokens, pos)?;
    let after_semi = eat_semicolon(tokens, after)?;
    let span = Span::new(start_span.start(), span_at(tokens, after - 1).end());
    Ok((
        Statement::new(StatementKind::VariableDeclaration(decl), span),
        after_semi,
    ))
}

fn looks_like_let_declaration(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos + 1),
        Peek::Token(t) if matches!(
            t.value(),
            TokenKind::Identifier(_)
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::KwYield
                | TokenKind::KwAwait
        )
    )
}

fn parse_expression_statement(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Statement, usize), Error> {
    let (expression, after_expr) = parse_expression(tokens, pos)?;
    let after_semi = eat_semicolon(tokens, after_expr)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_expr - 1).end());
    Ok((
        Statement::new(StatementKind::Expression { expression }, span),
        after_semi,
    ))
}

/// Parse a `var` / `let` / `const` declaration up to and including the last
/// declarator (not the trailing `;`).
///
/// # Errors
///
/// See [`Error`].
pub fn parse_variable_declaration(
    tokens: &[Token],
    pos: usize,
) -> Result<(VariableDeclaration, usize), Error> {
    let (kind, after_kw) = match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "`var`, `let`, or `const`",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::KwVar => Ok((VariableKind::Var, pos + 1)),
            TokenKind::KwLet => Ok((VariableKind::Let, pos + 1)),
            TokenKind::KwConst => Ok((VariableKind::Const, pos + 1)),
            _other => Err(Error::UnexpectedToken {
                at: tok.span(),
                expected: "`var`, `let`, or `const`",
                found: format!("{}", tok.value()),
            }),
        },
    }?;
    let (declarators, after_decls) = collect_declarators(tokens, after_kw, Vec::new())?;
    Ok((VariableDeclaration::new(kind, declarators), after_decls))
}

fn collect_declarators(
    tokens: &[Token],
    pos: usize,
    acc: Vec<ecma_syntax_cat::declaration::VariableDeclarator>,
) -> Result<(Vec<ecma_syntax_cat::declaration::VariableDeclarator>, usize), Error> {
    let (id, after_id) = parse_binding_pattern(tokens, pos)?;
    let (init, after_init) = if is_kind(tokens, after_id, &TokenKind::Eq) {
        let (expr, after_expr) = parse_assignment_expression(tokens, after_id + 1)?;
        (Some(expr), after_expr)
    } else {
        (None, after_id)
    };
    let declarator = ecma_syntax_cat::declaration::VariableDeclarator::new(id, init);
    let extended: Vec<ecma_syntax_cat::declaration::VariableDeclarator> =
        acc.into_iter().chain(std::iter::once(declarator)).collect();
    if is_kind(tokens, after_init, &TokenKind::Comma) {
        collect_declarators(tokens, after_init + 1, extended)
    } else {
        Ok((extended, after_init))
    }
}

/// Parse a binding pattern (identifier, array pattern, or object pattern).
///
/// # Errors
///
/// See [`Error`].
pub fn parse_binding_pattern(
    tokens: &[Token],
    pos: usize,
) -> Result<(ecma_syntax_cat::pattern::Pattern, usize), Error> {
    let start_span = span_at(tokens, pos);
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "binding pattern",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::Identifier(_)
            | TokenKind::KwLet
            | TokenKind::KwYield
            | TokenKind::KwAwait => {
                let (id, after) = expect_identifier(tokens, pos)?;
                Ok((
                    ecma_syntax_cat::pattern::Pattern::new(
                        ecma_syntax_cat::pattern::PatternKind::Identifier(id),
                        start_span,
                    ),
                    after,
                ))
            }
            TokenKind::LBracket => parse_array_binding_pattern(tokens, pos),
            TokenKind::LBrace => parse_object_binding_pattern(tokens, pos),
            _other => Err(Error::UnexpectedToken {
                at: tok.span(),
                expected: "binding pattern",
                found: format!("{}", tok.value()),
            }),
        },
    }
    .and_then(|(pattern, after)| maybe_with_default(tokens, after, pattern))
}

fn maybe_with_default(
    tokens: &[Token],
    pos: usize,
    pattern: ecma_syntax_cat::pattern::Pattern,
) -> Result<(ecma_syntax_cat::pattern::Pattern, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::Eq) {
        let (default, after_default) = parse_assignment_expression(tokens, pos + 1)?;
        let span = Span::new(pattern.span().start(), default.span().end());
        Ok((
            ecma_syntax_cat::pattern::Pattern::new(
                ecma_syntax_cat::pattern::PatternKind::Assignment {
                    left: Box::new(pattern),
                    right: Box::new(default),
                },
                span,
            ),
            after_default,
        ))
    } else {
        Ok((pattern, pos))
    }
}

fn parse_array_binding_pattern(
    tokens: &[Token],
    pos: usize,
) -> Result<(ecma_syntax_cat::pattern::Pattern, usize), Error> {
    let start_span = span_at(tokens, pos);
    collect_array_pattern(tokens, pos + 1, Vec::new(), start_span)
}

fn collect_array_pattern(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Option<ecma_syntax_cat::pattern::Pattern>>,
    start_span: Span,
) -> Result<(ecma_syntax_cat::pattern::Pattern, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBracket) {
        let span = Span::new(start_span.start(), span_at(tokens, pos).end());
        Ok((
            ecma_syntax_cat::pattern::Pattern::new(
                ecma_syntax_cat::pattern::PatternKind::Array { elements: acc },
                span,
            ),
            pos + 1,
        ))
    } else if is_kind(tokens, pos, &TokenKind::Comma) {
        let extended: Vec<Option<ecma_syntax_cat::pattern::Pattern>> =
            acc.into_iter().chain(std::iter::once(None)).collect();
        collect_array_pattern(tokens, pos + 1, extended, start_span)
    } else if is_kind(tokens, pos, &TokenKind::Spread) {
        let span = span_at(tokens, pos);
        let (inner, after_inner) = parse_binding_pattern(tokens, pos + 1)?;
        let rest_span = Span::new(span.start(), inner.span().end());
        let rest = ecma_syntax_cat::pattern::Pattern::new(
            ecma_syntax_cat::pattern::PatternKind::Rest {
                argument: Box::new(inner),
            },
            rest_span,
        );
        let extended: Vec<Option<ecma_syntax_cat::pattern::Pattern>> =
            acc.into_iter().chain(std::iter::once(Some(rest))).collect();
        let after_close = expect_kind(tokens, after_inner, &TokenKind::RBracket, "`]`")?;
        let full = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
        Ok((
            ecma_syntax_cat::pattern::Pattern::new(
                ecma_syntax_cat::pattern::PatternKind::Array { elements: extended },
                full,
            ),
            after_close,
        ))
    } else {
        let (pat, after_pat) = parse_binding_pattern(tokens, pos)?;
        let extended: Vec<Option<ecma_syntax_cat::pattern::Pattern>> =
            acc.into_iter().chain(std::iter::once(Some(pat))).collect();
        if is_kind(tokens, after_pat, &TokenKind::Comma) {
            collect_array_pattern(tokens, after_pat + 1, extended, start_span)
        } else {
            let after_close = expect_kind(tokens, after_pat, &TokenKind::RBracket, "`]`")?;
            let full = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
            Ok((
                ecma_syntax_cat::pattern::Pattern::new(
                    ecma_syntax_cat::pattern::PatternKind::Array { elements: extended },
                    full,
                ),
                after_close,
            ))
        }
    }
}

fn parse_object_binding_pattern(
    tokens: &[Token],
    pos: usize,
) -> Result<(ecma_syntax_cat::pattern::Pattern, usize), Error> {
    let start_span = span_at(tokens, pos);
    collect_object_pattern(tokens, pos + 1, Vec::new(), start_span)
}

fn collect_object_pattern(
    tokens: &[Token],
    pos: usize,
    acc: Vec<ecma_syntax_cat::pattern::ObjectPatternMember>,
    start_span: Span,
) -> Result<(ecma_syntax_cat::pattern::Pattern, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace) {
        let span = Span::new(start_span.start(), span_at(tokens, pos).end());
        Ok((
            ecma_syntax_cat::pattern::Pattern::new(
                ecma_syntax_cat::pattern::PatternKind::Object { properties: acc },
                span,
            ),
            pos + 1,
        ))
    } else if is_kind(tokens, pos, &TokenKind::Spread) {
        let (inner, after_inner) = parse_binding_pattern(tokens, pos + 1)?;
        let rest = ecma_syntax_cat::pattern::ObjectPatternMember::Rest { argument: inner };
        let extended: Vec<ecma_syntax_cat::pattern::ObjectPatternMember> =
            acc.into_iter().chain(std::iter::once(rest)).collect();
        let after_close = expect_kind(tokens, after_inner, &TokenKind::RBrace, "`}`")?;
        let full = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
        Ok((
            ecma_syntax_cat::pattern::Pattern::new(
                ecma_syntax_cat::pattern::PatternKind::Object {
                    properties: extended,
                },
                full,
            ),
            after_close,
        ))
    } else {
        let (member, after_member) = parse_object_pattern_member(tokens, pos)?;
        let extended: Vec<ecma_syntax_cat::pattern::ObjectPatternMember> =
            acc.into_iter().chain(std::iter::once(member)).collect();
        if is_kind(tokens, after_member, &TokenKind::Comma) {
            collect_object_pattern(tokens, after_member + 1, extended, start_span)
        } else {
            let after_close = expect_kind(tokens, after_member, &TokenKind::RBrace, "`}`")?;
            let full = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
            Ok((
                ecma_syntax_cat::pattern::Pattern::new(
                    ecma_syntax_cat::pattern::PatternKind::Object {
                        properties: extended,
                    },
                    full,
                ),
                after_close,
            ))
        }
    }
}

fn parse_object_pattern_member(
    tokens: &[Token],
    pos: usize,
) -> Result<(ecma_syntax_cat::pattern::ObjectPatternMember, usize), Error> {
    let computed = is_kind(tokens, pos, &TokenKind::LBracket);
    let (key, after_key) = if computed {
        let (inner, after_inner) = parse_assignment_expression(tokens, pos + 1)?;
        let after_close = expect_kind(tokens, after_inner, &TokenKind::RBracket, "`]`")?;
        (
            ecma_syntax_cat::expression::PropertyKey::Computed(Box::new(inner)),
            after_close,
        )
    } else {
        let (id, after_id) = expect_identifier(tokens, pos)?;
        (
            ecma_syntax_cat::expression::PropertyKey::Identifier(id),
            after_id,
        )
    };
    if is_kind(tokens, after_key, &TokenKind::Colon) {
        let (value, after_value) = parse_binding_pattern(tokens, after_key + 1)?;
        Ok((
            ecma_syntax_cat::pattern::ObjectPatternMember::Property {
                key,
                value,
                computed,
                shorthand: false,
            },
            after_value,
        ))
    } else {
        let shorthand_value = key_to_pattern(&key, span_at(tokens, pos))?;
        let (final_value, after_final) = maybe_with_default(tokens, after_key, shorthand_value)?;
        Ok((
            ecma_syntax_cat::pattern::ObjectPatternMember::Property {
                key,
                value: final_value,
                computed,
                shorthand: true,
            },
            after_final,
        ))
    }
}

fn key_to_pattern(
    key: &ecma_syntax_cat::expression::PropertyKey,
    span: Span,
) -> Result<ecma_syntax_cat::pattern::Pattern, Error> {
    match key {
        ecma_syntax_cat::expression::PropertyKey::Identifier(id) => {
            Ok(ecma_syntax_cat::pattern::Pattern::new(
                ecma_syntax_cat::pattern::PatternKind::Identifier(id.clone()),
                span,
            ))
        }
        ecma_syntax_cat::expression::PropertyKey::String(_)
        | ecma_syntax_cat::expression::PropertyKey::Number(_)
        | ecma_syntax_cat::expression::PropertyKey::Computed(_)
        | ecma_syntax_cat::expression::PropertyKey::Private(_) => Err(Error::UnexpectedToken {
            at: span,
            expected: "identifier shorthand key",
            found: "non-identifier key without `:` value".to_owned(),
        }),
    }
}
