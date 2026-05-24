//! ECMAScript parser.
//!
//! Consumes a slice of [`ecma_lex_cat::token::Token`] and produces an
//! [`ecma_syntax_cat::program::Program`].
//!
//! Two entry points cover the two ECMA-262 goals:
//!
//! * [`parse_script`] for `Script`s (no `import`/`export` at top level).
//! * [`parse_module`] for `Module`s.
//!
//! Both expect a token slice that ends in `TokenKind::Eof` (the form
//! [`ecma_lex_cat`] produces).  The parser is recursive-descent for
//! statements and precedence-climbing for expressions; cover-grammar
//! refinement re-interprets parenthesised expressions as arrow-function
//! parameter lists when a trailing `=>` appears.
//!
//! # Examples
//!
//! ```
//! use ecma_lex_cat::lex;
//! use ecma_parse_cat::parse_script;
//!
//! # fn main() -> Result<(), ecma_parse_cat::Error> {
//! let tokens = lex("let x = 1 + 2;")?;
//! let program = parse_script(&tokens)?;
//! assert!(matches!(
//!     program.value(),
//!     ecma_syntax_cat::program::ProgramKind::Script { .. }
//! ));
//! # Ok(())
//! # }
//! ```

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
// Recursive-descent parsers naturally pair `after_lparen`/`after_rparen`,
// `after_lbrace`/`after_rbrace`, etc.  These trip pedantic::similar_names
// but renaming them would obscure the grammar correspondence.
#![allow(clippy::similar_names)]

mod declaration;
mod error;
mod expression;
mod module;
mod pattern;
mod precedence;
mod program;
mod statement;
mod stream;

pub use error::Error;
pub use program::{parse_module, parse_script};
