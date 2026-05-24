//! Integration tests covering module-level parsing (import/export).

use ecma_lex_cat::lex;
use ecma_parse_cat::{Error, parse_module};
use ecma_syntax_cat::module::{ExportDeclaration, ImportSpecifier, ModuleItemKind};
use ecma_syntax_cat::program::ProgramKind;

fn first_module_item(source: &str) -> Result<ModuleItemKind, Error> {
    let tokens = lex(source)?;
    let program = parse_module(&tokens)?;
    match program.value() {
        ProgramKind::Module { body } => {
            body.first()
                .map(|m| m.value().clone())
                .ok_or(Error::UnexpectedEof {
                    expected: "first module item",
                })
        }
        ProgramKind::Script { .. } => Err(Error::UnexpectedEof {
            expected: "module-mode program",
        }),
    }
}

#[test]
fn parses_default_import() -> Result<(), Error> {
    let item = first_module_item("import foo from \"./foo.js\";")?;
    match item {
        ModuleItemKind::Import(decl) => match decl.specifiers().first() {
            Some(ImportSpecifier::Default { .. }) => Ok(()),
            _other => Err(Error::UnexpectedEof {
                expected: "default import specifier",
            }),
        },
        _other => Err(Error::UnexpectedEof {
            expected: "import declaration",
        }),
    }
}

#[test]
fn parses_named_imports() -> Result<(), Error> {
    let item = first_module_item("import { a, b as c } from \"./m.js\";")?;
    match item {
        ModuleItemKind::Import(decl) if decl.specifiers().len() == 2 => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "named import",
        }),
    }
}

#[test]
fn parses_namespace_import() -> Result<(), Error> {
    let item = first_module_item("import * as ns from \"./m.js\";")?;
    match item {
        ModuleItemKind::Import(decl) => match decl.specifiers().first() {
            Some(ImportSpecifier::Namespace { .. }) => Ok(()),
            _other => Err(Error::UnexpectedEof {
                expected: "namespace import",
            }),
        },
        _other => Err(Error::UnexpectedEof {
            expected: "import declaration",
        }),
    }
}

#[test]
fn parses_bare_import() -> Result<(), Error> {
    let item = first_module_item("import \"./side-effect.js\";")?;
    match item {
        ModuleItemKind::Import(decl) if decl.specifiers().is_empty() => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "bare import",
        }),
    }
}

#[test]
fn parses_named_export() -> Result<(), Error> {
    let item = first_module_item("export { foo, bar as baz };")?;
    match item {
        ModuleItemKind::Export(ExportDeclaration::Named { specifiers, .. })
            if specifiers.len() == 2 =>
        {
            Ok(())
        }
        _other => Err(Error::UnexpectedEof {
            expected: "named export",
        }),
    }
}

#[test]
fn parses_export_default_expression() -> Result<(), Error> {
    let item = first_module_item("export default 42;")?;
    match item {
        ModuleItemKind::Export(ExportDeclaration::Default { .. }) => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "export default",
        }),
    }
}

#[test]
fn parses_export_all() -> Result<(), Error> {
    let item = first_module_item("export * from \"./m.js\";")?;
    match item {
        ModuleItemKind::Export(ExportDeclaration::All { .. }) => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "export all",
        }),
    }
}

#[test]
fn parses_export_declaration() -> Result<(), Error> {
    let item = first_module_item("export let x = 1;")?;
    match item {
        ModuleItemKind::Export(ExportDeclaration::Declaration { .. }) => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "export declaration",
        }),
    }
}
