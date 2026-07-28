//! The builtin function table (ADR 0007 §6).
//!
//! Two questions are answered here, and only here:
//!
//! 1. What shape are the arguments — a leading block, a filehandle, exactly one
//!    operand, or a list?
//! 2. What does the lexer expect straight after the name? `split` is followed by
//!    a pattern, so `split /,/, $x` must lex `/` as a match; `time` is followed
//!    by an operator.
//!
//! The old parser answered these from a 22-entry table plus string comparisons
//! against `"sort"` and `"//"` scattered across four files. Anything missing
//! from that table — `push`, `die`, `defined`, `open` — was guessed at.

use crate::lex::Expect;

/// How a builtin takes its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// No arguments: `time`, `wantarray`.
    Nullary,
    /// Exactly one operand, binding at named-unary precedence: `defined`, `ref`.
    NamedUnary,
    /// A list: `print`, `push`, `join`.
    List,
    /// An optional leading block, then a list: `map`, `grep`, `sort`.
    BlockList,
    /// An optional bareword or scalar filehandle, then a list: `print`, `say`.
    FilehandleList,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Builtin {
    pub shape: Shape,
    /// What the lexer should expect immediately after the name.
    pub expect_after_name: Expect,
}

/// Look up a builtin by name.
///
/// A miss means "user-defined, declaration unknown", which is treated as a list
/// operator. Perl itself cannot do better without running `BEGIN` blocks, and
/// saying so is part of the specification rather than a gap in it.
pub(crate) fn lookup(name: &str) -> Option<Builtin> {
    let (shape, expect_after_name) = match name {
        // -- block-taking list operators
        "map" | "grep" | "sort" => (Shape::BlockList, Expect::Term),

        // -- filehandle-taking list operators
        "print" | "printf" | "say" => (Shape::FilehandleList, Expect::Term),

        // -- named unary operators (perlop's "named unary operators")
        "defined" | "ref" | "scalar" | "lc" | "uc" | "lcfirst" | "ucfirst" | "length" | "chr"
        | "ord" | "hex" | "oct" | "int" | "abs" | "sqrt" | "log" | "exp" | "sin" | "cos"
        | "quotemeta" | "fc" | "rand" | "srand" | "exists" | "delete" | "each" | "keys"
        | "values" | "shift" | "pop" | "chomp" | "chop" | "chdir" | "rmdir" | "readlink"
        | "stat" | "lstat" | "undef" | "study" | "pos" | "alarm" | "sleep" | "caller" | "exit"
        | "umask" | "gmtime" | "localtime" | "lock" | "fileno" | "readdir" | "closedir"
        | "rewinddir" | "telldir" | "getpgrp" => (Shape::NamedUnary, Expect::Term),

        // -- nullary
        "time" | "times" | "wait" | "getppid" | "fork" => (Shape::Nullary, Expect::Operator),

        // -- ordinary list operators
        "push" | "unshift" | "splice" | "join" | "reverse" | "sprintf" | "die" | "warn"
        | "open" | "close" | "binmode" | "eof" | "read" | "sysread" | "syswrite" | "seek"
        | "sysseek" | "tell" | "truncate" | "unlink" | "rename" | "mkdir" | "opendir" | "chmod"
        | "chown" | "utime" | "link" | "symlink" | "system" | "exec" | "kill" | "bless" | "tie"
        | "untie" | "tied" | "select" | "pack" | "unpack" | "index" | "rindex" | "substr"
        | "atan2" | "crypt" | "formline" | "chroot" | "setpgrp" | "waitpid" | "wantarray" => {
            (Shape::List, Expect::Term)
        }

        // `split` is the reason the expect question exists at all: the first
        // argument is a pattern, so the `/` after it must lex as a match.
        "split" => (Shape::List, Expect::Term),

        _ => return None,
    };

    Some(Builtin {
        shape,
        expect_after_name,
    })
}

/// Whether an unknown bareword should be treated as a list operator.
///
/// It always should — with no symbol table there is nothing better to go on.
/// This is written down rather than left implicit so that the approximation is
/// visible (ADR 0007 §6).
pub(crate) const UNKNOWN_IS_LIST_OPERATOR: bool = true;
