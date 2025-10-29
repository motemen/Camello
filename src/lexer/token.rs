//! Token definitions for the Perl lexer.

use crate::{SyntaxKind, T};
use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Sigils（変数の型を示すプレフィックス）
    #[token("$#")]
    DollarHash,

    #[token("$")]
    Dollar,

    #[token("@")]
    At,

    // Reference operator
    #[token("\\")]
    Backslash,

    // Ampersand (for function references)
    #[token("&")]
    Ampersand,

    // データセクションキーワード (must come before Ident to take precedence)
    #[token("__END__")]
    EndKw,

    #[token("__DATA__")]
    DataKw,

    // Keywords (must come before Ident to take precedence)
    #[token("require")]
    RequireKw,

    #[token("BEGIN")]
    BeginKw,

    #[token("END")]
    EndBlockKw,

    #[token("INIT")]
    InitKw,

    #[token("CHECK")]
    CheckKw,

    #[token("UNITCHECK")]
    UnitcheckKw,

    // POD commands (must be at line start)
    // Note: POD detection is handled manually in lexer due to line-start requirement
    PodCommand,

    // 識別子（サブルーチン名など）
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    // リテラル
    // Numeric literals with underscores and multiple bases
    // Hex float (e.g., 0x1.999ap-4), must have '.' and 'p' exponent
    #[regex(
        r"0x[0-9A-Fa-f](?:_*[0-9A-Fa-f])*\.[0-9A-Fa-f](?:_*[0-9A-Fa-f])*[pP][+-]?[0-9](?:_*[0-9])*",
        priority = 3
    )]
    // Hex, binary, and octal integers
    #[regex(r"0x[0-9A-Fa-f](?:_*[0-9A-Fa-f])*")]
    #[regex(r"0b[01](?:_*[01])*")]
    #[regex(r"0o[0-7](?:_*[0-7])*")]
    // Decimal numbers (int, float, scientific)
    #[regex(
        r"[0-9](?:_*[0-9])*(?:\.[0-9](?:_*[0-9])*)?(?:[eE][+-]?[0-9](?:_*[0-9])*)?",
        priority = 1
    )]
    #[regex(r"\.[0-9](?:_*[0-9])*(?:[eE][+-]?[0-9](?:_*[0-9])*)?", priority = 1)]
    Number,

    #[regex(r#""([^"\\]|\\.)*""#)]
    #[regex(r"'([^'\\]|\\.)*'")]
    String,
    // Backtick command substitution (handled manually like RegexLiteral due to complexity)
    BacktickString,

    // Version literal (v1.23, v5.008_001, etc.) - requires at least one dot component
    #[regex(r"v[0-9]+(\.[0-9_]+)+")]
    Version,

    // Bare version number (e.g., 5.24.1) - contextually determined
    #[regex(r"[0-9]+(\.[0-9]+){2,}")]
    BareVersion,

    // RegexLiteral - handled manually via context-sensitive disambiguation
    RegexLiteral,

    // 記号
    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token(";")]
    Semicolon,

    #[token(",")]
    Comma,

    #[token("::")]
    DoubleColon,

    // Ternary operator tokens
    #[token("?")]
    QuestionMark,

    #[token(":")]
    Colon,

    // 演算子 (order matters - longer ones first!)
    #[token("=~")]
    RegexMatch,

    #[token("~~")]
    SmartMatch,

    #[token("!~")]
    RegexNotMatch,

    #[token("=>")]
    FatComma,

    #[token("->")]
    Arrow,

    #[token("<<")]
    ShiftLeft,

    #[token(">>")]
    ShiftRight,

    #[token(">=")]
    GreaterEqual,

    #[token("<=")]
    LessEqual,

    #[token("==")]
    EqualEqual,

    #[token("!=")]
    NotEqual,

    #[token("&&")]
    LogicalAnd,

    #[token("||")]
    LogicalOr,

    #[token("!")]
    LogicalNot,

    #[token("~")]
    Tilde,

    #[token("//")]
    DefinedOr,

    #[token("<=>")]
    Spaceship,

    #[token("=")]
    Eq,

    #[token("++")]
    Increment,

    #[token("--")]
    Decrement,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("...")]
    DotDotDot,

    #[token("..")]
    DotDot,

    #[token(".")]
    Dot,

    // Multiplicative operators
    #[token("**")]
    Exponent,

    #[token("*")]
    Star,

    #[token("/", priority = 1)]
    Slash,

    #[token("%")]
    Percent,

    #[token(">")]
    Greater,

    #[token("<")]
    Less,

    #[token("^")]
    Caret,

    #[token("|")]
    Pipe,

    // 空白
    #[regex(r"[ \t\f]+")]
    Whitespace,

    // 改行（重要なので個別にトークン化）
    #[regex(r"\r\n|\r|\n")]
    Newline,

    // コメント
    #[regex(r"#[^\r\n]*")]
    Comment,

    // データセクション（__END__ / __DATA__ 以降のすべてのテキスト）
    DataSection,

    // Postfix dereference operators (handled manually due to context sensitivity)
    PostfixDerefArray,          // ->@*
    PostfixDerefHash,           // ->%*
    PostfixDerefScalar,         // ->$*
    PostfixDerefArrayLastIndex, // ->$#*
    PostfixDerefCode,           // ->&*
    PostfixDerefGlob,           // ->**
}

