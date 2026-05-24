//! Integration tests covering statement parsing.

use ecma_lex_cat::lex;
use ecma_parse_cat::{Error, parse_script};
use ecma_syntax_cat::declaration::VariableKind;
use ecma_syntax_cat::program::ProgramKind;
use ecma_syntax_cat::statement::StatementKind;

fn first_statement_kind(source: &str) -> Result<StatementKind, Error> {
    let tokens = lex(source)?;
    let program = parse_script(&tokens)?;
    match program.value() {
        ProgramKind::Script { body } => {
            body.first()
                .map(|s| s.value().clone())
                .ok_or(Error::UnexpectedEof {
                    expected: "first statement",
                })
        }
        ProgramKind::Module { .. } => Err(Error::UnexpectedEof {
            expected: "script-mode program",
        }),
    }
}

#[test]
fn parses_empty_statement() -> Result<(), Error> {
    let kind = first_statement_kind(";")?;
    matches!(kind, StatementKind::Empty)
        .then_some(())
        .ok_or(Error::UnexpectedEof {
            expected: "empty statement",
        })
}

#[test]
fn parses_let_declaration() -> Result<(), Error> {
    let kind = first_statement_kind("let x = 1;")?;
    match kind {
        StatementKind::VariableDeclaration(decl) if decl.kind() == VariableKind::Let => Ok(()),
        _other => Err(Error::UnexpectedEof { expected: "let" }),
    }
}

#[test]
fn parses_const_destructuring() -> Result<(), Error> {
    let kind = first_statement_kind("const { a, b } = obj;")?;
    match kind {
        StatementKind::VariableDeclaration(decl)
            if decl.kind() == VariableKind::Const && decl.declarators().len() == 1 =>
        {
            Ok(())
        }
        _other => Err(Error::UnexpectedEof {
            expected: "const destructure",
        }),
    }
}

#[test]
fn parses_if_else() -> Result<(), Error> {
    let kind = first_statement_kind("if (x) { y; } else { z; }")?;
    match kind {
        StatementKind::If {
            alternate: Some(_), ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "if-else",
        }),
    }
}

#[test]
fn parses_for_classic() -> Result<(), Error> {
    let kind = first_statement_kind("for (let i = 0; i < 10; i++) {}")?;
    match kind {
        StatementKind::For {
            init: Some(_),
            test: Some(_),
            update: Some(_),
            ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "classic for loop",
        }),
    }
}

#[test]
fn parses_for_of() -> Result<(), Error> {
    let kind = first_statement_kind("for (const x of arr) {}")?;
    match kind {
        StatementKind::ForOf {
            is_await: false, ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof { expected: "for-of" }),
    }
}

#[test]
fn parses_for_in() -> Result<(), Error> {
    let kind = first_statement_kind("for (const k in obj) {}")?;
    match kind {
        StatementKind::ForIn { .. } => Ok(()),
        _other => Err(Error::UnexpectedEof { expected: "for-in" }),
    }
}

#[test]
fn parses_while() -> Result<(), Error> {
    let kind = first_statement_kind("while (x) { x = next(); }")?;
    matches!(kind, StatementKind::While { .. })
        .then_some(())
        .ok_or(Error::UnexpectedEof { expected: "while" })
}

#[test]
fn parses_try_catch_finally() -> Result<(), Error> {
    let kind = first_statement_kind("try { a; } catch (e) { b; } finally { c; }")?;
    match kind {
        StatementKind::Try {
            handler: Some(_),
            finalizer: Some(_),
            ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "try with catch and finally",
        }),
    }
}

#[test]
fn parses_throw() -> Result<(), Error> {
    let kind = first_statement_kind("throw new Error();")?;
    matches!(kind, StatementKind::Throw { .. })
        .then_some(())
        .ok_or(Error::UnexpectedEof { expected: "throw" })
}

#[test]
fn parses_return_with_argument() -> Result<(), Error> {
    let tokens = lex("function f() { return 1; }")?;
    let program = parse_script(&tokens)?;
    match program.value() {
        ProgramKind::Script { body } => {
            match body.first().map(ecma_syntax_cat::span::Spanned::value) {
                Some(StatementKind::FunctionDeclaration(func)) => match func.body().first() {
                    Some(stmt) => match stmt.value() {
                        StatementKind::Return { argument: Some(_) } => Ok(()),
                        _other => Err(Error::UnexpectedEof {
                            expected: "return with argument",
                        }),
                    },
                    None => Err(Error::UnexpectedEof {
                        expected: "function body return statement",
                    }),
                },
                _other => Err(Error::UnexpectedEof {
                    expected: "function declaration",
                }),
            }
        }
        ProgramKind::Module { .. } => Err(Error::UnexpectedEof { expected: "script" }),
    }
}

#[test]
fn parses_switch() -> Result<(), Error> {
    let kind = first_statement_kind("switch (x) { case 1: a; break; default: b; }")?;
    match kind {
        StatementKind::Switch { cases, .. } if cases.len() == 2 => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "switch with two cases",
        }),
    }
}

#[test]
fn parses_class_declaration() -> Result<(), Error> {
    let kind =
        first_statement_kind("class Foo extends Bar { constructor() {} method() { return 1; } }")?;
    match kind {
        StatementKind::ClassDeclaration(class) if class.body().len() == 2 => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "class with two members",
        }),
    }
}

#[test]
fn parses_labeled_break() -> Result<(), Error> {
    let kind = first_statement_kind("outer: while (true) { break outer; }")?;
    matches!(kind, StatementKind::Labeled { .. })
        .then_some(())
        .ok_or(Error::UnexpectedEof {
            expected: "labeled statement",
        })
}
