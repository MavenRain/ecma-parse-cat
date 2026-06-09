//! Expression parsing.

use crate::error::Error;
use crate::pattern::reinterpret_expression_as_arrow_params;
use crate::precedence::{BinaryInfo, BinaryKind, MIN_PRECEDENCE, assignment_operator, binary_info};
use crate::stream::{
    Peek, expect_identifier, expect_identifier_or_keyword, expect_kind, expect_member_name,
    expect_private_identifier, is_kind, peek, span_at, span_before,
};
use ecma_lex_cat::token::{Token, TokenKind};
use ecma_syntax_cat::expression::{
    Expression, ExpressionKind, MemberProperty, ObjectMember, ObjectPropertyKind, PropertyKey,
};
use ecma_syntax_cat::function::{ArrowBody, ArrowFunction};
use ecma_syntax_cat::literal::Literal;
use ecma_syntax_cat::operator::{UnaryOperator, UpdateOperator};
use ecma_syntax_cat::pattern::Pattern;
use ecma_syntax_cat::span::Span;

/// Parse a full expression, accepting comma-sequence at the top level.
pub fn parse_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let (first, after_first) = parse_assignment_expression(tokens, pos)?;
    parse_sequence_tail(tokens, after_first, first)
}

fn parse_sequence_tail(
    tokens: &[Token],
    pos: usize,
    first: Expression,
) -> Result<(Expression, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::Comma) {
        collect_sequence(tokens, pos, vec![first])
    } else {
        Ok((first, pos))
    }
}

fn collect_sequence(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Expression>,
) -> Result<(Expression, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::Comma) {
        let (next, after_next) = parse_assignment_expression(tokens, pos + 1)?;
        let extended: Vec<Expression> = acc.into_iter().chain(std::iter::once(next)).collect();
        collect_sequence(tokens, after_next, extended)
    } else {
        let span = sequence_span(&acc);
        Ok((
            Expression::new(ExpressionKind::Sequence { expressions: acc }, span),
            pos,
        ))
    }
}

fn sequence_span(exprs: &[Expression]) -> Span {
    let start = exprs.first().map_or(Span::synthetic(), Expression::span);
    let end = exprs.last().map_or(Span::synthetic(), Expression::span);
    Span::new(start.start(), end.end())
}

/// Parse an expression that does not include the top-level comma.
pub fn parse_assignment_expression(
    tokens: &[Token],
    pos: usize,
) -> Result<(Expression, usize), Error> {
    if let Some(parsed) = try_parse_arrow_starter(tokens, pos)? {
        Ok(parsed)
    } else {
        parse_assignment_tail(tokens, pos)
    }
}

fn parse_assignment_tail(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let (left, after_left) = parse_conditional_expression(tokens, pos)?;
    if is_kind(tokens, after_left, &TokenKind::Arrow) {
        // v0.3: `async (a, b) => ...` is parsed by
        // `parse_conditional_expression` as a call expression
        // (`async(a, b)`); when an arrow follows we extract the
        // call's arguments as arrow parameters and mark the arrow
        // async.  Falls through to the synchronous arrow path
        // otherwise.
        let async_params = try_async_call_as_arrow_params(&left)?;
        let (params, is_async) = async_params.map_or_else(
            || reinterpret_expression_as_arrow_params(left).map(|p| (p, false)),
            |p| Ok((p, true)),
        )?;
        finish_arrow(
            tokens,
            after_left + 1,
            params,
            is_async,
            span_at(tokens, pos),
        )
    } else if let Some(op) = peek_assignment_operator(tokens, after_left) {
        let (right, after_right) = parse_assignment_expression(tokens, after_left + 1)?;
        let span = combined_span(left.span(), right.span());
        Ok((
            Expression::new(
                ExpressionKind::Assignment {
                    operator: op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            ),
            after_right,
        ))
    } else {
        Ok((left, after_left))
    }
}

fn peek_assignment_operator(
    tokens: &[Token],
    pos: usize,
) -> Option<ecma_syntax_cat::operator::AssignmentOperator> {
    match peek(tokens, pos) {
        Peek::Eof => None,
        Peek::Token(tok) => assignment_operator(tok.value()),
    }
}

/// Try to parse a form that can only be an arrow function: `async <ident> =>`,
/// `async (...) =>`, or `() =>`.  Returns `Ok(Some(_))` if it consumed an
/// arrow form, `Ok(None)` if the input is not an arrow starter.
fn try_parse_arrow_starter(
    tokens: &[Token],
    pos: usize,
) -> Result<Option<(Expression, usize)>, Error> {
    if is_empty_paren_arrow(tokens, pos) {
        let after_open = pos + 1;
        let after_close = after_open + 1;
        let after_arrow = expect_kind(tokens, after_close, &TokenKind::Arrow, "`=>`")?;
        let span = span_at(tokens, pos);
        finish_arrow(tokens, after_arrow, Vec::new(), false, span).map(Some)
    } else if is_async_function(tokens, pos) {
        let after_async = pos + 1;
        parse_function_expression(tokens, after_async, true).map(Some)
    } else if is_async_single_param_arrow(tokens, pos) {
        // v0.3: `async ident => body` -- one-parameter async arrow.
        // The two-token lookahead at `is_async_single_param_arrow`
        // already confirmed `async` + identifier + `=>` so we can
        // build the pattern directly here.
        parse_async_single_param_arrow(tokens, pos).map(Some)
    } else {
        Ok(None)
    }
}

fn is_async_single_param_arrow(tokens: &[Token], pos: usize) -> bool {
    let is_async = matches!(
        peek(tokens, pos),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(name) if name == "async")
    );
    let is_ident_after = matches!(
        peek(tokens, pos + 1),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(_))
    );
    let is_arrow_after = is_kind(tokens, pos + 2, &TokenKind::Arrow);
    is_async && is_ident_after && is_arrow_after
}

