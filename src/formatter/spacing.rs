use crate::SyntaxKind;

/// Represents the spacing behavior of a token
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenSpacing {
    /// Whether this token requires a space before it (in general)
    pub space_before: SpaceRule,
    /// Whether this token requires a space after it (in general)
    pub space_after: SpaceRule,
    /// Token category for contextual spacing rules
    pub category: TokenCategory,
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
    Keyword,
    Delimiter,
    Identifier,
    Variable,
    Punctuation,
    Literal,
}

impl TokenSpacing {
    pub const fn new(
        space_before: SpaceRule,
        space_after: SpaceRule,
        category: TokenCategory,
    ) -> Self {
        Self {
            space_before,
            space_after,
            category,
        }
    }

    /// Convenient constructors for common patterns
    pub const fn binary_op() -> Self {
        Self::new(
            SpaceRule::Always,
            SpaceRule::Always,
            TokenCategory::BinaryOperator,
        )
    }

    pub const fn prefix_op() -> Self {
        Self::new(
            SpaceRule::Contextual,
            SpaceRule::Never,
            TokenCategory::PrefixOperator,
        )
    }

    pub const fn keyword() -> Self {
        Self::new(
            SpaceRule::Contextual,
            SpaceRule::Always,
            TokenCategory::Keyword,
        )
    }
}

