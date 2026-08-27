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
    /// The import list of a `use` or a `no`, so that a block of them reads as
    /// the table it is: module name, then what is taken from it. Off by default
    /// (`align_use_imports`).
    UseImports,
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
        /// Whether the anchors written directly inside are worth recording —
        /// that is, whether this construct occupies more than one line.
        ///
        /// Usually the same answer as `broken`, and for the same reason: a group
        /// that stays on one line has no second line for its anchors to agree
        /// with. They part company where the writer put something after the
        /// opening bracket and then broke the line anyway. `f($o,` seeds no
        /// break, so `broken` is false and nothing inside it may take a
        /// `Doc::Line` — and the lines the writer went on to write are a table
        /// like any other.
        anchored: bool,
        body: Box<Doc>,
    },
    /// One indent unit for any line break inside.
    Indent(Box<Doc>),
    /// Statements that begin their own continuation scopes.
    ///
    /// A line the writer wrapped takes one indent level for the whole of the
    /// expression it wraps (docs/formatting.md INDENT-3), and what holds that
    /// level is the bracket the expression is written inside. A block written
    /// in one is not part of that expression: `f(sub {` puts the body of the
    /// subroutine inside the argument list's scope, and a wrap in one of its
    /// statements took the level away with it — every line after it, the
    /// brackets closing the call included, came back a level deeper.
    Statements(Box<Doc>),
    /// Continuation lines begin this many columns after the statement's base
    /// indentation. Bareword calls use the width of `name ` so arguments hang
    /// from the first argument rather than from an unrelated fixed tab stop.
    ///
    /// `None` asks for wherever the scope around it hangs from rather than for
    /// a column of its own: a call written along a list takes the lines it
    /// swallowed back to the list's own column. `Some(0)` is the opposite
    /// answer and not the same one — no hanging column at all, which is what a
    /// bracket's contents ask for. They are placed from the bracket, and the
    /// call the bracket was written in hangs its arguments somewhere they have
    /// nothing to do with.
    Hanging {
        columns: Option<usize>,
        body: Box<Doc>,
    },
    /// A construct placed from the line it begins on rather than from the
    /// statement's own level (docs/formatting.md INDENT-4).
    ///
    /// A block's body is one level below the construct that owns it and its
    /// closing brace is back at that construct's level — and when the user
    /// wrapped the line first, that level is not the statement's. `$cond\n? do
    /// {` writes the `do` on a continuation line, and without this its body and
    /// its closing brace came back at the statement's level, shallower than
    /// their own header.
    ///
    /// The scope is the construct and not the block, because a wrapped
    /// condition or signature belongs to the block after it: `if ($a\n&& $b) {`
    /// begins where the `if` does, and its body takes the statement's level
    /// however far the condition wrapped.
    Rooted(Box<Doc>),
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
        /// Whether the break is the user wrapping a line, and so takes a
        /// continuation indent (docs/formatting.md INDENT-3).
        ///
        /// The newline after a block written across lines is not one: the
        /// block's own layout ended that line, at the level the statement
        /// started from. Indenting what follows would put it deeper than the
        /// `}` above it — and, when what follows opens a block of its own,
        /// deeper than that block's body, which is placed from the statement's
        /// level and knows nothing of the continuation.
        wraps: bool,
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
            anchored: broken,
            body: Box::new(body),
        }
    }

    /// A group the writer broke across lines without seeding a break.
    ///
    /// Flat, so nothing inside it takes a `Doc::Line`; anchored, because it has
    /// the several lines that alignment is a relation between.
    #[must_use]
    pub fn group_across_lines(body: Doc) -> Doc {
        Doc::Group {
            broken: false,
            anchored: true,
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

    #[must_use]
    pub fn statements(body: Doc) -> Doc {
        Doc::Statements(Box::new(body))
    }

    #[must_use]
    pub fn rooted(body: Doc) -> Doc {
        Doc::Rooted(Box::new(body))
    }

    pub fn hanging(columns: Option<usize>, body: Doc) -> Doc {
        Doc::Hanging {
            columns,
            body: Box::new(body),
        }
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
