//! Pattern parsing and expression-to-pattern reinterpretation.
//!
//! Arrow-function parameter lists are parsed first as expressions (because
//! the parser cannot tell that `(a, b)` is an arrow's parameter list until
//! it sees the trailing `=>`).  This module's
//! [`reinterpret_expression_as_pattern`] turns the parsed expression back
//! into a pattern after the cover-grammar resolves.

use crate::error::Error;
use ecma_syntax_cat::expression::{Expression, ExpressionKind, ObjectMember, ObjectPropertyKind};
use ecma_syntax_cat::pattern::{ObjectPatternMember, Pattern, PatternKind};
use ecma_syntax_cat::span::Span;

/// Convert an [`Expression`] into the [`Pattern`] it would represent if
/// used as an arrow-function parameter or destructuring target.  Returns
/// [`Error::InvalidArrowParameter`] when the expression is not a valid
/// pattern shape.
pub fn reinterpret_expression_as_pattern(expr: Expression) -> Result<Pattern, Error> {
    let (kind, span) = expr.into_parts();
    convert_kind(kind, span)
}

/// Convert an [`Expression`] into the parameter list it would represent if
/// used as an arrow-function parameter list.  Handles the cover-grammar
/// case where `(a, b)` parses first as a parenthesised sequence and only
/// becomes recognisable as a parameter list once the trailing `=>` arrives.
///
/// # Errors
///
/// [`Error::InvalidArrowParameter`] when any element cannot become a pattern.
pub fn reinterpret_expression_as_arrow_params(expr: Expression) -> Result<Vec<Pattern>, Error> {
    let unwrapped = unwrap_parenthesized(expr);
    let (kind, span) = unwrapped.into_parts();
    if let ExpressionKind::Sequence { expressions } = kind {
        expressions
            .into_iter()
            .map(reinterpret_expression_as_pattern)
            .collect()
    } else {
        reinterpret_expression_as_pattern(Expression::new(kind, span)).map(|p| vec![p])
    }
}

fn unwrap_parenthesized(expr: Expression) -> Expression {
    let (kind, span) = expr.into_parts();
    if let ExpressionKind::Parenthesized { expression } = kind {
        unwrap_parenthesized(*expression)
    } else {
        Expression::new(kind, span)
    }
}

fn convert_kind(kind: ExpressionKind, span: Span) -> Result<Pattern, Error> {
    match kind {
        ExpressionKind::Identifier(id) => Ok(Pattern::new(PatternKind::Identifier(id), span)),
        ExpressionKind::Parenthesized { expression } => {
            reinterpret_expression_as_pattern(*expression)
        }
        ExpressionKind::Array { elements } => convert_array(elements, span),
        ExpressionKind::Object { properties } => convert_object(properties, span),
        ExpressionKind::Spread { argument } => convert_spread(*argument, span),
        ExpressionKind::Assignment {
            operator,
            left,
            right,
        } => convert_assignment(operator, *left, *right, span),
        ExpressionKind::This
        | ExpressionKind::Super
        | ExpressionKind::PrivateIdentifier(_)
        | ExpressionKind::Literal(_)
        | ExpressionKind::Template { .. }
        | ExpressionKind::TaggedTemplate { .. }
        | ExpressionKind::Member { .. }
        | ExpressionKind::Call { .. }
        | ExpressionKind::New { .. }
        | ExpressionKind::Update { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Logical { .. }
        | ExpressionKind::Conditional { .. }
        | ExpressionKind::Sequence { .. }
        | ExpressionKind::ArrowFunction(_)
        | ExpressionKind::FunctionExpression(_)
        | ExpressionKind::ClassExpression(_)
        | ExpressionKind::Yield { .. }
        | ExpressionKind::Await { .. }
        | ExpressionKind::Chain { .. }
        | ExpressionKind::ImportExpression { .. }
        | ExpressionKind::MetaProperty { .. } => Err(Error::InvalidArrowParameter { at: span }),
    }
}

fn convert_array(elements: Vec<Option<Expression>>, span: Span) -> Result<Pattern, Error> {
    let converted = elements.into_iter().try_fold(
        Vec::<Option<Pattern>>::new(),
        |acc, slot| -> Result<Vec<Option<Pattern>>, Error> {
            let next = slot.map(reinterpret_expression_as_pattern).transpose()?;
            Ok(acc.into_iter().chain(std::iter::once(next)).collect())
        },
    )?;
    Ok(Pattern::new(
        PatternKind::Array {
            elements: converted,
        },
        span,
    ))
}

fn convert_object(properties: Vec<ObjectMember>, span: Span) -> Result<Pattern, Error> {
    let members = properties.into_iter().try_fold(
        Vec::<ObjectPatternMember>::new(),
        |acc, member| -> Result<Vec<ObjectPatternMember>, Error> {
            let next = convert_object_member(member, span)?;
            Ok(acc.into_iter().chain(std::iter::once(next)).collect())
        },
    )?;
    Ok(Pattern::new(
        PatternKind::Object {
            properties: members,
        },
        span,
    ))
}

fn convert_object_member(
    member: ObjectMember,
    fallback_span: Span,
) -> Result<ObjectPatternMember, Error> {
    match member {
        ObjectMember::Property {
            key,
            value,
            kind,
            computed,
            shorthand,
        } => match kind {
            ObjectPropertyKind::Init => Ok(ObjectPatternMember::Property {
                key,
                value: reinterpret_expression_as_pattern(value)?,
                computed,
                shorthand,
            }),
            ObjectPropertyKind::Get | ObjectPropertyKind::Set | ObjectPropertyKind::Method => {
                Err(Error::InvalidArrowParameter { at: fallback_span })
            }
        },
        ObjectMember::Spread { argument } => Ok(ObjectPatternMember::Rest {
            argument: reinterpret_expression_as_pattern(argument)?,
        }),
    }
}

fn convert_spread(argument: Expression, span: Span) -> Result<Pattern, Error> {
    let inner = reinterpret_expression_as_pattern(argument)?;
    Ok(Pattern::new(
        PatternKind::Rest {
            argument: Box::new(inner),
        },
        span,
    ))
}

fn convert_assignment(
    operator: ecma_syntax_cat::operator::AssignmentOperator,
    left: Expression,
    right: Expression,
    span: Span,
) -> Result<Pattern, Error> {
    use ecma_syntax_cat::operator::AssignmentOperator;
    match operator {
        AssignmentOperator::Assign => {
            let pat = reinterpret_expression_as_pattern(left)?;
            Ok(Pattern::new(
                PatternKind::Assignment {
                    left: Box::new(pat),
                    right: Box::new(right),
                },
                span,
            ))
        }
        AssignmentOperator::AddAssign
        | AssignmentOperator::SubtractAssign
        | AssignmentOperator::MultiplyAssign
        | AssignmentOperator::DivideAssign
        | AssignmentOperator::RemainderAssign
        | AssignmentOperator::ExponentiationAssign
        | AssignmentOperator::LeftShiftAssign
        | AssignmentOperator::RightShiftAssign
        | AssignmentOperator::UnsignedRightShiftAssign
        | AssignmentOperator::BitwiseOrAssign
        | AssignmentOperator::BitwiseXorAssign
        | AssignmentOperator::BitwiseAndAssign
        | AssignmentOperator::LogicalOrAssign
        | AssignmentOperator::LogicalAndAssign
        | AssignmentOperator::NullishCoalescingAssign => {
            Err(Error::InvalidArrowParameter { at: span })
        }
    }
}