/// Get spacing information for a token
pub const fn get_token_spacing(kind: SyntaxKind) -> TokenSpacing {
    use SpaceRule::*;
    use TokenCategory::*;

    match kind {
        // Arrow operator (highest priority - never spaces)
        SyntaxKind::ARROW => TokenSpacing::new(Never, Never, BinaryOperator),

        // Ternary operators
        SyntaxKind::QUESTION_MARK => TokenSpacing::new(Always, Never, BinaryOperator),
        SyntaxKind::COLON => TokenSpacing::new(Always, Never, BinaryOperator),

        // Binary operators that always need spaces
        SyntaxKind::EQ => TokenSpacing::binary_op(),
        SyntaxKind::PLUS => TokenSpacing::binary_op(),
        SyntaxKind::MINUS => TokenSpacing::binary_op(),
        SyntaxKind::DOT => TokenSpacing::binary_op(),
        SyntaxKind::FAT_COMMA => TokenSpacing::binary_op(),
        SyntaxKind::STAR => TokenSpacing::binary_op(),
        SyntaxKind::SLASH => TokenSpacing::binary_op(),
        SyntaxKind::MODULO => TokenSpacing::binary_op(),
        SyntaxKind::X => TokenSpacing::binary_op(),

        // Comparison operators
        SyntaxKind::GT
        | SyntaxKind::LT
        | SyntaxKind::GE
        | SyntaxKind::LE
        | SyntaxKind::EQ_EQ
        | SyntaxKind::NE
        | SyntaxKind::STR_EQ
        | SyntaxKind::STR_NE
        | SyntaxKind::STR_GT
        | SyntaxKind::STR_LT
        | SyntaxKind::STR_GE
        | SyntaxKind::STR_LE
        | SyntaxKind::STR_CMP
        | SyntaxKind::SPACESHIP => TokenSpacing::binary_op(),

        // Regex operators
        SyntaxKind::REGEX_MATCH | SyntaxKind::REGEX_NOT_MATCH => TokenSpacing::binary_op(),

        // Logical operators
        SyntaxKind::LOGICAL_AND | SyntaxKind::LOGICAL_OR => TokenSpacing::binary_op(),
        SyntaxKind::LOGICAL_NOT => TokenSpacing::prefix_op(),
        SyntaxKind::NOT_KW | SyntaxKind::AND_KW | SyntaxKind::OR_KW | SyntaxKind::XOR_KW => {
            TokenSpacing::binary_op()
        }
        SyntaxKind::DEFINED_OR => TokenSpacing::binary_op(),

        // Comma: no space before, space after
        SyntaxKind::COMMA => TokenSpacing::new(Never, Always, Punctuation),

        // Keywords that need space after
        SyntaxKind::MY_KW
        | SyntaxKind::OUR_KW
        | SyntaxKind::STATE_KW
        | SyntaxKind::LOCAL_KW
        | SyntaxKind::FOR_KW
        | SyntaxKind::FOREACH_KW
        | SyntaxKind::WHILE_KW
        | SyntaxKind::IF_KW
        | SyntaxKind::UNLESS_KW
        | SyntaxKind::ELSIF_KW
        | SyntaxKind::ELSE_KW
        | SyntaxKind::PACKAGE_KW
        | SyntaxKind::USE_KW
        | SyntaxKind::SUB_KW => TokenSpacing::keyword(),

        // RETURN_KW: contextual spacing (no space before semicolon)
        SyntaxKind::RETURN_KW => TokenSpacing::new(Contextual, Contextual, Keyword),

        // Delimiters
        SyntaxKind::L_BRACE => TokenSpacing::new(Always, Never, Delimiter),
        SyntaxKind::R_BRACE => TokenSpacing::new(Never, Contextual, Delimiter),
        SyntaxKind::L_PAREN => TokenSpacing::new(Contextual, Never, Delimiter),
        SyntaxKind::R_PAREN => TokenSpacing::new(Never, Contextual, Delimiter),
        SyntaxKind::L_BRACKET => TokenSpacing::new(Contextual, Never, Delimiter),
        SyntaxKind::R_BRACKET => TokenSpacing::new(Never, Contextual, Delimiter),

        // Semicolon: no space before, contextual after
        SyntaxKind::SEMICOLON => TokenSpacing::new(Never, Contextual, Punctuation),

        // Double colon: no spaces around
        SyntaxKind::DOUBLE_COLON => TokenSpacing::new(Never, Never, Punctuation),

        // Identifiers and variables
        SyntaxKind::IDENT | SyntaxKind::QUALIFIED_IDENT => {
            TokenSpacing::new(Contextual, Contextual, Identifier)
        }

        // Variables
        SyntaxKind::SCALAR_VAR | SyntaxKind::ARRAY_VAR | SyntaxKind::HASH_VAR => {
            TokenSpacing::new(Contextual, Contextual, Variable)
        }

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
    if let Some(needs_space) = handle_special_cases(prev, context.current_token, context) {
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
fn handle_special_cases(
    prev: SyntaxKind,
    current: SyntaxKind,
    _context: &SpacingContext,
) -> Option<bool> {
    use SyntaxKind::*;

    match (prev, current) {
        // Arrow operator: highest priority, never spaces
        (ARROW, _) | (_, ARROW) => Some(false),

        // Comma: no space before, space after (high priority)
        (_, COMMA) => Some(false),
        (COMMA, _) => Some(true),

        // Exception: no space before semicolon when previous token is slash (for q-string delimiters)
        (SLASH, SEMICOLON) => Some(false),

        // Logical NOT: special handling
        (L_PAREN, LOGICAL_NOT) => Some(false), // No space after (
        (_, LOGICAL_NOT) => Some(true),        // Space before ! in other cases
        (LOGICAL_NOT, _) => Some(false),       // No space after !

        // RETURN_KW: no space before semicolon, but space before other tokens
        (RETURN_KW, SEMICOLON) => Some(false),
        (RETURN_KW, _) => Some(true),

        // R_BRACE: space before expressions but not semicolons or closing parentheses
        (R_BRACE, kind) if kind != SEMICOLON && kind != R_PAREN => Some(true),
        (R_BRACE, SEMICOLON | R_PAREN) => Some(false),

        // L_PAREN after certain tokens
        (SCALAR_VAR | ARRAY_VAR | HASH_VAR, L_PAREN) => Some(true),
        (
            MY_KW | OUR_KW | STATE_KW | LOCAL_KW | FOR_KW | FOREACH_KW | WHILE_KW | IF_KW
            | UNLESS_KW | ELSIF_KW,
            L_PAREN,
        ) => Some(true),

        // After identifier: special rules (exclude R_PAREN to fix function call spacing)
        (IDENT, SEMICOLON | DOUBLE_COLON | L_PAREN | R_PAREN) => Some(false),
        (IDENT, _) => Some(true),

        // SUB_KW with identifiers
        (SUB_KW, IDENT | QUALIFIED_IDENT) => Some(true),

        // No space inside parentheses/brackets/braces
        (_, R_PAREN | R_BRACE | R_BRACKET) => Some(false),
        (L_PAREN | L_BRACE | L_BRACKET, _) => Some(false),

        _ => None,
    }
}

/// Handle contextual spacing when general rules aren't sufficient
fn handle_contextual_spacing(
    prev: SyntaxKind,
    current: SyntaxKind,
    _prev_spacing: &TokenSpacing,
    _current_spacing: &TokenSpacing,
) -> bool {
    use SyntaxKind::*;

    match (prev, current) {
        // Comma: always space after
        (COMMA, _) => true,
        (_, COMMA) => false,

        // Keywords in postfix position (if/unless)
        (_, IF_KW | UNLESS_KW) => true,

        // Generally no space inside delimiters
        (L_PAREN | L_BRACE | L_BRACKET, _) => false,
        (_, R_PAREN | R_BRACE | R_BRACKET) => false,

        _ => false,
    }
}