fn parse_async_single_param_arrow(
    tokens: &[Token],
    pos: usize,
) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    let (param_id, after_param) = expect_identifier(tokens, pos + 1)?;
    let param_span = span_before(tokens, after_param);
    let after_arrow = expect_kind(tokens, after_param, &TokenKind::Arrow, "`=>`")?;
    let pattern = ecma_syntax_cat::pattern::Pattern::new(
        ecma_syntax_cat::pattern::PatternKind::Identifier(param_id),
        param_span,
    );
    finish_arrow(tokens, after_arrow, vec![pattern], true, start_span)
}

/// v0.3: if `expr` is a call expression `async(a, b, ...)` (i.e.
/// the callee is the bare identifier `async`), return its arguments
/// reinterpreted as arrow parameters.  Used by `parse_assignment_tail`
/// to recognise `async (a, b) => ...` after the conditional
/// expression already consumed the call form.  Returns `Ok(None)`
/// when `expr` isn't this shape (the caller falls through to the
/// synchronous arrow path).
fn try_async_call_as_arrow_params(expr: &Expression) -> Result<Option<Vec<Pattern>>, Error> {
    let async_args = async_call_arguments(expr);
    async_args
        .map(|arguments| {
            arguments
                .iter()
                .map(|arg| reinterpret_expression_as_arrow_params(arg.clone()).map(into_singleton))
                .collect::<Result<Vec<_>, _>>()
                .map(|nested| nested.into_iter().flatten().collect())
        })
        .transpose()
}