impl Token {
    #[must_use]
    pub fn to_syntax_kind(&self) -> SyntaxKind {
        match self {
            Token::DollarHash => SyntaxKind::ARRAY_INDEX_SIGIL,
            Token::Dollar => SyntaxKind::SCALAR_SIGIL,
            Token::At => SyntaxKind::ARRAY_SIGIL,
            Token::Backslash => T!['\\'],
            Token::Ampersand => SyntaxKind::CODE_SIGIL, // Will be disambiguated
            Token::RequireKw => T![require],
            Token::BeginKw => T![BEGIN],
            Token::EndBlockKw => T![END],
            Token::InitKw => T![INIT],
            Token::CheckKw => T![CHECK],
            Token::UnitcheckKw => T![UNITCHECK],
            Token::Ident => SyntaxKind::IDENT,
            Token::Number => SyntaxKind::NUMBER,
            Token::String => SyntaxKind::STRING,
            Token::BacktickString => SyntaxKind::BACKTICK_STRING,
            Token::Version => SyntaxKind::VERSION,
            Token::BareVersion => SyntaxKind::BARE_VERSION,
            Token::RegexLiteral => SyntaxKind::REGEX_LITERAL,
            Token::EndKw => T![__END__],
            Token::DataKw => T![__DATA__],
            Token::PodCommand => SyntaxKind::POD_CONTENT, // Not used anymore
            Token::LBrace => T!['{'],
            Token::RBrace => T!['}'],
            Token::LParen => T!['('],
            Token::RParen => T![')'],
            Token::LBracket => T!['['],
            Token::RBracket => T![']'],
            Token::Semicolon => T![;],
            Token::Comma => T![,],
            Token::DoubleColon => T![::],
            Token::QuestionMark => T![?],
            Token::Colon => T![:],
            Token::Eq => T![=],
            Token::Increment => T![++],
            Token::Decrement => T![--],
            Token::Plus => T![+],
            Token::Minus => T![-],
            Token::DotDotDot => T![...],
            Token::DotDot => T![..],
            Token::Dot => T![.],
            Token::Arrow => T![->],
            Token::FatComma => T![=>],
            Token::Exponent => T![**],
            Token::Star => T![*],
            Token::Slash => T![/],
            Token::Percent => T![%],
            Token::Greater => T![>],
            Token::Less => T![<],
            Token::ShiftLeft => T![<<],
            Token::ShiftRight => T![>>],
            Token::Caret => SyntaxKind::CARET, // Will be disambiguated
            Token::Pipe => T![|],
            Token::GreaterEqual => T![>=],
            Token::LessEqual => T![<=],
            Token::EqualEqual => T![==],
            Token::NotEqual => T![!=],
            Token::RegexMatch => T![=~],
            Token::SmartMatch => T![~~],
            Token::RegexNotMatch => T![!~],
            Token::LogicalAnd => T![&&],
            Token::LogicalOr => T![||],
            Token::LogicalNot => T![!],
            Token::Tilde => T![~],
            Token::DefinedOr => T!["//"],
            Token::Spaceship => T![<=>],
            Token::Whitespace => SyntaxKind::WHITESPACE,
            Token::Newline => SyntaxKind::NEWLINE,
            Token::Comment => SyntaxKind::COMMENT,
            Token::DataSection => SyntaxKind::DATA_SECTION,
            Token::PostfixDerefArray => SyntaxKind::POSTFIX_DEREF_ARRAY,
            Token::PostfixDerefHash => SyntaxKind::POSTFIX_DEREF_HASH,
            Token::PostfixDerefScalar => SyntaxKind::POSTFIX_DEREF_SCALAR,
            Token::PostfixDerefArrayLastIndex => SyntaxKind::POSTFIX_DEREF_ARRAY_LAST_INDEX,
            Token::PostfixDerefCode => SyntaxKind::POSTFIX_DEREF_CODE,
            Token::PostfixDerefGlob => SyntaxKind::POSTFIX_DEREF_GLOB,
        }
    }
}
