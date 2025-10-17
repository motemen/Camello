use crate::{SyntaxKind, T};

/// Represents the spacing behavior of a token
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenSpacing {
    /// Whether this token requires a space before it (in general)
    space_before: SpaceRule,
    /// Whether this token requires a space after it (in general)
    space_after: SpaceRule,
    /// Token category for contextual spacing rules
    category: TokenCategory,
}

/// Rules for spacing around tokens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceRule {
    /// Never add space
    Never,
    /// Always add space (unless overridden by context)
    Always,
    /// Add space only in certain contexts
    Contextual,
    /// No general rule (depends entirely on context)
    None,
}

/// Categories of tokens for contextual spacing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenCategory {
    BinaryOperator,
    PrefixOperator,
    PostfixOperator,
    Keyword,
    Delimiter,
    Identifier,
    Variable,
    Punctuation,
    Literal,
}

impl TokenSpacing {
    const fn new(space_before: SpaceRule, space_after: SpaceRule, category: TokenCategory) -> Self {
        Self {
            space_before,
            space_after,
            category,
        }
    }

    /// Convenient constructors for common patterns
    const fn binary_op() -> Self {
        Self::new(
            SpaceRule::Always,
            SpaceRule::Always,
            TokenCategory::BinaryOperator,
        )
    }

    const fn prefix_op() -> Self {
        Self::new(
            SpaceRule::Contextual,
            SpaceRule::Never,
            TokenCategory::PrefixOperator,
        )
    }

    const fn postfix_op() -> Self {
        Self::new(
            SpaceRule::Never,
            SpaceRule::Contextual,
            TokenCategory::PostfixOperator,
        )
    }

    const fn keyword() -> Self {
        Self::new(
            SpaceRule::Contextual,
            SpaceRule::Always,
            TokenCategory::Keyword,
        )
    }
}

/// Get spacing information for a token
pub const fn get_token_spacing(kind: SyntaxKind) -> TokenSpacing {
    use SpaceRule::{Always, Contextual, Never, None};
    use TokenCategory::{
        BinaryOperator, Delimiter, Identifier, Keyword, PrefixOperator, Punctuation, Variable,
    };

    match kind {
        // Arrow operator (highest priority - never spaces)
        T![->] => TokenSpacing::new(Never, Never, BinaryOperator),

        // Ternary operators
        T![?] | T![:] => TokenSpacing::new(Always, Never, BinaryOperator),

        // Binary operators that always need spaces
        T![=]
        | T![+]
        | T![-]
        | T![.]
        | T![=>]
        | T![*]
        | T![/]
        | T![%]
        | T![x]
        | T![**]
        | T![<<]
        | T![>>]
        | T![..]
        | T![...] => TokenSpacing::binary_op(),

        // Unary operators (prefix/postfix)
        SyntaxKind::UNARY_PLUS | SyntaxKind::UNARY_MINUS => TokenSpacing::prefix_op(),
        SyntaxKind::PREFIX_INCREMENT | SyntaxKind::PREFIX_DECREMENT => TokenSpacing::prefix_op(),
        SyntaxKind::POSTFIX_INCREMENT | SyntaxKind::POSTFIX_DECREMENT => TokenSpacing::postfix_op(),
        SyntaxKind::POSTFIX_DEREF_ARRAY
        | SyntaxKind::POSTFIX_DEREF_HASH
        | SyntaxKind::POSTFIX_DEREF_SCALAR
        | SyntaxKind::POSTFIX_DEREF_ARRAY_LAST_INDEX
        | SyntaxKind::POSTFIX_DEREF_CODE
        | SyntaxKind::POSTFIX_DEREF_GLOB => TokenSpacing::postfix_op(),

        // Comparison operators
        T![>]
        | T![<]
        | T![>=]
        | T![<=]
        | T![==]
        | T![!=]
        | T![~~]
        | T![eq]
        | T![ne]
        | T![gt]
        | T![lt]
        | T![ge]
        | T![le]
        | T![cmp]
        | T![<=>] => TokenSpacing::binary_op(),

        // Regex operators
        T![=~] | T![!~] => TokenSpacing::binary_op(),

        // Logical operators
        T![&&] | T![||] => TokenSpacing::binary_op(),
        T![!] | T![~] => TokenSpacing::prefix_op(),
        T![not] | T![and] | T![or] | T![xor] => TokenSpacing::binary_op(),
        T!["//"] => TokenSpacing::binary_op(),

        // Bitwise operators
        T![&] | T![|] | T![^] => TokenSpacing::binary_op(),

        SyntaxKind::FILE_TEST_OP => TokenSpacing::new(
            SpaceRule::Contextual,
            SpaceRule::Always,
            TokenCategory::PrefixOperator,
        ),

        // Comma: no space before, space after
        T![,] => TokenSpacing::new(Never, Always, Punctuation),

        // Keywords that need space after
        T![my]
        | T![our]
        | T![state]
        | T![local]
        | T![for]
        | T![foreach]
        | T![while]
        | T![until]
        | T![if]
        | T![unless]
        | T![elsif]
        | T![else]
        | T![catch]
        | T![finally]
        | T![given]
        | T![when]
        | T![default]
        | T![package]
        | T![use]
        | T![no]
        | T![require]
        | T![sub] => TokenSpacing::keyword(),

        // RETURN_KW and loop control keywords: contextual spacing (no space before semicolon)
        T![return] | T![next] | T![last] | T![redo] => {
            TokenSpacing::new(Contextual, Contextual, Keyword)
        }

        // Delimiters
        T!['{'] => TokenSpacing::new(Always, Never, Delimiter),
        T!['}'] => TokenSpacing::new(Never, Contextual, Delimiter),
        T!['('] => TokenSpacing::new(Contextual, Never, Delimiter),
        T![')'] => TokenSpacing::new(Never, Contextual, Delimiter),
        T!['['] => TokenSpacing::new(Contextual, Never, Delimiter),
        T![']'] => TokenSpacing::new(Never, Contextual, Delimiter),
        SyntaxKind::DELIMITER => TokenSpacing::new(Never, Never, Delimiter),

        // Semicolon: no space before, contextual after
        T![;] => TokenSpacing::new(Never, Contextual, Punctuation),

        // Double colon: no spaces around
        T![::] => TokenSpacing::new(Never, Never, Punctuation),

        // Sigils: no space after sigil
        SyntaxKind::SCALAR_SIGIL
        | SyntaxKind::ARRAY_SIGIL
        | SyntaxKind::HASH_SIGIL
        | SyntaxKind::TYPEGLOB_SIGIL
        | T!['\\']
        | SyntaxKind::CODE_SIGIL => TokenSpacing::new(Contextual, Never, PrefixOperator),

        // Identifiers and variables
        SyntaxKind::IDENT | SyntaxKind::QUALIFIED_IDENT => {
            TokenSpacing::new(Contextual, Contextual, Identifier)
        }

        // Variables
        SyntaxKind::SCALAR_VAR
        | SyntaxKind::ARRAY_VAR
        | SyntaxKind::HASH_VAR
        | SyntaxKind::TYPEGLOB_VAR => TokenSpacing::new(Contextual, Contextual, Variable),

        // Default: no specific spacing requirements
        _ => TokenSpacing::new(None, None, TokenCategory::Literal),
    }
}

