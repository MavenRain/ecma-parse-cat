//! Function, class, and formal-parameter parsing.

use crate::error::Error;
use crate::expression::parse_assignment_expression;
use crate::statement::{parse_binding_element, parse_binding_pattern, parse_block_body};
use crate::stream::{Peek, expect_identifier, expect_kind, is_kind, peek, span_at};
use ecma_lex_cat::token::{Token, TokenKind};
use ecma_syntax_cat::class::{Class, ClassMember, MethodKind};
use ecma_syntax_cat::expression::PropertyKey;
use ecma_syntax_cat::function::Function;
use ecma_syntax_cat::pattern::{Pattern, PatternKind};
use ecma_syntax_cat::span::Span;

/// Parse a function declaration (`function name(params) { body }`).
///
/// # Errors
///
/// See [`Error`].
pub fn parse_function_declaration(
    tokens: &[Token],
    pos: usize,
) -> Result<(Function, usize), Error> {
    let after_kw = expect_kind(tokens, pos, &TokenKind::KwFunction, "keyword `function`")?;
    let (is_generator, after_star) = if is_kind(tokens, after_kw, &TokenKind::Star) {
        (true, after_kw + 1)
    } else {
        (false, after_kw)
    };
    let (id, after_id) = expect_identifier(tokens, after_star)?;
    let (params, after_params) = parse_formal_parameters(tokens, after_id)?;
    let (body, after_body) = parse_block_body(tokens, after_params)?;
    Ok((
        Function::new(Some(id), params, body, false, is_generator),
        after_body,
    ))
}

/// Parse a function declaration where the identifier may be omitted.  Used
/// by `export default function() {}`.
///
/// # Errors
///
/// See [`Error`].
pub fn parse_function_declaration_optional_name(
    tokens: &[Token],
    pos: usize,
) -> Result<(Function, usize), Error> {
    let after_kw = expect_kind(tokens, pos, &TokenKind::KwFunction, "keyword `function`")?;
    let (is_generator, after_star) = if is_kind(tokens, after_kw, &TokenKind::Star) {
        (true, after_kw + 1)
    } else {
        (false, after_kw)
    };
    let (id, after_id) = if matches!(
        peek(tokens, after_star),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(_))
    ) {
        let (name, after_name) = expect_identifier(tokens, after_star)?;
        (Some(name), after_name)
    } else {
        (None, after_star)
    };
    let (params, after_params) = parse_formal_parameters(tokens, after_id)?;
    let (body, after_body) = parse_block_body(tokens, after_params)?;
    Ok((
        Function::new(id, params, body, false, is_generator),
        after_body,
    ))
}

/// Parse a `(params)` list.  Returns the patterns and the position after
/// the closing paren.
///
/// # Errors
///
/// See [`Error`].
pub fn parse_formal_parameters(
    tokens: &[Token],
    pos: usize,
) -> Result<(Vec<Pattern>, usize), Error> {
    let after_open = expect_kind(tokens, pos, &TokenKind::LParen, "`(`")?;
    if is_kind(tokens, after_open, &TokenKind::RParen) {
        Ok((Vec::new(), after_open + 1))
    } else {
        collect_formal_params(tokens, after_open, Vec::new())
    }
}

fn collect_formal_params(
    tokens: &[Token],
    pos: usize,
    acc: Vec<Pattern>,
) -> Result<(Vec<Pattern>, usize), Error> {
    let (param, after_param) = parse_formal_parameter(tokens, pos)?;
    let extended: Vec<Pattern> = acc.into_iter().chain(std::iter::once(param)).collect();
    if is_kind(tokens, after_param, &TokenKind::Comma) {
        let next = after_param + 1;
        if is_kind(tokens, next, &TokenKind::RParen) {
            Ok((extended, next + 1))
        } else {
            collect_formal_params(tokens, next, extended)
        }
    } else {
        let after_close = expect_kind(tokens, after_param, &TokenKind::RParen, "`)` or `,`")?;
        Ok((extended, after_close))
    }
}

fn parse_formal_parameter(tokens: &[Token], pos: usize) -> Result<(Pattern, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::Spread) {
        let start_span = span_at(tokens, pos);
        let (inner, after_inner) = parse_binding_pattern(tokens, pos + 1)?;
        let span = Span::new(start_span.start(), inner.span().end());
        Ok((
            Pattern::new(
                PatternKind::Rest {
                    argument: Box::new(inner),
                },
                span,
            ),
            after_inner,
        ))
    } else {
        parse_binding_element(tokens, pos)
    }
}