/// Return the argument list when `expr` is a call whose callee is
/// the bare identifier `async`.  Used by
/// [`try_async_call_as_arrow_params`] as the predicate half of the
/// `async (a, b) =>` cover-grammar refinement.
fn async_call_arguments(expr: &Expression) -> Option<&Vec<Expression>> {
    if let ExpressionKind::Call {
        callee, arguments, ..
    } = expr.value()
    {
        if let ExpressionKind::Identifier(id) = callee.value() {
            if id.as_str() == "async" {
                Some(arguments)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

fn into_singleton(patterns: Vec<Pattern>) -> Vec<Pattern> {
    patterns
}

fn is_empty_paren_arrow(tokens: &[Token], pos: usize) -> bool {
    is_kind(tokens, pos, &TokenKind::LParen)
        && is_kind(tokens, pos + 1, &TokenKind::RParen)
        && is_kind(tokens, pos + 2, &TokenKind::Arrow)
}

fn is_async_function(tokens: &[Token], pos: usize) -> bool {
    match peek(tokens, pos) {
        Peek::Eof => false,
        Peek::Token(tok) => match tok.value() {
            TokenKind::Identifier(name) if name == "async" => {
                is_kind(tokens, pos + 1, &TokenKind::KwFunction)
            }
            _other => false,
        },
    }
}

fn finish_arrow(
    tokens: &[Token],
    pos: usize,
    params: Vec<Pattern>,
    is_async: bool,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    let (body, end_pos, body_span) = parse_arrow_body(tokens, pos)?;
    let arrow = ArrowFunction::new(params, body, is_async);
    let span = Span::new(start_span.start(), body_span.end());
    Ok((
        Expression::new(ExpressionKind::ArrowFunction(Box::new(arrow)), span),
        end_pos,
    ))
}

fn parse_arrow_body(tokens: &[Token], pos: usize) -> Result<(ArrowBody, usize, Span), Error> {
    if is_kind(tokens, pos, &TokenKind::LBrace) {
        let (stmts, after_close) = crate::statement::parse_block_body(tokens, pos)?;
        let span = span_at(tokens, pos);
        Ok((ArrowBody::Block(stmts), after_close, span))
    } else {
        let (expr, after_expr) = parse_assignment_expression(tokens, pos)?;
        let span = expr.span();
        Ok((ArrowBody::Expression(Box::new(expr)), after_expr, span))
    }
}

fn parse_conditional_expression(
    tokens: &[Token],
    pos: usize,
) -> Result<(Expression, usize), Error> {
    let (test, after_test) = parse_binary_expression(tokens, pos, MIN_PRECEDENCE)?;
    if is_kind(tokens, after_test, &TokenKind::Question) {
        let (consequent, after_conseq) = parse_assignment_expression(tokens, after_test + 1)?;
        let after_colon = expect_kind(tokens, after_conseq, &TokenKind::Colon, "`:`")?;
        let (alternate, after_alt) = parse_assignment_expression(tokens, after_colon)?;
        let span = combined_span(test.span(), alternate.span());
        Ok((
            Expression::new(
                ExpressionKind::Conditional {
                    test: Box::new(test),
                    consequent: Box::new(consequent),
                    alternate: Box::new(alternate),
                },
                span,
            ),
            after_alt,
        ))
    } else {
        Ok((test, after_test))
    }
}

fn parse_binary_expression(
    tokens: &[Token],
    pos: usize,
    min_prec: u8,
) -> Result<(Expression, usize), Error> {
    let (lhs, after_lhs) = parse_unary_expression(tokens, pos)?;
    parse_binary_tail(tokens, after_lhs, lhs, min_prec)
}

fn parse_binary_tail(
    tokens: &[Token],
    pos: usize,
    lhs: Expression,
    min_prec: u8,
) -> Result<(Expression, usize), Error> {
    let info = peek_binary_info(tokens, pos);
    match info {
        None => Ok((lhs, pos)),
        Some(BinaryInfo { precedence, .. }) if precedence < min_prec => Ok((lhs, pos)),
        Some(info) => {
            let next_min = if info.right_assoc {
                info.precedence
            } else {
                info.precedence + 1
            };
            let (rhs, after_rhs) = parse_binary_expression(tokens, pos + 1, next_min)?;
            let combined = build_binary(info.op, lhs, rhs);
            parse_binary_tail(tokens, after_rhs, combined, min_prec)
        }
    }
}

fn peek_binary_info(tokens: &[Token], pos: usize) -> Option<BinaryInfo> {
    match peek(tokens, pos) {
        Peek::Eof => None,
        Peek::Token(tok) => binary_info(tok.value()),
    }
}

fn build_binary(op: BinaryKind, lhs: Expression, rhs: Expression) -> Expression {
    let span = combined_span(lhs.span(), rhs.span());
    let kind = match op {
        BinaryKind::Binary(binary_op) => ExpressionKind::Binary {
            operator: binary_op,
            left: Box::new(lhs),
            right: Box::new(rhs),
        },
        BinaryKind::Logical(logical_op) => ExpressionKind::Logical {
            operator: logical_op,
            left: Box::new(lhs),
            right: Box::new(rhs),
        },
    };
    Expression::new(kind, span)
}

fn combined_span(a: Span, b: Span) -> Span {
    Span::new(a.start(), b.end())
}

fn parse_unary_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "expression",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::KwDelete => {
                parse_unary_prefix(tokens, pos, UnaryOperator::Delete, start_span)
            }
            TokenKind::KwVoid => parse_unary_prefix(tokens, pos, UnaryOperator::Void, start_span),
            TokenKind::KwTypeof => {
                parse_unary_prefix(tokens, pos, UnaryOperator::TypeOf, start_span)
            }
            TokenKind::Plus => parse_unary_prefix(tokens, pos, UnaryOperator::Plus, start_span),
            TokenKind::Minus => parse_unary_prefix(tokens, pos, UnaryOperator::Minus, start_span),
            TokenKind::Tilde => {
                parse_unary_prefix(tokens, pos, UnaryOperator::BitwiseNot, start_span)
            }
            TokenKind::Bang => {
                parse_unary_prefix(tokens, pos, UnaryOperator::LogicalNot, start_span)
            }
            TokenKind::PlusPlus => {
                parse_update_prefix(tokens, pos, UpdateOperator::Increment, start_span)
            }
            TokenKind::MinusMinus => {
                parse_update_prefix(tokens, pos, UpdateOperator::Decrement, start_span)
            }
            TokenKind::KwAwait => parse_await(tokens, pos, start_span),
            TokenKind::KwYield => parse_yield(tokens, pos, start_span),
            _other => parse_update_expression(tokens, pos),
        },
    }
}

fn parse_unary_prefix(
    tokens: &[Token],
    pos: usize,
    operator: UnaryOperator,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    let (operand, after) = parse_unary_expression(tokens, pos + 1)?;
    let span = Span::new(start_span.start(), operand.span().end());
    Ok((
        Expression::new(
            ExpressionKind::Unary {
                operator,
                argument: Box::new(operand),
            },
            span,
        ),
        after,
    ))
}

fn parse_update_prefix(
    tokens: &[Token],
    pos: usize,
    operator: UpdateOperator,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    let (operand, after) = parse_unary_expression(tokens, pos + 1)?;
    let span = Span::new(start_span.start(), operand.span().end());
    Ok((
        Expression::new(
            ExpressionKind::Update {
                operator,
                argument: Box::new(operand),
                prefix: true,
            },
            span,
        ),
        after,
    ))
}

fn parse_await(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    let (arg, after) = parse_unary_expression(tokens, pos + 1)?;
    let span = Span::new(start_span.start(), arg.span().end());
    Ok((
        Expression::new(
            ExpressionKind::Await {
                argument: Box::new(arg),
            },
            span,
        ),
        after,
    ))
}

fn parse_yield(
    tokens: &[Token],
    pos: usize,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    let after_kw = pos + 1;
    let delegate = is_kind(tokens, after_kw, &TokenKind::Star);
    let after_star = if delegate { after_kw + 1 } else { after_kw };
    let yields_arg = starts_expression(tokens, after_star);
    let (argument, end_pos, end_span) = if yields_arg {
        let (expr, after_expr) = parse_assignment_expression(tokens, after_star)?;
        let s = expr.span();
        (Some(Box::new(expr)), after_expr, s)
    } else {
        (None, after_star, start_span)
    };
    let span = Span::new(start_span.start(), end_span.end());
    Ok((
        Expression::new(ExpressionKind::Yield { argument, delegate }, span),
        end_pos,
    ))
}

fn starts_expression(tokens: &[Token], pos: usize) -> bool {
    match peek(tokens, pos) {
        Peek::Eof => false,
        Peek::Token(tok) => match tok.value() {
            TokenKind::Semicolon
            | TokenKind::Comma
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::Colon
            | TokenKind::Eof => false,
            _other => true,
        },
    }
}

fn parse_update_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let (operand, after) = parse_lhs_expression(tokens, pos)?;
    if is_kind(tokens, after, &TokenKind::PlusPlus) {
        let span = Span::new(operand.span().start(), span_at(tokens, after).end());
        Ok((
            Expression::new(
                ExpressionKind::Update {
                    operator: UpdateOperator::Increment,
                    argument: Box::new(operand),
                    prefix: false,
                },
                span,
            ),
            after + 1,
        ))
    } else if is_kind(tokens, after, &TokenKind::MinusMinus) {
        let span = Span::new(operand.span().start(), span_at(tokens, after).end());
        Ok((
            Expression::new(
                ExpressionKind::Update {
                    operator: UpdateOperator::Decrement,
                    argument: Box::new(operand),
                    prefix: false,
                },
                span,
            ),
            after + 1,
        ))
    } else {
        Ok((operand, after))
    }
}