/// Context for making spacing decisions
#[derive(Debug)]
pub struct SpacingContext {
    pub prev_token: Option<SyntaxKind>,
    pub current_token: SyntaxKind,
    pub at_line_start: bool,
}

/// Determine if a space is needed before the current token
pub fn needs_space_before(context: &SpacingContext) -> bool {
    if context.at_line_start {
        return false;
    }

    let Some(prev) = context.prev_token else {
        return false;
    };

    let prev_spacing = get_token_spacing(prev);
    let current_spacing = get_token_spacing(context.current_token);

    // Handle special cases first (highest priority overrides)
    if let Some(needs_space) = handle_special_cases(prev, context.current_token) {
        return needs_space;
    }

    // Apply general spacing rules with proper precedence
    match (prev_spacing.space_after, current_spacing.space_before) {
        // Never takes highest priority
        (SpaceRule::Never, _) | (_, SpaceRule::Never) => false,
        // Then Always
        (SpaceRule::Always, _) | (_, SpaceRule::Always) => true,
        // Then Contextual
        (SpaceRule::Contextual, _) | (_, SpaceRule::Contextual) => {
            handle_contextual_spacing(prev, context.current_token, &prev_spacing, &current_spacing)
        }
        // Default to no space
        (SpaceRule::None, SpaceRule::None) => false,
    }
}

