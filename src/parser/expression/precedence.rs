use crate::{SyntaxKind, T};

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
    pub const VAR_DECL: Precedence = Precedence(12); // my, our, state, local
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
        // Comma operator (lowest precedence, except for logical operators)
        T![,] => Some(OperatorInfo::new(
            Precedence::COMMA,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Fat comma operator (=> for hash pairs) - lowest precedence
        T![=>] => Some(OperatorInfo::new(
            Precedence::COMMA,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Assignment (right associative)
        T![=] => Some(OperatorInfo::new(
            Precedence::ASSIGNMENT,
            true,
            SyntaxKind::INFIX_EXPR,
        )),

        // Logical OR
        T![||] => Some(OperatorInfo::new(
            Precedence::LOGICAL_OR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Logical AND
        T![&&] => Some(OperatorInfo::new(
            Precedence::LOGICAL_AND,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Comparison operators
        T![<]
        | T![>]
        | T![<=]
        | T![>=]
        | T![==]
        | T![!=]
        | T![eq]
        | T![ne]
        | T![gt]
        | T![lt]
        | T![ge]
        | T![le]
        | T![cmp]
        | T![<=>] => Some(OperatorInfo::new(
            Precedence::COMPARISON,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Regex operators
        T![=~] | T![!~] => Some(OperatorInfo::new(
            Precedence::REGEX_MATCH,
            false,
            SyntaxKind::REGEX_EXPR,
        )),

        // Additive operators
        T![+] | T![-] | T![.] => Some(OperatorInfo::new(
            Precedence::ADDITIVE,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Bit shift operators
        T![<<] | T![>>] => Some(OperatorInfo::new(
            Precedence::BIT_SHIFT,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Multiplicative operators
        T![*] | T![/] | T![%] | T![x] => Some(OperatorInfo::new(
            Precedence::MULTIPLICATIVE,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Exponentiation operator (right associative)
        T![**] => Some(OperatorInfo::new(
            Precedence::EXPONENT,
            true,
            SyntaxKind::INFIX_EXPR,
        )),

        // Bitwise operators (ordered by precedence: &: highest, ^: middle, |: lowest)
        T![&] => Some(OperatorInfo::new(
            Precedence::BITWISE_AND,
            false,
            SyntaxKind::INFIX_EXPR,
        )),
        T![^] => Some(OperatorInfo::new(
            Precedence::BITWISE_XOR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),
        T![|] => Some(OperatorInfo::new(
            Precedence::BITWISE_OR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Defined-or operator
        T!["//"] => Some(OperatorInfo::new(
            Precedence::DEFINED_OR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Range operators
        T![..] | T![...] => Some(OperatorInfo::new(
            Precedence::RANGE,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        // Low-precedence logical operators (NOT_KW is handled as a prefix operator)
        T![and] => Some(OperatorInfo::new(
            Precedence::LOGICAL_AND_KW,
            false,
            SyntaxKind::INFIX_EXPR,
        )),
        T![or] | T![xor] => Some(OperatorInfo::new(
            Precedence::LOGICAL_OR_XOR,
            false,
            SyntaxKind::INFIX_EXPR,
        )),

        _ => None,
    }
}
