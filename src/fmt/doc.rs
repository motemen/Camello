//! The document IR (the formatter contract).
//!
//! Everything the formatter decides about layout is decided while building this
//! and is then fixed. The renderer only walks it. That is the whole point: the
//! old formatter appended straight to a string, so a decision could only be made
//! from what had already been written, and could never be revised.

use crate::lang::SyntaxToken;

/// What a run of vertically alignable things belongs to (the formatter contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorClass {
    /// `=` and the compound assignments.
    Assign,
    /// `=>`, distinguished by nesting depth so that an inner hash aligns
    /// separately from the one containing it.
    FatComma(u8),
    /// The operator that supplies a default: `$args->{port} // 8080`,
    /// `$opt->{name} || 'anon'`. One class, so a run of lines mixing the two
    /// still agrees on one column. `or` is not one of these: it binds loosely
    /// enough to be flow control rather than a default.
    Fallback,
    /// A postfix `if` / `unless` / `while` / `until` / `for`.
    PostfixKeyword,
    /// An end-of-line comment.
    TrailingComment,
}

/// Where a comment goes relative to the code it was attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// On a line of its own.
    OwnLine,
    /// At the end of the line it shares with code.
    Trailing,
}

#[derive(Debug, Clone)]
pub enum Doc {
    /// An ordinary token; its text comes from the tree.
    Token(SyntaxToken),
    /// Content reproduced byte for byte: heredoc bodies, POD, `__DATA__`,
    /// quote-like contents and string literals.
    ///
    /// The renderer neither splits these nor indents inside them, which is what
    /// makes F1 — indentation injected into a multi-line string literal, growing
    /// with every pass — not merely fixed but unrepresentable.
    ///
    /// Text rather than a token, because the unit is not always one token: a
    /// quote-like operator is one lexical run (the lexer contract) and is written as
    /// one atom, delimiters included.
    Raw(smol_str::SmolStr),
    /// Verbatim content that owns whole lines and starts in column 0.
    ///
    /// A heredoc body is the case: it begins at a line start in the source and
    /// must begin at a line start in the output, whatever the indentation of the
    /// statement its marker appeared in. POD, `__DATA__` and the picture lines
    /// of a `format` are the same.
    ///
    /// Text rather than a token, because a region is not always one token —
    /// `__END__`, the newline after it and the data are three — and the whole
    /// region has to be written in one go. Written token by token, the second
    /// one starts on an empty line and picks up the enclosing indentation.
    VerbatimLines(smol_str::SmolStr),
    /// Exactly one space. Spacing is decided during build, so the renderer never
    /// inserts one on its own and there is nothing for a call site to bypass.
    Space,
    Concat(Vec<Doc>),
    /// A layout unit whose flat-or-broken state was decided at build time from
    /// the source (the formatter contract).
    Group {
        broken: bool,
        body: Box<Doc>,
    },
    /// One indent unit for any line break inside.
    Indent(Box<Doc>),
    /// The extent of a continuation indent (docs/formatting.md INDENT-3).
    ///
    /// The first line break the user made inside this takes one indent level,
    /// and every line until the end of it is written at that level — so an
    /// `Indent` nested inside is one level deeper again, and the closing bracket
    /// emitted *after* it is back where the construct started. Applying the
    /// level to one line at a time instead made the contents of a bracket that
    /// broke inside a wrapped argument list no deeper than the bracket itself.
    Continuation(Box<Doc>),
    /// Newline in a broken group, a space in a flat one.
    Line,
    /// Newline in a broken group, nothing in a flat one.
    SoftLine,
    /// Always a newline.
    HardLine,
    /// A place the user may have put a newline, preserved individually
    /// (docs/formatting.md POLICY-4).
    UserLine {
        broken: bool,
    },
    /// One blank line, already normalised to at most one.
    BlankLine,
    /// A zero-width alignment anchor.
    /// An alignment point, and the width of what follows it that has to end at
    /// the column the group agrees on.
    ///
    /// `=` and `-=` belong to one class and are two widths: what a reader lines
    /// up on is the `=`, so the shorter operator is the one that gets padded in
    /// front. A width of 0 means the group agrees at the anchor itself, which is
    /// what `=>` and a trailing comment want.
    Anchor(AnchorClass, usize),
    Comment(smol_str::SmolStr, Placement),
    /// Declares the shape of the statement now being emitted, so the align pass
    /// can tell where one group of comparable statements ends and the next
    /// begins. Produces no output.
    ///
    /// `None` is a declaration too — "the statement now being emitted has no
    /// comparable shape" — and every statement makes one. A statement that
    /// declared nothing would leave the previous statement's shape standing, and
    /// the two would align with each other.
    Shape(Option<ShapeKey>),
    /// Nothing at all. Lets builders return a value unconditionally.
    Nil,
}

impl Doc {
    #[must_use]
    pub fn concat(parts: Vec<Doc>) -> Doc {
        match parts.len() {
            0 => Doc::Nil,
            1 => parts.into_iter().next().expect("length checked"),
            _ => Doc::Concat(parts),
        }
    }

    #[must_use]
    pub fn group(broken: bool, body: Doc) -> Doc {
        Doc::Group {
            broken,
            body: Box::new(body),
        }
    }

    #[must_use]
    pub fn indent(body: Doc) -> Doc {
        Doc::Indent(Box::new(body))
    }

    #[must_use]
    pub fn continuation(body: Doc) -> Doc {
        Doc::Continuation(Box::new(body))
    }

    /// Whether this contributes nothing to the output.
    #[must_use]
    pub fn is_nil(&self) -> bool {
        match self {
            Doc::Nil => true,
            Doc::Concat(parts) => parts.iter().all(Doc::is_nil),
            _ => false,
        }
    }
}

/// A statement's shape, used to decide where an alignment group ends
/// (the formatter contract, docs/formatting.md §7).
///
/// Two statements only align with each other if these match, which is what stops
/// `my $x = 1;` from aligning with `$y = 2;`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeKey {
    pub statement: crate::lang::NodeKind,
    /// Whether the statement declares. docs/formatting.md §7 keys on the *presence*
    /// of `my`/`our`/`state`/`local`, not on which one, so a run of mixed
    /// declarations still aligns.
    pub declares: bool,
    pub list_assignment: bool,
}