/// Parse a class declaration or expression body (the parser does not
/// distinguish; the calling site wraps the result in either `ClassDeclaration`
/// or `ClassExpression`).
///
/// # Errors
///
/// See [`Error`].
pub fn parse_class(tokens: &[Token], pos: usize) -> Result<(Class, usize), Error> {
    let after_kw = expect_kind(tokens, pos, &TokenKind::KwClass, "keyword `class`")?;
    let (id, after_id) = if matches!(
        peek(tokens, after_kw),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(_))
    ) {
        let (name, after_name) = expect_identifier(tokens, after_kw)?;
        (Some(name), after_name)
    } else {
        (None, after_kw)
    };
    let (super_class, after_super) = if is_kind(tokens, after_id, &TokenKind::KwExtends) {
        let (expr, after_expr) = parse_assignment_expression(tokens, after_id + 1)?;
        (Some(expr), after_expr)
    } else {
        (None, after_id)
    };
    let after_lbrace = expect_kind(tokens, after_super, &TokenKind::LBrace, "`{`")?;
    let (members, after_rbrace) = collect_class_members(tokens, after_lbrace, Vec::new())?;
    Ok((Class::new(id, super_class, members), after_rbrace))
}

fn collect_class_members(
    tokens: &[Token],
    pos: usize,
    acc: Vec<ClassMember>,
) -> Result<(Vec<ClassMember>, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace) {
        Ok((acc, pos + 1))
    } else if is_kind(tokens, pos, &TokenKind::Semicolon) {
        collect_class_members(tokens, pos + 1, acc)
    } else {
        let (member, after_member) = parse_class_member(tokens, pos)?;
        let extended: Vec<ClassMember> = acc.into_iter().chain(std::iter::once(member)).collect();
        collect_class_members(tokens, after_member, extended)
    }
}

fn parse_class_member(tokens: &[Token], pos: usize) -> Result<(ClassMember, usize), Error> {
    let (is_static, after_static) = if is_static_keyword_at(tokens, pos) {
        (true, pos + 1)
    } else {
        (false, pos)
    };
    if is_kind(tokens, after_static, &TokenKind::LBrace) && is_static {
        let (body, after_body) = parse_block_body(tokens, after_static)?;
        Ok((ClassMember::StaticBlock { body }, after_body))
    } else {
        parse_class_method_or_field(tokens, after_static, is_static)
    }
}

fn is_static_keyword_at(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos),
        Peek::Token(t) if matches!(t.value(), TokenKind::KwStatic)
    ) && !matches!(
        peek(tokens, pos + 1),
        Peek::Token(t) if matches!(t.value(), TokenKind::Eq | TokenKind::Semicolon | TokenKind::LParen)
    )
}

fn parse_class_method_or_field(
    tokens: &[Token],
    pos: usize,
    is_static: bool,
) -> Result<(ClassMember, usize), Error> {
    let (method_kind, after_kind) = detect_method_kind(tokens, pos);
    let (key, computed, after_key) = parse_class_property_key(tokens, after_kind)?;
    if is_kind(tokens, after_key, &TokenKind::LParen) {
        let (params, after_params) = parse_formal_parameters(tokens, after_key)?;
        let (body, after_body) = parse_block_body(tokens, after_params)?;
        let function = Function::new(None, params, body, false, false);
        Ok((
            ClassMember::Method {
                key,
                value: function,
                kind: method_kind,
                is_static,
                computed,
            },
            after_body,
        ))
    } else if is_kind(tokens, after_key, &TokenKind::Eq) {
        let (value, after_value) = parse_assignment_expression(tokens, after_key + 1)?;
        let after_semi = if is_kind(tokens, after_value, &TokenKind::Semicolon) {
            after_value + 1
        } else {
            after_value
        };
        Ok((
            ClassMember::Property {
                key,
                value: Some(value),
                is_static,
                computed,
            },
            after_semi,
        ))
    } else {
        let after_semi = if is_kind(tokens, after_key, &TokenKind::Semicolon) {
            after_key + 1
        } else {
            after_key
        };
        Ok((
            ClassMember::Property {
                key,
                value: None,
                is_static,
                computed,
            },
            after_semi,
        ))
    }
}

fn detect_method_kind(tokens: &[Token], pos: usize) -> (MethodKind, usize) {
    match peek(tokens, pos) {
        Peek::Eof => (MethodKind::Method, pos),
        Peek::Token(tok) => match tok.value() {
            TokenKind::Identifier(name) if name == "constructor" => (MethodKind::Constructor, pos),
            TokenKind::Identifier(name) if name == "get" && peek_after_modifier(tokens, pos) => {
                (MethodKind::Get, pos + 1)
            }
            TokenKind::Identifier(name) if name == "set" && peek_after_modifier(tokens, pos) => {
                (MethodKind::Set, pos + 1)
            }
            _other => (MethodKind::Method, pos),
        },
    }
}

fn peek_after_modifier(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos + 1),
        Peek::Token(t) if matches!(
            t.value(),
            TokenKind::Identifier(_)
                | TokenKind::PrivateIdentifier(_)
                | TokenKind::String(_)
                | TokenKind::Number(_)
                | TokenKind::LBracket
        )
    )
}

fn parse_class_property_key(
    tokens: &[Token],
    pos: usize,
) -> Result<(PropertyKey, bool, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "class member name",
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
                let (id, after) = crate::stream::expect_private_identifier(tokens, pos)?;
                Ok((PropertyKey::Private(id), false, after))
            }
            _other => {
                let (id, after) = crate::stream::expect_identifier_or_keyword(tokens, pos)?;
                Ok((PropertyKey::Identifier(id), false, after))
            }
        },
    }
}