fn parse_lhs_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let (lhs, after) = parse_new_or_member(tokens, pos)?;
    parse_call_tail(tokens, after, lhs)
}

fn parse_new_or_member(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::KwNew) {
        parse_new_expression(tokens, pos)
    } else {
        parse_member_expression(tokens, pos)
    }
}

fn parse_new_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    let after_new = pos + 1;
    if is_kind(tokens, after_new, &TokenKind::Dot)
        && matches!(
            peek(tokens, after_new + 1),
            Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(name) if name == "target")
        )
    {
        let span = Span::new(start_span.start(), span_at(tokens, after_new + 1).end());
        let meta = ecma_syntax_cat::identifier::Identifier::new("new")?;
        let prop = ecma_syntax_cat::identifier::Identifier::new("target")?;
        Ok((
            Expression::new(
                ExpressionKind::MetaProperty {
                    meta,
                    property: prop,
                },
                span,
            ),
            after_new + 2,
        ))
    } else {
        let (callee, after_callee) = parse_new_or_member(tokens, after_new)?;
        let (arguments, after_args) = if is_kind(tokens, after_callee, &TokenKind::LParen) {
            parse_arguments(tokens, after_callee)?
        } else {
            (Vec::new(), after_callee)
        };
        let end_span = span_at(tokens, after_args.saturating_sub(1));
        let span = Span::new(start_span.start(), end_span.end());
        let new_expr = Expression::new(
            ExpressionKind::New {
                callee: Box::new(callee),
                arguments,
            },
            span,
        );
        parse_member_tail(tokens, after_args, new_expr)
    }
}

fn parse_member_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let (base, after_base) = parse_primary_expression(tokens, pos)?;
    parse_member_tail(tokens, after_base, base)
}

fn parse_member_tail(
    tokens: &[Token],
    pos: usize,
    base: Expression,
) -> Result<(Expression, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::Dot) {
        let (name, after_name) = expect_member_name(tokens, pos + 1)?;
        let span = Span::new(base.span().start(), span_at(tokens, after_name - 1).end());
        let member = Expression::new(
            ExpressionKind::Member {
                object: Box::new(base),
                property: MemberProperty::Identifier(name),
                optional: false,
            },
            span,
        );
        parse_member_tail(tokens, after_name, member)
    } else if is_kind(tokens, pos, &TokenKind::LBracket) {
        let (inner, after_inner) = parse_expression(tokens, pos + 1)?;
        let after_close = expect_kind(tokens, after_inner, &TokenKind::RBracket, "`]`")?;
        let span = Span::new(base.span().start(), span_at(tokens, after_close - 1).end());
        let member = Expression::new(
            ExpressionKind::Member {
                object: Box::new(base),
                property: MemberProperty::Computed(Box::new(inner)),
                optional: false,
            },
            span,
        );
        parse_member_tail(tokens, after_close, member)
    } else {
        Ok((base, pos))
    }
}

fn parse_call_tail(
    tokens: &[Token],
    pos: usize,
    base: Expression,
) -> Result<(Expression, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::LParen) {
        let (args, after_args) = parse_arguments(tokens, pos)?;
        let span = Span::new(base.span().start(), span_at(tokens, after_args - 1).end());
        let call = Expression::new(
            ExpressionKind::Call {
                callee: Box::new(base),
                arguments: args,
                optional: false,
            },
            span,
        );
        parse_call_tail(tokens, after_args, call)
    } else if is_kind(tokens, pos, &TokenKind::Dot) || is_kind(tokens, pos, &TokenKind::LBracket) {
        let (next, after_next) = parse_member_tail(tokens, pos, base)?;
        parse_call_tail(tokens, after_next, next)
    } else if is_kind(tokens, pos, &TokenKind::OptionalChain) {
        parse_optional_chain(tokens, pos, base)
    } else {
        Ok((base, pos))
    }
}