/// Handle special case overrides that don't fit the general rules
fn handle_special_cases(prev: SyntaxKind, current: SyntaxKind) -> Option<bool> {
    use SyntaxKind::*;

    match (prev, current) {
        // Variables followed by other variables, identifiers, or strings need space (for filehandle syntax)
        (
            SCALAR_VAR | ARRAY_VAR | HASH_VAR | TYPEGLOB_VAR,
            SCALAR_SIGIL | ARRAY_SIGIL | HASH_SIGIL | TYPEGLOB_SIGIL | SCALAR_VAR | ARRAY_VAR
            | HASH_VAR | TYPEGLOB_VAR | IDENT | STRING,
        ) => Some(true),

        // Arrow operator: highest priority, never spaces
        (ARROW, _) | (_, ARROW) => Some(false),

        // Postfix dereference operators (->@*, ->%*, ->$*): never add spaces before them
        (
            _,
            POSTFIX_DEREF_ARRAY
            | POSTFIX_DEREF_HASH
            | POSTFIX_DEREF_SCALAR
            | POSTFIX_DEREF_ARRAY_LAST_INDEX
            | POSTFIX_DEREF_CODE
            | POSTFIX_DEREF_GLOB,
        ) => Some(false),

        // Compound assignment operators: no space between operator and =
        // e.g., ||=, &&=, +=, -=, *=, /=, %=, .=, //=, &=
        (prev_op, EQ) if prev_op.is_compoundable_operator() => Some(false),

        // Comma: no space before, space after (high priority)
        (_, COMMA) => Some(false),
        (COMMA, _) => Some(true),

        (QUESTION_MARK, _) => Some(true),
        (COLON, _) => Some(true),

        // Logical NOT: special handling
        (L_PAREN, LOGICAL_NOT) => Some(false), // No space after (
        (LOGICAL_NOT, _) => Some(false),       // No space after !
        (_, LOGICAL_NOT) => Some(true),        // Space before ! in other cases
        (L_PAREN, BITWISE_NOT) => Some(false),
        (BITWISE_NOT, _) => Some(false),
        (_, BITWISE_NOT) => Some(true),

        // RETURN_KW and loop control keywords: no space before semicolon, but space before other tokens
        (RETURN_KW | NEXT_KW | LAST_KW | REDO_KW, SEMICOLON) => Some(false),
        (RETURN_KW | NEXT_KW | LAST_KW | REDO_KW, _) => Some(true),

        // R_BRACE: space before expressions but not semicolons or closing parentheses
        (R_BRACE, kind) if kind != SEMICOLON && kind != R_PAREN => Some(true),
        (R_BRACE, SEMICOLON | R_PAREN) => Some(false),

        // L_PAREN after certain tokens
        (SCALAR_VAR | ARRAY_VAR | HASH_VAR | TYPEGLOB_VAR, L_PAREN) => Some(true),
        // COMPOUND_VAR followed by L_PAREN should have no space (for function calls like &{$code}())
        (COMPOUND_VAR, L_PAREN) => Some(false),
        (
            MY_KW | OUR_KW | STATE_KW | LOCAL_KW | FOR_KW | FOREACH_KW | WHILE_KW | UNTIL_KW
            | IF_KW | UNLESS_KW | ELSIF_KW | CATCH_KW | GIVEN_KW | WHEN_KW,
            L_PAREN,
        ) => Some(true),

        // After identifier: special rules (exclude R_PAREN to fix function call spacing)
        (IDENT, SEMICOLON | DOUBLE_COLON | L_PAREN | R_PAREN) => Some(false),
        (IDENT, _) => Some(true),

        // Quote-like delimiter followed by postfix keywords (e.g. s{}{} if)
        (
            DELIMITER,
            IF_KW | UNLESS_KW | WHILE_KW | UNTIL_KW | FOR_KW | FOREACH_KW | WHEN_KW | CATCH_KW
                | FINALLY_KW,
        ) => Some(true),

        // SUB_KW with identifiers
        (SUB_KW, IDENT | QUALIFIED_IDENT) => Some(true),

        // DELIMITER followed by binary operators needs space
        // (e.g., q(a) x 10, m/pattern/ and $var)
        (
            DELIMITER,
            X | PLUS | MINUS | STAR | SLASH | MODULO | DOT | EQ | LT | GT | LE | GE | EQ_EQ | NE
            | SMART_MATCH | STR_EQ | STR_NE | STR_GT | STR_LT | STR_GE | STR_LE | STR_CMP
            | SPACESHIP | LOGICAL_AND | LOGICAL_OR | REGEX_MATCH | REGEX_NOT_MATCH | AND_KW | OR_KW
            | XOR_KW | BITWISE_AND | BITWISE_OR | BITWISE_XOR | EXPONENT | SHIFT_LEFT | SHIFT_RIGHT,
        ) => Some(true),

        _ => None,
    }
}

/// Handle contextual spacing when general rules aren't sufficient
fn handle_contextual_spacing(
    prev: SyntaxKind,
    current: SyntaxKind,
    prev_spacing: &TokenSpacing,
    current_spacing: &TokenSpacing,
) -> bool {
    use SyntaxKind::{
        CATCH_KW, COMMA, FINALLY_KW, FOREACH_KW, FOR_KW, IF_KW, POSTFIX_DEREF_ARRAY,
        POSTFIX_DEREF_ARRAY_LAST_INDEX, POSTFIX_DEREF_CODE, POSTFIX_DEREF_GLOB, POSTFIX_DEREF_HASH,
        POSTFIX_DEREF_SCALAR, UNLESS_KW, WHEN_KW,
    };

    if prev_spacing.category == TokenCategory::Variable
        && !matches!(
            current,
            POSTFIX_DEREF_ARRAY
                | POSTFIX_DEREF_HASH
                | POSTFIX_DEREF_SCALAR
                | POSTFIX_DEREF_ARRAY_LAST_INDEX
                | POSTFIX_DEREF_CODE
                | POSTFIX_DEREF_GLOB
        )
        && matches!(
            current_spacing.category,
            TokenCategory::Identifier
                | TokenCategory::Literal
                | TokenCategory::Variable
                | TokenCategory::Keyword
        )
    {
        return true;
    }

    match (prev, current) {
        // Comma: always space after
        (COMMA, _) => true,
        (_, COMMA) => false,

        // Keywords in postfix position (if/unless/for/catch/finally/when)
        (_, IF_KW | UNLESS_KW | FOR_KW | FOREACH_KW | CATCH_KW | FINALLY_KW | WHEN_KW) => true,

        _ => false,
    }
}
