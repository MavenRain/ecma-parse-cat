//! Module-level item parsing: `import` and `export` declarations.

use crate::error::Error;
use crate::expression::parse_assignment_expression;
use crate::statement::parse_statement;
use crate::stream::{
    Peek, eat_semicolon, expect_identifier, expect_identifier_or_keyword, expect_kind, is_kind,
    peek, span_at,
};
use ecma_lex_cat::token::{Token, TokenKind};
use ecma_syntax_cat::module::{
    ExportDeclaration, ExportDefault, ExportSpecifier, ImportDeclaration, ImportSpecifier,
    ModuleItem, ModuleItemKind,
};
use ecma_syntax_cat::span::Span;
use ecma_syntax_cat::statement::StatementKind;

/// Parse a module-level item (statement, import, or export).
///
/// # Errors
///
/// See [`Error`].
pub fn parse_module_item(tokens: &[Token], pos: usize) -> Result<(ModuleItem, usize), Error> {
    let start_span = span_at(tokens, pos);
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "module item",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::KwImport if !is_import_call(tokens, pos) => {
                let (decl, after) = parse_import_declaration(tokens, pos)?;
                let span = Span::new(start_span.start(), span_at(tokens, after - 1).end());
                Ok((ModuleItem::new(ModuleItemKind::Import(decl), span), after))
            }
            TokenKind::KwExport => {
                let (decl, after) = parse_export_declaration(tokens, pos)?;
                let span = Span::new(start_span.start(), span_at(tokens, after - 1).end());
                Ok((ModuleItem::new(ModuleItemKind::Export(decl), span), after))
            }
            _other => {
                let (stmt, after) = parse_statement(tokens, pos)?;
                let span = Span::new(start_span.start(), span_at(tokens, after - 1).end());
                Ok((
                    ModuleItem::new(ModuleItemKind::Statement(stmt), span),
                    after,
                ))
            }
        },
    }
}

fn is_import_call(tokens: &[Token], pos: usize) -> bool {
    is_kind(tokens, pos + 1, &TokenKind::LParen) || is_kind(tokens, pos + 1, &TokenKind::Dot)
}

fn parse_import_declaration(
    tokens: &[Token],
    pos: usize,
) -> Result<(ImportDeclaration, usize), Error> {
    let after_kw = pos + 1;
    bare_import_source(tokens, after_kw).map_or_else(
        || parse_import_with_specifiers(tokens, after_kw),
        |source| {
            let after_source = after_kw + 1;
            let after_semi = eat_semicolon(tokens, after_source)?;
            Ok((ImportDeclaration::new(Vec::new(), source), after_semi))
        },
    )
}

fn bare_import_source(tokens: &[Token], pos: usize) -> Option<String> {
    match peek(tokens, pos) {
        Peek::Eof => None,
        Peek::Token(tok) => match tok.value() {
            TokenKind::String(s) => Some(s.clone()),
            _other => None,
        },
    }
}

fn parse_import_with_specifiers(
    tokens: &[Token],
    after_kw: usize,
) -> Result<(ImportDeclaration, usize), Error> {
    let (specifiers, after_specs) = parse_import_specifiers(tokens, after_kw)?;
    let after_from = expect_identifier_matching(tokens, after_specs, "from")?;
    let (source, after_source) = expect_string(tokens, after_from)?;
    let after_semi = eat_semicolon(tokens, after_source)?;
    Ok((ImportDeclaration::new(specifiers, source), after_semi))
}

fn parse_import_specifiers(
    tokens: &[Token],
    pos: usize,
) -> Result<(Vec<ImportSpecifier>, usize), Error> {
    let (head, after_head) = match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "import specifier",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::Star => parse_namespace_import(tokens, pos),
            TokenKind::LBrace => {
                let (named, after_named) = parse_named_imports(tokens, pos)?;
                Ok((named, after_named))
            }
            _other => parse_default_then_optional(tokens, pos),
        },
    }?;
    Ok((head, after_head))
}

fn parse_namespace_import(
    tokens: &[Token],
    pos: usize,
) -> Result<(Vec<ImportSpecifier>, usize), Error> {
    let after_star = pos + 1;
    let after_as = expect_identifier_matching(tokens, after_star, "as")?;
    let (local, after_local) = expect_identifier(tokens, after_as)?;
    Ok((vec![ImportSpecifier::Namespace { local }], after_local))
}