fn parse_optional_chain(
    tokens: &[Token],
    pos: usize,
    base: Expression,
) -> Result<(Expression, usize), Error> {
    let after_qmark = pos + 1;
    if is_kind(tokens, after_qmark, &TokenKind::LParen) {
        let (args, after_args) = parse_arguments(tokens, after_qmark)?;
        let span = Span::new(base.span().start(), span_at(tokens, after_args - 1).end());
        let inner = Expression::new(
            ExpressionKind::Call {
                callee: Box::new(base),
                arguments: args,
                optional: true,
            },
            span,
        );
        let chained = wrap_in_chain(inner);
        parse_call_tail(tokens, after_args, chained)
    } else if is_kind(tokens, after_qmark, &TokenKind::LBracket) {
        let (inner_expr, after_expr) = parse_expression(tokens, after_qmark + 1)?;
        let after_close = expect_kind(tokens, after_expr, &TokenKind::RBracket, "`]`")?;
        let span = Span::new(base.span().start(), span_at(tokens, after_close - 1).end());
        let member = Expression::new(
            ExpressionKind::Member {
                object: Box::new(base),
                property: MemberProperty::Computed(Box::new(inner_expr)),
                optional: true,
            },
            span,
        );
        let chained = wrap_in_chain(member);
        parse_call_tail(tokens, after_close, chained)
    } else {
        let (name, after_name) = expect_member_name(tokens, after_qmark)?;
        let span = Span::new(base.span().start(), span_at(tokens, after_name - 1).end());
        let member = Expression::new(
            ExpressionKind::Member {
                object: Box::new(base),
                property: MemberProperty::Identifier(name),
                optional: true,
            },
            span,
        );
        let chained = wrap_in_chain(member);
        parse_call_tail(tokens, after_name, chained)
    }
}

fn wrap_in_chain(expr: Expression) -> Expression {
    let span = expr.span();
    Expression::new(
        ExpressionKind::Chain {
            expression: Box::new(expr),
        },
        span,
    )
}

fn parse_arguments(tokens: &[Token], pos: usize) -> Result<(Vec<Expression>, usize), Error> {
    let after_open = expect_kind(tokens, pos, &TokenKind::LParen, "`(`")?;
    if is_kind(tokens, after_open, &TokenKind::RParen) {
        Ok((Vec::new(), after_open + 1))
    } else {
        collect_arguments(tokens, after_open, Vec::new())
    }
}

fn collect_arguments(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Expression>,
) -> Result<(Vec<Expression>, usize), Error> {
    let (arg, after_arg) = if is_kind(tokens, pos, &TokenKind::Spread) {
        let span = span_at(tokens, pos);
        let (inner, after_inner) = parse_assignment_expression(tokens, pos + 1)?;
        let full_span = Span::new(span.start(), inner.span().end());
        let spread = Expression::new(
            ExpressionKind::Spread {
                argument: Box::new(inner),
            },
            full_span,
        );
        (spread, after_inner)
    } else {
        parse_assignment_expression(tokens, pos)?
    };
    let extended: Vec<Expression> = acc.into_iter().chain(std::iter::once(arg)).collect();
    if is_kind(tokens, after_arg, &TokenKind::Comma) {
        let next_pos = after_arg + 1;
        if is_kind(tokens, next_pos, &TokenKind::RParen) {
            Ok((extended, next_pos + 1))
        } else {
            collect_arguments(tokens, next_pos, extended)
        }
    } else {
        let after_close = expect_kind(tokens, after_arg, &TokenKind::RParen, "`)` or `,`")?;
        Ok((extended, after_close))
    }
}

fn parse_primary_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "expression",
        }),
        Peek::Token(tok) => dispatch_primary(tokens, pos, tok, start_span),
    }
}

fn dispatch_primary(
    tokens: &[Token],
    pos: usize,
    tok: &Token,
    span: Span,
) -> Result<(Expression, usize), Error> {
    match tok.value() {
        TokenKind::KwThis => Ok((Expression::new(ExpressionKind::This, span), pos + 1)),
        TokenKind::KwSuper => Ok((Expression::new(ExpressionKind::Super, span), pos + 1)),
        TokenKind::KwTrue => Ok(literal_token(pos, span, Literal::boolean(true))),
        TokenKind::KwFalse => Ok(literal_token(pos, span, Literal::boolean(false))),
        TokenKind::KwNull => Ok(literal_token(pos, span, Literal::null())),
        TokenKind::Number(n) => Ok(literal_token(pos, span, Literal::number(*n))),
        TokenKind::BigInt(digits) => Ok(literal_token(pos, span, Literal::bigint(digits.clone()))),
        TokenKind::String(s) => Ok(literal_token(pos, span, Literal::string(s.clone()))),
        TokenKind::RegExp { pattern, flags } => Ok((
            Expression::new(
                ExpressionKind::Literal(Literal::regex(pattern.clone(), flags.clone())?),
                span,
            ),
            pos + 1,
        )),
        TokenKind::TemplateNoSubstitution(content) => Ok((
            Expression::new(
                ExpressionKind::Template {
                    quasis: vec![content.clone()],
                    expressions: Vec::new(),
                },
                span,
            ),
            pos + 1,
        )),
        TokenKind::TemplateHead(_) => parse_template_with_substitutions(tokens, pos),
        TokenKind::Identifier(name) => Ok((
            Expression::new(
                ExpressionKind::Identifier(ecma_syntax_cat::identifier::Identifier::new(
                    name.clone(),
                )?),
                span,
            ),
            pos + 1,
        )),
        TokenKind::PrivateIdentifier(_) => {
            let (id, after) = expect_private_identifier(tokens, pos)?;
            Ok((
                Expression::new(ExpressionKind::PrivateIdentifier(id), span),
                after,
            ))
        }
        TokenKind::LParen => parse_parenthesised(tokens, pos),
        TokenKind::LBracket => parse_array_literal(tokens, pos),
        TokenKind::LBrace => parse_object_literal(tokens, pos),
        TokenKind::KwFunction => parse_function_expression(tokens, pos, false),
        TokenKind::KwClass => parse_class_expression(tokens, pos),
        TokenKind::KwImport => parse_import_expression(tokens, pos),
        _other => Err(Error::UnexpectedToken {
            at: span,
            expected: "expression",
            found: format!("{}", tok.value()),
        }),
    }
}

