//! Operator precedence tables.
//!
//! Maps token kinds to the operator they represent plus a numeric precedence
//! level (higher = binds tighter) and an associativity flag.

use ecma_lex_cat::token::TokenKind;
use ecma_syntax_cat::operator::{BinaryOperator, LogicalOperator};

/// Information about a binary infix operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinaryInfo {
    /// The classified operator (binary or logical).
    pub op: BinaryKind,
    /// Numeric precedence; higher binds tighter.
    pub precedence: u8,
    /// True for right-associative operators (only `**` among binaries).
    pub right_assoc: bool,
}

/// Whether the infix operator surfaces as a [`BinaryOperator`] or a
/// [`LogicalOperator`] in the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryKind {
    /// Maps to `ExpressionKind::Binary`.
    Binary(BinaryOperator),
    /// Maps to `ExpressionKind::Logical`.
    Logical(LogicalOperator),
}

/// Minimum precedence the parser starts at (admits all binary operators).
pub const MIN_PRECEDENCE: u8 = 1;

/// Look up the binary-operator information for the token at the lookahead
/// position, returning `None` if the token is not a binary operator.
#[must_use]
#[allow(clippy::too_many_lines)] // one arm per binary-operator TokenKind variant
pub fn binary_info(kind: &TokenKind) -> Option<BinaryInfo> {
    match kind {
        // Nullish coalescing (lowest)
        TokenKind::QQ => Some(BinaryInfo {
            op: BinaryKind::Logical(LogicalOperator::NullishCoalescing),
            precedence: 3,
            right_assoc: false,
        }),
        TokenKind::PipePipe => Some(BinaryInfo {
            op: BinaryKind::Logical(LogicalOperator::Or),
            precedence: 4,
            right_assoc: false,
        }),
        TokenKind::AmpAmp => Some(BinaryInfo {
            op: BinaryKind::Logical(LogicalOperator::And),
            precedence: 5,
            right_assoc: false,
        }),
        TokenKind::Pipe => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::BitwiseOr),
            precedence: 6,
            right_assoc: false,
        }),
        TokenKind::Caret => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::BitwiseXor),
            precedence: 7,
            right_assoc: false,
        }),
        TokenKind::Amp => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::BitwiseAnd),
            precedence: 8,
            right_assoc: false,
        }),
        TokenKind::EqEq => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::Equal),
            precedence: 9,
            right_assoc: false,
        }),
        TokenKind::BangEq => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::NotEqual),
            precedence: 9,
            right_assoc: false,
        }),
        TokenKind::EqEqEq => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::StrictEqual),
            precedence: 9,
            right_assoc: false,
        }),
        TokenKind::BangEqEq => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::StrictNotEqual),
            precedence: 9,
            right_assoc: false,
        }),
        TokenKind::Lt => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::LessThan),
            precedence: 10,
            right_assoc: false,
        }),
        TokenKind::LtEq => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::LessThanOrEqual),
            precedence: 10,
            right_assoc: false,
        }),
        TokenKind::Gt => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::GreaterThan),
            precedence: 10,
            right_assoc: false,
        }),
        TokenKind::GtEq => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::GreaterThanOrEqual),
            precedence: 10,
            right_assoc: false,
        }),
        TokenKind::KwInstanceof => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::InstanceOf),
            precedence: 10,
            right_assoc: false,
        }),
        TokenKind::KwIn => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::In),
            precedence: 10,
            right_assoc: false,
        }),
        TokenKind::LtLt => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::LeftShift),
            precedence: 11,
            right_assoc: false,
        }),
        TokenKind::GtGt => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::RightShift),
            precedence: 11,
            right_assoc: false,
        }),
        TokenKind::GtGtGt => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::UnsignedRightShift),
            precedence: 11,
            right_assoc: false,
        }),
        TokenKind::Plus => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::Add),
            precedence: 12,
            right_assoc: false,
        }),
        TokenKind::Minus => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::Subtract),
            precedence: 12,
            right_assoc: false,
        }),
        TokenKind::Star => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::Multiply),
            precedence: 13,
            right_assoc: false,
        }),
        TokenKind::Slash => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::Divide),
            precedence: 13,
            right_assoc: false,
        }),
        TokenKind::Percent => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::Remainder),
            precedence: 13,
            right_assoc: false,
        }),
        TokenKind::StarStar => Some(BinaryInfo {
            op: BinaryKind::Binary(BinaryOperator::Exponentiation),
            precedence: 14,
            right_assoc: true,
        }),
        _other => None,
    }
}

/// Look up the assignment operator for the token, returning `None` if not
/// an assignment.
#[must_use]
pub fn assignment_operator(
    kind: &TokenKind,
) -> Option<ecma_syntax_cat::operator::AssignmentOperator> {
    use ecma_syntax_cat::operator::AssignmentOperator as A;
    match kind {
        TokenKind::Eq => Some(A::Assign),
        TokenKind::PlusEq => Some(A::AddAssign),
        TokenKind::MinusEq => Some(A::SubtractAssign),
        TokenKind::StarEq => Some(A::MultiplyAssign),
        TokenKind::SlashEq => Some(A::DivideAssign),
        TokenKind::PercentEq => Some(A::RemainderAssign),
        TokenKind::StarStarEq => Some(A::ExponentiationAssign),
        TokenKind::LtLtEq => Some(A::LeftShiftAssign),
        TokenKind::GtGtEq => Some(A::RightShiftAssign),
        TokenKind::GtGtGtEq => Some(A::UnsignedRightShiftAssign),
        TokenKind::AmpEq => Some(A::BitwiseAndAssign),
        TokenKind::PipeEq => Some(A::BitwiseOrAssign),
        TokenKind::CaretEq => Some(A::BitwiseXorAssign),
        TokenKind::AmpAmpEq => Some(A::LogicalAndAssign),
        TokenKind::PipePipeEq => Some(A::LogicalOrAssign),
        TokenKind::QQEq => Some(A::NullishCoalescingAssign),
        _other => None,
    }
}