fn parse_named_imports(
    tokens: &[Token],
    pos: usize,
) -> Result<(Vec<ImportSpecifier>, usize), Error> {
    let after_open = pos + 1;
    collect_named_imports(tokens, after_open, Vec::new())
}

fn collect_named_imports(
    tokens: &[Token],
    pos: usize,
    acc: Vec<ImportSpecifier>,
) -> Result<(Vec<ImportSpecifier>, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace) {
        Ok((acc, pos + 1))
    } else {
        let (imported, after_imported) = expect_identifier_or_keyword(tokens, pos)?;
        let (local, after_local) = if is_as_keyword(tokens, after_imported) {
            let (name, after_name) = expect_identifier(tokens, after_imported + 1)?;
            (name, after_name)
        } else {
            (imported.clone(), after_imported)
        };
        let extended: Vec<ImportSpecifier> = acc
            .into_iter()
            .chain(std::iter::once(ImportSpecifier::Named { imported, local }))
            .collect();
        if is_kind(tokens, after_local, &TokenKind::Comma) {
            collect_named_imports(tokens, after_local + 1, extended)
        } else {
            let after_close = expect_kind(tokens, after_local, &TokenKind::RBrace, "`}`")?;
            Ok((extended, after_close))
        }
    }
}

fn parse_default_then_optional(
    tokens: &[Token],
    pos: usize,
) -> Result<(Vec<ImportSpecifier>, usize), Error> {
    let (local, after_local) = expect_identifier(tokens, pos)?;
    let head = ImportSpecifier::Default { local };
    if is_kind(tokens, after_local, &TokenKind::Comma) {
        let (rest, after_rest) = match peek(tokens, after_local + 1) {
            Peek::Eof => Err(Error::UnexpectedEof {
                expected: "import specifier",
            }),
            Peek::Token(tok) => match tok.value() {
                TokenKind::Star => parse_namespace_import(tokens, after_local + 1),
                TokenKind::LBrace => parse_named_imports(tokens, after_local + 1),
                _other => Err(Error::UnexpectedToken {
                    at: tok.span(),
                    expected: "`*` or `{`",
                    found: format!("{}", tok.value()),
                }),
            },
        }?;
        let combined: Vec<ImportSpecifier> = std::iter::once(head).chain(rest).collect();
        Ok((combined, after_rest))
    } else {
        Ok((vec![head], after_local))
    }
}

fn parse_export_declaration(
    tokens: &[Token],
    pos: usize,
) -> Result<(ExportDeclaration, usize), Error> {
    let after_kw = pos + 1;
    match peek(tokens, after_kw) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "export form",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::Star => parse_export_all(tokens, after_kw),
            TokenKind::LBrace => parse_export_named(tokens, after_kw),
            TokenKind::KwDefault => parse_export_default(tokens, after_kw),
            _other => parse_export_declaration_inline(tokens, after_kw),
        },
    }
}

fn parse_export_all(tokens: &[Token], pos: usize) -> Result<(ExportDeclaration, usize), Error> {
    let after_star = pos + 1;
    let (exported, after_as) = if is_as_keyword(tokens, after_star) {
        let (id, after_id) = expect_identifier(tokens, after_star + 1)?;
        (Some(id), after_id)
    } else {
        (None, after_star)
    };
    let after_from = expect_identifier_matching(tokens, after_as, "from")?;
    let (source, after_source) = expect_string(tokens, after_from)?;
    let after_semi = eat_semicolon(tokens, after_source)?;
    Ok((ExportDeclaration::All { exported, source }, after_semi))
}

fn parse_export_named(tokens: &[Token], pos: usize) -> Result<(ExportDeclaration, usize), Error> {
    let after_open = pos + 1;
    let (specifiers, after_close) = collect_export_specifiers(tokens, after_open, Vec::new())?;
    let (source, after_source) = if is_from_keyword(tokens, after_close) {
        let (s, after_s) = expect_string(tokens, after_close + 1)?;
        (Some(s), after_s)
    } else {
        (None, after_close)
    };
    let after_semi = eat_semicolon(tokens, after_source)?;
    Ok((ExportDeclaration::Named { specifiers, source }, after_semi))
}