fn literal_token(pos: usize, span: Span, literal: Literal) -> (Expression, usize) {
    (
        Expression::new(ExpressionKind::Literal(literal), span),
        pos + 1,
    )
}

fn parse_parenthesised(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let (inner, after_inner) = parse_expression(tokens, pos + 1)?;
    let after_close = expect_kind(tokens, after_inner, &TokenKind::RParen, "`)`")?;
    let span = Span::new(
        span_at(tokens, pos).start(),
        span_at(tokens, after_close - 1).end(),
    );
    Ok((
        Expression::new(
            ExpressionKind::Parenthesized {
                expression: Box::new(inner),
            },
            span,
        ),
        after_close,
    ))
}

fn parse_array_literal(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    collect_array_elements(tokens, pos + 1, Vec::new(), start_span)
}

fn collect_array_elements(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Option<Expression>>,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBracket) {
        let span = Span::new(start_span.start(), span_at(tokens, pos).end());
        Ok((
            Expression::new(ExpressionKind::Array { elements: acc }, span),
            pos + 1,
        ))
    } else if is_kind(tokens, pos, &TokenKind::Comma) {
        let extended = append_slot(acc, None);
        collect_array_elements(tokens, pos + 1, extended, start_span)
    } else {
        let (element, after_element) = if is_kind(tokens, pos, &TokenKind::Spread) {
            let s = span_at(tokens, pos);
            let (inner, after_inner) = parse_assignment_expression(tokens, pos + 1)?;
            let full = Span::new(s.start(), inner.span().end());
            (
                Expression::new(
                    ExpressionKind::Spread {
                        argument: Box::new(inner),
                    },
                    full,
                ),
                after_inner,
            )
        } else {
            parse_assignment_expression(tokens, pos)?
        };
        let extended = append_slot(acc, Some(element));
        if is_kind(tokens, after_element, &TokenKind::Comma) {
            collect_array_elements(tokens, after_element + 1, extended, start_span)
        } else {
            let after_close = expect_kind(tokens, after_element, &TokenKind::RBracket, "`]`")?;
            let span = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
            Ok((
                Expression::new(ExpressionKind::Array { elements: extended }, span),
                after_close,
            ))
        }
    }
}

fn append_slot(acc: Vec<Option<Expression>>, slot: Option<Expression>) -> Vec<Option<Expression>> {
    acc.into_iter().chain(std::iter::once(slot)).collect()
}

fn parse_object_literal(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    collect_object_members(tokens, pos + 1, Vec::new(), start_span)
}

fn collect_object_members(
    tokens: &[Token],
    pos: usize,
    acc: Vec<ObjectMember>,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace) {
        let span = Span::new(start_span.start(), span_at(tokens, pos).end());
        Ok((
            Expression::new(ExpressionKind::Object { properties: acc }, span),
            pos + 1,
        ))
    } else {
        let (member, after_member) = parse_object_member(tokens, pos)?;
        let extended: Vec<ObjectMember> = acc.into_iter().chain(std::iter::once(member)).collect();
        if is_kind(tokens, after_member, &TokenKind::Comma) {
            collect_object_members(tokens, after_member + 1, extended, start_span)
        } else {
            let after_close = expect_kind(tokens, after_member, &TokenKind::RBrace, "`}`")?;
            let span = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
            Ok((
                Expression::new(
                    ExpressionKind::Object {
                        properties: extended,
                    },
                    span,
                ),
                after_close,
            ))
        }
    }
}

