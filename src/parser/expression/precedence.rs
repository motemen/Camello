use crate::SyntaxKind;

/// Operator precedence levels for Pratt parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Precedence(pub u8);

impl Precedence {
    pub const LOWEST: Precedence = Precedence(0);
    pub const COMMA: Precedence = Precedence(1); // , => (comma and fat comma)
    pub const LIST_ITEM: Precedence = Precedence(Precedence::COMMA.0 + 1); // expressions in lists
    pub const LOGICAL_OR_XOR: Precedence = Precedence(5); // or, xor (lowest precedence)
    pub const LOGICAL_AND_KW: Precedence = Precedence(6); // and
    pub const LOGICAL_NOT_KW: Precedence = Precedence(7); // not
    pub const ASSIGNMENT: Precedence = Precedence(10); // =
    pub const TERNARY: Precedence = Precedence(15); // ?: (ternary conditional)
    pub const RANGE: Precedence = Precedence(17); // .. and ... range operators
    pub const DEFINED_OR: Precedence = Precedence(20); // // (same as ||)
    pub const LOGICAL_OR: Precedence = Precedence(20); // ||
    pub const LOGICAL_AND: Precedence = Precedence(30); // &&
    pub const COMPARISON: Precedence = Precedence(40); // ==, !=, <, >, <=, >=, eq, ne, lt, gt, le, ge, cmp, <=>
                                                       // Bitwise operators must bind less tightly than comparisons and more tightly than logical &&/||
    pub const BITWISE_AND: Precedence = Precedence(39); // & (bitwise AND)
    pub const BITWISE_XOR: Precedence = Precedence(38); // ^ (bitwise XOR)
    pub const BITWISE_OR: Precedence = Precedence(37); // | (bitwise OR)
    pub const BIT_SHIFT: Precedence = Precedence(48); // <<, >>
    pub const ADDITIVE: Precedence = Precedence(50); // +, -, .
    pub const MULTIPLICATIVE: Precedence = Precedence(60); // *, /, %, x
    pub const EXPONENT: Precedence = Precedence(75); // ** (exponentiation)
    pub const PREFIX: Precedence = Precedence(65); // ! (logical not prefix)
    pub const REGEX_MATCH: Precedence = Precedence(70); // =~, !~
    pub const POSTFIX: Precedence = Precedence(80); // ->, [], {}, ()
}

/// Operator information for Pratt parsing
#[derive(Debug, Clone, Copy)]
pub struct OperatorInfo {
    pub precedence: Precedence,
    pub right_associative: bool,
    pub node_kind: SyntaxKind,
}

impl OperatorInfo {
    pub const fn new(
        precedence: Precedence,
        right_associative: bool,
        node_kind: SyntaxKind,
    ) -> Self {
        Self {
            precedence,
            right_associative,
            node_kind,
        }
    }
}

/// Get operator information for binary operators
pub fn get_operator_info(kind: SyntaxKind) -> Option<OperatorInfo> {
    match kind {
        // Fat comma operator (=> for hash pairs) - lowest precedence
        SyntaxKind::FAT_COMMA => Some(OperatorInfo::new(
            Precedence::COMMA,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Assignment (right associative)
        SyntaxKind::EQ => Some(OperatorInfo::new(
            Precedence::ASSIGNMENT,
            true,
            SyntaxKind::INFIX_EXPR,
        )),

        // Logical OR
        SyntaxKind::LOGICAL_OR => Some(OperatorInfo::new(
            Precedence::LOGICAL_OR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Logical AND
        SyntaxKind::LOGICAL_AND => Some(OperatorInfo::new(
            Precedence::LOGICAL_AND,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Comparison operators
        SyntaxKind::LT
        | SyntaxKind::GT
        | SyntaxKind::LE
        | SyntaxKind::GE
        | SyntaxKind::EQ_EQ
        | SyntaxKind::NE
        | SyntaxKind::STR_EQ
        | SyntaxKind::STR_NE
        | SyntaxKind::STR_GT
        | SyntaxKind::STR_LT
        | SyntaxKind::STR_GE
        | SyntaxKind::STR_LE
        | SyntaxKind::STR_CMP
        | SyntaxKind::SPACESHIP => Some(OperatorInfo::new(
            Precedence::COMPARISON,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Regex operators
        SyntaxKind::REGEX_MATCH | SyntaxKind::REGEX_NOT_MATCH => Some(OperatorInfo::new(
            Precedence::REGEX_MATCH,
            false,
            SyntaxKind::REGEX_EXPR,
        )),

        // Additive operators
        SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::DOT => Some(OperatorInfo::new(
            Precedence::ADDITIVE,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Bit shift operators
        SyntaxKind::SHIFT_LEFT | SyntaxKind::SHIFT_RIGHT => Some(OperatorInfo::new(
            Precedence::BIT_SHIFT,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Multiplicative operators
        SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::MODULO | SyntaxKind::X => Some(
            OperatorInfo::new(Precedence::MULTIPLICATIVE, false, SyntaxKind::INFIX_EXPR),
        ),

        // Exponentiation operator (right associative)
        SyntaxKind::EXPONENT => Some(OperatorInfo::new(
            Precedence::EXPONENT,
            true,
            SyntaxKind::INFIX_EXPR,
        )),

        // Bitwise operators (ordered by precedence: &: highest, ^: middle, |: lowest)
        SyntaxKind::BITWISE_AND => Some(OperatorInfo::new(
            Precedence::BITWISE_AND,
            false,
            SyntaxKind::INFIX_EXPR,
        )),
        SyntaxKind::BITWISE_XOR => Some(OperatorInfo::new(
            Precedence::BITWISE_XOR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),
        SyntaxKind::BITWISE_OR => Some(OperatorInfo::new(
            Precedence::BITWISE_OR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Defined-or operator
        SyntaxKind::DEFINED_OR => Some(OperatorInfo::new(
            Precedence::DEFINED_OR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Range operators
        SyntaxKind::RANGE | SyntaxKind::RANGE_EXCLUSIVE => Some(OperatorInfo::new(
            Precedence::RANGE,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Low-precedence logical operators (NOT_KW is handled as a prefix operator)
        SyntaxKind::AND_KW => Some(OperatorInfo::new(
            Precedence::LOGICAL_AND_KW,
            false,
            SyntaxKind::INFIX_EXPR,
        )),
        SyntaxKind::OR_KW | SyntaxKind::XOR_KW => Some(OperatorInfo::new(
            Precedence::LOGICAL_OR_XOR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        _ => None,
    }
}