fn collect_export_specifiers(
    tokens: &[Token],
    pos: usize,
    acc: Vec<ExportSpecifier>,
) -> Result<(Vec<ExportSpecifier>, usize), Error> {
    if is_kind(tokens, pos, &TokenKind::RBrace) {
        Ok((acc, pos + 1))
    } else {
        let (local, after_local) = expect_identifier_or_keyword(tokens, pos)?;
        let (exported, after_exported) = if is_as_keyword(tokens, after_local) {
            let (name, after_name) = expect_identifier_or_keyword(tokens, after_local + 1)?;
            (name, after_name)
        } else {
            (local.clone(), after_local)
        };
        let extended: Vec<ExportSpecifier> = acc
            .into_iter()
            .chain(std::iter::once(ExportSpecifier::new(local, exported)))
            .collect();
        if is_kind(tokens, after_exported, &TokenKind::Comma) {
            collect_export_specifiers(tokens, after_exported + 1, extended)
        } else {
            let after_close = expect_kind(tokens, after_exported, &TokenKind::RBrace, "`}`")?;
            Ok((extended, after_close))
        }
    }
}

fn parse_export_default(tokens: &[Token], pos: usize) -> Result<(ExportDeclaration, usize), Error> {
    let after_kw = pos + 1;
    if is_kind(tokens, after_kw, &TokenKind::KwFunction) {
        let (func, after_func) =
            crate::declaration::parse_function_declaration_optional_name(tokens, after_kw)?;
        Ok((
            ExportDeclaration::Default {
                declaration: ExportDefault::Function(func),
            },
            after_func,
        ))
    } else if is_kind(tokens, after_kw, &TokenKind::KwClass) {
        let (class, after_class) = crate::declaration::parse_class(tokens, after_kw)?;
        Ok((
            ExportDeclaration::Default {
                declaration: ExportDefault::Class(class),
            },
            after_class,
        ))
    } else {
        let (expr, after_expr) = parse_assignment_expression(tokens, after_kw)?;
        let after_semi = eat_semicolon(tokens, after_expr)?;
        Ok((
            ExportDeclaration::Default {
                declaration: ExportDefault::Expression(expr),
            },
            after_semi,
        ))
    }
}

fn parse_export_declaration_inline(
    tokens: &[Token],
    pos: usize,
) -> Result<(ExportDeclaration, usize), Error> {
    let (stmt, after_stmt) = parse_statement(tokens, pos)?;
    let is_decl = matches!(
        stmt.value(),
        StatementKind::VariableDeclaration(_)
            | StatementKind::FunctionDeclaration(_)
            | StatementKind::ClassDeclaration(_)
    );
    if is_decl {
        Ok((
            ExportDeclaration::Declaration { declaration: stmt },
            after_stmt,
        ))
    } else {
        Err(Error::UnexpectedToken {
            at: stmt.span(),
            expected: "declaration after `export`",
            found: "non-declaration statement".to_owned(),
        })
    }
}

fn expect_identifier_matching(
    tokens: &[Token],
    pos: usize,
    target: &'static str,
) -> Result<usize, Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof { expected: target }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::Identifier(name) if name == target => Ok(pos + 1),
            _other => Err(Error::UnexpectedToken {
                at: tok.span(),
                expected: target,
                found: format!("{}", tok.value()),
            }),
        },
    }
}

fn is_as_keyword(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(name) if name == "as")
    )
}

fn is_from_keyword(tokens: &[Token], pos: usize) -> bool {
    matches!(
        peek(tokens, pos),
        Peek::Token(t) if matches!(t.value(), TokenKind::Identifier(name) if name == "from")
    )
}

fn expect_string(tokens: &[Token], pos: usize) -> Result<(String, usize), Error> {
    match peek(tokens, pos) {
        Peek::Eof => Err(Error::UnexpectedEof {
            expected: "string literal",
        }),
        Peek::Token(tok) => match tok.value() {
            TokenKind::String(s) => Ok((s.clone(), pos + 1)),
            _other => Err(Error::UnexpectedToken {
                at: tok.span(),
                expected: "string literal",
                found: format!("{}", tok.value()),
            }),
        },
    }
}