fn parse_object_member(tokens: &[Token], pos: usize) -> Result<(ObjectMember, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::Spread) {
        let (inner, after_inner) = parse_assignment_expression(tokens, pos + 1)?;
        Ok((ObjectMember::Spread { argument: inner }, after_inner))
    } else if let Some(accessor_kind) = accessor_keyword_followed_by_key(tokens, pos) {
        parse_accessor_member(tokens, pos + 1, accessor_kind)
    } else {
        let (key, computed, after_key) = parse_property_key(tokens, pos)?;
        if is_kind(tokens, after_key, &TokenKind::Colon) {
            let (value, after_value) = parse_assignment_expression(tokens, after_key + 1)?;
            Ok((
                ObjectMember::Property {
                    key,
                    value,
                    kind: ObjectPropertyKind::Init,
                    computed,
                    shorthand: false,
                },
                after_value,
            ))
        } else if is_kind(tokens, after_key, &TokenKind::LParen) {
            parse_shorthand_method(tokens, after_key, key, computed)
        } else {
            let shorthand_span = span_at(tokens, pos);
            let value = property_key_to_shorthand_value(&key, shorthand_span)?;
            Ok((
                ObjectMember::Property {
                    key,
                    value,
                    kind: ObjectPropertyKind::Init,
                    computed,
                    shorthand: true,
                },
                after_key,
            ))
        }
    }
}

/// Detect `get`/`set` shorthand at `pos`: the token is the
/// identifier `get` or `set` AND the following token starts a
/// property key (`identifier`, `"string"`, `42`, or `[computed]`).
/// `{ get }` (shorthand init), `{ get: v }` (data property), and
/// `{ get() {} }` (shorthand method named "get") all return `None`
/// because the second token is `,`/`}`/`:`/`(` respectively.
fn accessor_keyword_followed_by_key(tokens: &[Token], pos: usize) -> Option<ObjectPropertyKind> {
    accessor_keyword(tokens, pos).filter(|_| is_property_key_start(tokens, pos + 1))
}

fn accessor_keyword(tokens: &[Token], pos: usize) -> Option<ObjectPropertyKind> {
    match peek(tokens, pos) {
        Peek::Eof => None,
        Peek::Token(t) => match t.value() {
            TokenKind::Identifier(name) => match name.as_str() {
                "get" => Some(ObjectPropertyKind::Get),
                "set" => Some(ObjectPropertyKind::Set),
                _other => None,
            },
            _other => None,
        },
    }
}

fn is_property_key_start(tokens: &[Token], pos: usize) -> bool {
    matches!(peek(tokens, pos), Peek::Token(t) if matches!(
        t.value(),
        TokenKind::Identifier(_)
            | TokenKind::String(_)
            | TokenKind::Number(_)
            | TokenKind::LBracket
    ))
}

fn parse_accessor_member(
    tokens: &[Token],
    pos: usize,
    kind: ObjectPropertyKind,
) -> Result<(ObjectMember, usize), Error> {
    let start_span = span_before(tokens, pos);
    let (key, computed, after_key) = parse_property_key(tokens, pos)?;
    let (value, after_body) = parse_method_function(tokens, after_key, start_span)?;
    Ok((
        ObjectMember::Property {
            key,
            value,
            kind,
            computed,
            shorthand: false,
        },
        after_body,
    ))
}

fn parse_shorthand_method(
    tokens: &[Token],
    pos_at_lparen: usize,
    key: PropertyKey,
    computed: bool,
) -> Result<(ObjectMember, usize), Error> {
    let start_span = span_before(tokens, pos_at_lparen);
    let (value, after_body) = parse_method_function(tokens, pos_at_lparen, start_span)?;
    Ok((
        ObjectMember::Property {
            key,
            value,
            kind: ObjectPropertyKind::Method,
            computed,
            shorthand: false,
        },
        after_body,
    ))
}

/// Parse the `(params) { body }` tail of a getter / setter /
/// shorthand-method member into a `FunctionExpression`.  `start_span`
/// is the position the resulting expression should span from
/// (typically the `get` / `set` / key token).  No `function` keyword
/// or method-name binding is consumed -- the caller supplies the key
/// separately, and the function value itself is anonymous (`id =
/// None`).  Getter / setter arity (0 / 1 respectively) is not yet
/// validated here; the engine can reject mismatches at invocation
/// time if needed.
fn parse_method_function(
    tokens: &[Token],
    pos_at_lparen: usize,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    let (params, after_params) =
        crate::declaration::parse_formal_parameters(tokens, pos_at_lparen)?;
    let (body, after_body) = crate::statement::parse_block_body(tokens, after_params)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_body - 1).end());
    let func = ecma_syntax_cat::function::Function::new(None, params, body, false, false);
    Ok((
        Expression::new(ExpressionKind::FunctionExpression(Box::new(func)), span),
        after_body,
    ))
}

fn property_key_to_shorthand_value(key: &PropertyKey, span: Span) -> Result<Expression, Error> {
    match key {
        PropertyKey::Identifier(id) => Ok(Expression::new(
            ExpressionKind::Identifier(id.clone()),
            span,
        )),
        PropertyKey::String(_)
        | PropertyKey::Number(_)
        | PropertyKey::Computed(_)
        | PropertyKey::Private(_) => Err(Error::UnexpectedToken {
            at: span,
            expected: "`:` (computed or literal keys require an explicit value)",
            found: "shorthand-key without identifier".to_owned(),
        }),
    }
}

