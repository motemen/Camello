//! The `define_language!` macro (ADR 0004 §2).
//!
//! A single invocation is the sole source of truth for the language vocabulary.
//! From it we generate:
//!
//! * [`TokenKind`](crate::lang::TokenKind) and [`NodeKind`](crate::lang::NodeKind)
//!   as two separate `#[repr(u16)]` enums, so that a node kind can never be
//!   written into a token slot (the `builder.token(INFIX_EXPR, …)` class of bug
//!   becomes a compile error).
//! * The `SyntaxKind(u16)` conversion layer used by rowan: tokens occupy
//!   `0..TOKEN_COUNT`, nodes occupy `TOKEN_COUNT..`.
//! * The `T![…]` macro, keyed uniformly by the source spelling.
//! * `is_keyword` / `is_punct` / `is_trivia`, derived from the section a kind
//!   was declared in rather than from a hand-maintained list.
//! * The keyword string → `TokenKind` lookup used by the lexer.
//! * `Display`, so diagnostics read ``expected `}` `` instead of
//!   `Expected R_BRACE`.

/// See the module documentation. Invoked exactly once, in `crate::lang`.
macro_rules! define_language {
    (
        keywords  { $($kw_text:tt => $kw_name:ident),* $(,)? }
        punct     { $($p_text:tt => $p_name:ident),* $(,)? }
        punct_ctx { $($pc_text:tt => $pc_name:ident),* $(,)? }
        trivia    { $($tv_name:ident : $tv_disp:literal),* $(,)? }
        tokens    { $($tk_name:ident : $tk_disp:literal),* $(,)? }
        nodes     { $($nd_name:ident),* $(,)? }
    ) => {
        /// A lexical element. Never a syntax node — see ADR 0004 §1.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        #[allow(non_camel_case_types)]
        pub enum TokenKind {
            $($kw_name,)*
            $($p_name,)*
            $($pc_name,)*
            $($tv_name,)*
            $($tk_name,)*
        }

        /// A composite syntax node. Never a token — see ADR 0004 §1.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        #[allow(non_camel_case_types)]
        pub enum NodeKind {
            $($nd_name,)*
        }

        /// Number of distinct [`TokenKind`] discriminants.
        ///
        /// Tokens map to `SyntaxKind(0..TOKEN_COUNT)` and nodes to
        /// `SyntaxKind(TOKEN_COUNT..)`, which is what makes the split
        /// recoverable from a raw rowan kind.
        pub const TOKEN_COUNT: u16 = ({
            const KEYWORDS: usize = [$(stringify!($kw_name),)* ""].len() - 1;
            const PUNCT: usize = [$(stringify!($p_name),)* ""].len() - 1;
            const PUNCT_CTX: usize = [$(stringify!($pc_name),)* ""].len() - 1;
            const TRIVIA: usize = [$(stringify!($tv_name),)* ""].len() - 1;
            const TOKENS: usize = [$(stringify!($tk_name),)* ""].len() - 1;
            KEYWORDS + PUNCT + PUNCT_CTX + TRIVIA + TOKENS
        }) as u16;

        /// Number of distinct [`NodeKind`] discriminants.
        pub const NODE_COUNT: u16 = ([$(stringify!($nd_name),)* ""].len() - 1) as u16;

        impl TokenKind {
            /// Reserved words and named operators (`if`, `eq`, `qw`, …).
            #[must_use]
            pub const fn is_keyword(self) -> bool {
                matches!(self, $(TokenKind::$kw_name)|*)
            }

            /// Punctuation and symbolic operators.
            #[must_use]
            pub const fn is_punct(self) -> bool {
                matches!(self, $(TokenKind::$p_name)|* $(| TokenKind::$pc_name)*)
            }

            /// Whitespace, newlines and comments (ADR 0006 §1).
            #[must_use]
            pub const fn is_trivia(self) -> bool {
                matches!(self, $(TokenKind::$tv_name)|*)
            }

            /// The canonical source spelling, for kinds that have exactly one.
            #[must_use]
            pub const fn text(self) -> Option<&'static str> {
                match self {
                    $(TokenKind::$kw_name => Some($kw_text),)*
                    $(TokenKind::$p_name => Some($p_text),)*
                    $(TokenKind::$pc_name => Some($pc_text),)*
                    _ => None,
                }
            }

            /// Human-readable name for diagnostics (ADR 0004 §2).
            #[must_use]
            pub const fn display_name(self) -> &'static str {
                match self {
                    $(TokenKind::$kw_name => concat!("`", $kw_text, "`"),)*
                    $(TokenKind::$p_name => concat!("`", $p_text, "`"),)*
                    $(TokenKind::$pc_name => concat!("`", $pc_text, "`"),)*
                    $(TokenKind::$tv_name => $tv_disp,)*
                    $(TokenKind::$tk_name => $tk_disp,)*
                }
            }

            /// Look up a reserved word by its spelling. Used by the lexer so that
            /// the keyword table lives in exactly one place (ADR 0004 §2).
            #[must_use]
            pub fn from_keyword(text: &str) -> Option<TokenKind> {
                match text {
                    $($kw_text => Some(TokenKind::$kw_name),)*
                    _ => None,
                }
            }
        }

        impl ::std::fmt::Display for TokenKind {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.display_name())
            }
        }

        impl NodeKind {
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(NodeKind::$nd_name => stringify!($nd_name),)*
                }
            }
        }

        impl ::std::fmt::Display for NodeKind {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.name())
            }
        }

        /// A [`TokenKind`] or [`NodeKind`] keyed by its source spelling.
        ///
        /// ```ignore
        /// T!["if"]   // TokenKind::IF_KW
        /// T!["{"]    // TokenKind::L_BRACE
        /// ```
        ///
        /// Kinds without a fixed spelling (`IDENT`, `NUMBER`, …) are written as
        /// plain `TokenKind::IDENT`; there is nothing for `T!` to add.
        ///
        /// The `punct_ctx` kinds deliberately have no key: `%` is `HASH_SIGIL`
        /// or `MODULO` depending on the lexer's `expect` state, so `T!["%"]`
        /// would name whichever one the table happened to list first. Callers
        /// write `TokenKind::MODULO` and say which they meant.
        macro_rules! T {
            $([$kw_text] => { $crate::lang::TokenKind::$kw_name };)*
            $([$p_text] => { $crate::lang::TokenKind::$p_name };)*
        }

        pub(crate) use T;
    };
}

pub(crate) use define_language;
