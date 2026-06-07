//! Integration tests covering expression parsing.

use ecma_lex_cat::lex;
use ecma_parse_cat::{Error, parse_script};
use ecma_syntax_cat::expression::{
    ExpressionKind, MemberProperty, ObjectMember, ObjectPropertyKind, PropertyKey,
};
use ecma_syntax_cat::operator::{BinaryOperator, LogicalOperator, UnaryOperator};
use ecma_syntax_cat::program::ProgramKind;
use ecma_syntax_cat::statement::StatementKind;

fn first_expression(source: &str) -> Result<ExpressionKind, Error> {
    let tokens = lex(source)?;
    let program = parse_script(&tokens)?;
    extract_first_expression(&program).ok_or(Error::UnexpectedEof {
        expected: "first expression statement",
    })
}

fn extract_first_expression(program: &ecma_syntax_cat::program::Program) -> Option<ExpressionKind> {
    match program.value() {
        ProgramKind::Script { body } => body.first().and_then(|stmt| match stmt.value() {
            StatementKind::Expression { expression } => Some(expression.value().clone()),
            _other => None,
        }),
        ProgramKind::Module { .. } => None,
    }
}

#[test]
fn parses_number_literal() -> Result<(), Error> {
    let kind = first_expression("42;")?;
    matches!(kind, ExpressionKind::Literal(_))
        .then_some(())
        .ok_or(Error::UnexpectedEof {
            expected: "literal expression",
        })
}

#[test]
fn parses_binary_with_precedence() -> Result<(), Error> {
    let kind = first_expression("1 + 2 * 3;")?;
    match kind {
        ExpressionKind::Binary {
            operator: BinaryOperator::Add,
            ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "binary add at top",
        }),
    }
}

#[test]
fn parses_right_associative_exponentiation() -> Result<(), Error> {
    let kind = first_expression("2 ** 3 ** 4;")?;
    let (op, _, right) = match kind {
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => (operator, left, right),
        _other => Err(Error::UnexpectedEof { expected: "binary" })?,
    };
    match (op, right.value()) {
        (
            BinaryOperator::Exponentiation,
            ExpressionKind::Binary {
                operator: BinaryOperator::Exponentiation,
                ..
            },
        ) => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "right-associative exponentiation",
        }),
    }
}

#[test]
fn parses_short_circuit_logical() -> Result<(), Error> {
    let kind = first_expression("a && b || c;")?;
    match kind {
        ExpressionKind::Logical {
            operator: LogicalOperator::Or,
            ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "outer logical-or",
        }),
    }
}

#[test]
fn parses_member_chain() -> Result<(), Error> {
    let kind = first_expression("foo.bar.baz;")?;
    match kind {
        ExpressionKind::Member {
            property: MemberProperty::Identifier(_),
            ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "member access chain",
        }),
    }
}

#[test]
fn parses_optional_chain() -> Result<(), Error> {
    let kind = first_expression("foo?.bar;")?;
    match kind {
        ExpressionKind::Chain { .. } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "optional-chain wrapper",
        }),
    }
}

#[test]
fn parses_call_with_arguments() -> Result<(), Error> {
    let kind = first_expression("f(1, 2, ...rest);")?;
    match kind {
        ExpressionKind::Call { arguments, .. } if arguments.len() == 3 => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "call with three args",
        }),
    }
}

#[test]
fn parses_unary_typeof() -> Result<(), Error> {
    let kind = first_expression("typeof x;")?;
    match kind {
        ExpressionKind::Unary {
            operator: UnaryOperator::TypeOf,
            ..
        } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "typeof unary",
        }),
    }
}

#[test]
fn parses_conditional_expression() -> Result<(), Error> {
    let kind = first_expression("a ? b : c;")?;
    match kind {
        ExpressionKind::Conditional { .. } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "conditional expression",
        }),
    }
}

#[test]
fn parses_assignment_chain() -> Result<(), Error> {
    let kind = first_expression("x = y = 1;")?;
    let inner = match kind {
        ExpressionKind::Assignment { right, .. } => Ok(right),
        _other => Err(Error::UnexpectedEof {
            expected: "outer assignment",
        }),
    }?;
    match inner.value() {
        ExpressionKind::Assignment { .. } => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "right-associative inner assignment",
        }),
    }
}

#[test]
fn parses_arrow_function_single_arg() -> Result<(), Error> {
    let kind = first_expression("x => x + 1;")?;
    match kind {
        ExpressionKind::ArrowFunction(_) => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "arrow function",
        }),
    }
}

#[test]
fn parses_arrow_function_paren_args() -> Result<(), Error> {
    let kind = first_expression("(a, b) => a + b;")?;
    match kind {
        ExpressionKind::ArrowFunction(_) => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "paren arrow function",
        }),
    }
}

#[test]
fn parses_arrow_function_empty_args() -> Result<(), Error> {
    let kind = first_expression("() => 42;")?;
    match kind {
        ExpressionKind::ArrowFunction(_) => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "empty arrow function",
        }),
    }
}

#[test]
fn parses_array_literal_with_holes() -> Result<(), Error> {
    let kind = first_expression("[1, , 3];")?;
    match kind {
        ExpressionKind::Array { elements } if elements.len() == 3 && elements[1].is_none() => {
            Ok(())
        }
        _other => Err(Error::UnexpectedEof {
            expected: "array with hole",
        }),
    }
}

#[test]
fn parses_object_literal_shorthand() -> Result<(), Error> {
    let kind = first_expression("({ a, b: 1, ...rest });")?;
    match kind {
        ExpressionKind::Parenthesized { expression } => match expression.value() {
            ExpressionKind::Object { properties } if properties.len() == 3 => Ok(()),
            _other => Err(Error::UnexpectedEof {
                expected: "object with three members",
            }),
        },
        _other => Err(Error::UnexpectedEof {
            expected: "parenthesised object",
        }),
    }
}