fn parse_property_key(tokens: &[Token], pos: usize) -> Result<(PropertyKey, bool, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "property key",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::LBracket => {
                let (inner, after_inner) = parse_assignment_expression(tokens, pos + 1)?;
                let after_close = expect_kind(tokens, after_inner, &TokenKind::RBracket, "`]`")?;
                Ok((PropertyKey::Computed(Box::new(inner)), true, after_close))
            }
            TokenKind::String(s) => Ok((PropertyKey::String(s.clone()), false, pos + 1)),
            TokenKind::Number(n) => Ok((PropertyKey::Number(*n), false, pos + 1)),
            TokenKind::PrivateIdentifier(_) => {
                let (id, after) = expect_private_identifier(tokens, pos)?;
                Ok((PropertyKey::Private(id), false, after))
            }
            _other => {
                let (id, after) = expect_identifier_or_keyword(tokens, pos)?;
                Ok((PropertyKey::Identifier(id), false, after))
            }
        },
    }
}

fn parse_template_with_substitutions(
    tokens: &[Token],
    pos: usize,
) -> Result<(Expression, usize), Error> {
    collect_template(tokens, pos, Vec::new(), Vec::new(), span_at(tokens, pos))
}

fn collect_template(
    tokens: &[Token],
    pos: usize,
    quasis: Vec<String>,
    expressions: Vec<Expression>,
    start_span: Span,
) -> Result<(Expression, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "template continuation",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::TemplateHead(content) | TokenKind::TemplateMiddle(content) => {
                let next_quasis: Vec<String> = quasis
                    .into_iter()
                    .chain(std::iter::once(content.clone()))
                    .collect();
                let (expr, after_expr) = parse_expression(tokens, pos + 1)?;
                let next_exprs: Vec<Expression> = expressions
                    .into_iter()
                    .chain(std::iter::once(expr))
                    .collect();
                collect_template(tokens, after_expr, next_quasis, next_exprs, start_span)
            }
            TokenKind::TemplateTail(content) => {
                let final_quasis: Vec<String> = quasis
                    .into_iter()
                    .chain(std::iter::once(content.clone()))
                    .collect();
                let span = Span::new(start_span.start(), span_at(tokens, pos).end());
                Ok((
                    Expression::new(
                        ExpressionKind::Template {
                            quasis: final_quasis,
                            expressions,
                        },
                        span,
                    ),
                    pos + 1,
                ))
            }
            _other => Err(Error::UnexpectedToken {
                at: span_at(tokens, pos),
                expected: "template continuation",
                found: format!("{}", tok.value()),
            }),
        },
    }
}

fn parse_function_expression(
    tokens: &[Token],
    pos: usize,
    is_async: bool,
) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    let after_kw = expect_kind(tokens, pos, &TokenKind::KwFunction, "keyword `function`")?;
    let (is_generator, after_star) = if is_kind(tokens, after_kw, &TokenKind::Star) {
        (true, after_kw + 1)
    } else {
        (false, after_kw)
    };
    let (id, after_id) = if is_kind_identifier(tokens, after_star) {
        let (name, after) = expect_identifier(tokens, after_star)?;
        (Some(name), after)
    } else {
        (None, after_star)
    };
    let (params, after_params) = crate::declaration::parse_formal_parameters(tokens, after_id)?;
    let (body, after_body) = crate::statement::parse_block_body(tokens, after_params)?;
    let span = Span::new(start_span.start(), span_at(tokens, after_body - 1).end());
    let func = ecma_syntax_cat::function::Function::new(id, params, body, is_async, is_generator);
    Ok((
        Expression::new(ExpressionKind::FunctionExpression(Box::new(func)), span),
        after_body,
    ))
}

fn is_kind_identifier(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(_))
    )
}

fn parse_class_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let (class, end_pos) = crate::declaration::parse_class(tokens, pos)?;
    let span = span_at(tokens, pos);
    Ok((
        Expression::new(
            ExpressionKind::ClassExpression(Box::new(class)),
            Span::new(span.start(), span_at(tokens, end_pos - 1).end()),
        ),
        end_pos,
    ))
}

fn parse_import_expression(tokens: &[Token], pos: usize) -> Result<(Expression, usize), Error> {
    let start_span = span_at(tokens, pos);
    let after_kw = pos + 1;
    if is_kind(tokens, after_kw, &TokenKind::Dot) {
        let (name, after_name) = expect_identifier_or_keyword(tokens, after_kw + 1)?;
        let meta = ecma_syntax_cat::identifier::Identifier::new("import")?;
        let span = Span::new(start_span.start(), span_at(tokens, after_name - 1).end());
        Ok((
            Expression::new(
                ExpressionKind::MetaProperty {
                    meta,
                    property: name,
                },
                span,
            ),
            after_name,
        ))
    } else {
        let after_open = expect_kind(tokens, after_kw, &TokenKind::LParen, "`(`")?;
        let (source, after_source) = parse_assignment_expression(tokens, after_open)?;
        let (options, after_options) = if is_kind(tokens, after_source, &TokenKind::Comma) {
            let (opts, after_opts) = parse_assignment_expression(tokens, after_source + 1)?;
            (Some(Box::new(opts)), after_opts)
        } else {
            (None, after_source)
        };
        let after_close = expect_kind(tokens, after_options, &TokenKind::RParen, "`)`")?;
        let span = Span::new(start_span.start(), span_at(tokens, after_close - 1).end());
        Ok((
            Expression::new(
                ExpressionKind::ImportExpression {
                    source: Box::new(source),
                    options,
                },
                span,
            ),
            after_close,
        ))
    }
}