#[test]
fn parses_template_literal_no_subst() -> Result<(), Error> {
    let kind = first_expression("`hello`;")?;
    match kind {
        ExpressionKind::Template {
            quasis,
            expressions,
        } if expressions.is_empty() && quasis.len() == 1 => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "template literal with no substitutions",
        }),
    }
}

#[test]
fn parses_template_literal_with_subst() -> Result<(), Error> {
    let kind = first_expression("`hello ${name}!`;")?;
    match kind {
        ExpressionKind::Template {
            quasis,
            expressions,
        } if expressions.len() == 1 && quasis.len() == 2 => Ok(()),
        _other => Err(Error::UnexpectedEof {
            expected: "template literal with one substitution",
        }),
    }
}

fn first_object_members(source: &str) -> Result<Vec<ObjectMember>, Error> {
    match first_expression(source)? {
        ExpressionKind::Parenthesized { expression } => match expression.value() {
            ExpressionKind::Object { properties } => Ok(properties.clone()),
            _other => Err(Error::UnexpectedEof {
                expected: "parenthesised object literal",
            }),
        },
        ExpressionKind::Object { properties } => Ok(properties),
        _other => Err(Error::UnexpectedEof {
            expected: "object literal",
        }),
    }
}

fn member_kind_and_key_name(member: &ObjectMember) -> Option<(ObjectPropertyKind, String)> {
    match member {
        ObjectMember::Property { key, kind, .. } => {
            let name = match key {
                PropertyKey::Identifier(id) => Some(id.as_str().to_owned()),
                PropertyKey::String(s) => Some(s.clone()),
                PropertyKey::Number(_) | PropertyKey::Computed(_) | PropertyKey::Private(_) => None,
            };
            name.map(|n| (*kind, n))
        }
        ObjectMember::Spread { .. } => None,
    }
}

#[test]
fn parses_getter_member() -> Result<(), Error> {
    let members = first_object_members("({ get x() { return 1; } });")?;
    let (kind, name) = members
        .first()
        .and_then(member_kind_and_key_name)
        .ok_or(Error::UnexpectedEof { expected: "getter" })?;
    (matches!(kind, ObjectPropertyKind::Get) && name == "x")
        .then_some(())
        .ok_or(Error::UnexpectedEof { expected: "get x" })
}

#[test]
fn parses_setter_member() -> Result<(), Error> {
    let members = first_object_members("({ set x(v) { } });")?;
    let (kind, name) = members
        .first()
        .and_then(member_kind_and_key_name)
        .ok_or(Error::UnexpectedEof { expected: "setter" })?;
    (matches!(kind, ObjectPropertyKind::Set) && name == "x")
        .then_some(())
        .ok_or(Error::UnexpectedEof { expected: "set x" })
}

#[test]
fn parses_method_member() -> Result<(), Error> {
    let members = first_object_members("({ greet() { return 1; } });")?;
    let (kind, name) = members
        .first()
        .and_then(member_kind_and_key_name)
        .ok_or(Error::UnexpectedEof { expected: "method" })?;
    (matches!(kind, ObjectPropertyKind::Method) && name == "greet")
        .then_some(())
        .ok_or(Error::UnexpectedEof {
            expected: "greet()",
        })
}

#[test]
fn parses_get_as_shorthand_method_when_next_is_lparen() -> Result<(), Error> {
    // `{ get() {} }` should NOT be a getter (no following key);
    // it is a shorthand method named "get".
    let members = first_object_members("({ get() { return 1; } });")?;
    let (kind, name) =
        members
            .first()
            .and_then(member_kind_and_key_name)
            .ok_or(Error::UnexpectedEof {
                expected: "method get",
            })?;
    (matches!(kind, ObjectPropertyKind::Method) && name == "get")
        .then_some(())
        .ok_or(Error::UnexpectedEof {
            expected: "shorthand method named get",
        })
}

#[test]
fn parses_get_as_init_when_followed_by_colon() -> Result<(), Error> {
    // `{ get: 1 }` -- "get" is a property name, not an accessor.
    let members = first_object_members("({ get: 1 });")?;
    let (kind, name) = members
        .first()
        .and_then(member_kind_and_key_name)
        .ok_or(Error::UnexpectedEof { expected: "init" })?;
    (matches!(kind, ObjectPropertyKind::Init) && name == "get")
        .then_some(())
        .ok_or(Error::UnexpectedEof {
            expected: "data property get",
        })
}

#[test]
fn parses_get_as_shorthand_init_when_alone() -> Result<(), Error> {
    // `{ get }` -- shorthand init referencing variable "get".
    let members = first_object_members("({ get });")?;
    let (kind, name) =
        members
            .first()
            .and_then(member_kind_and_key_name)
            .ok_or(Error::UnexpectedEof {
                expected: "shorthand init",
            })?;
    (matches!(kind, ObjectPropertyKind::Init) && name == "get")
        .then_some(())
        .ok_or(Error::UnexpectedEof {
            expected: "shorthand init get",
        })
}

#[test]
fn parses_combined_get_set_accessor_pair() -> Result<(), Error> {
    let members = first_object_members("({ get x() { return 1; }, set x(v) { } });")?;
    (members.len() == 2
        && member_kind_and_key_name(&members[0])
            .is_some_and(|(k, n)| matches!(k, ObjectPropertyKind::Get) && n == "x")
        && member_kind_and_key_name(&members[1])
            .is_some_and(|(k, n)| matches!(k, ObjectPropertyKind::Set) && n == "x"))
    .then_some(())
    .ok_or(Error::UnexpectedEof {
        expected: "get x followed by set x",
    })
}
