//! The builtin function table (the parser contract).
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
//!
//! **This table is generated.** `scripts/generate-builtins` reads perl's own
//! prototypes through `prototype("CORE::$name")` and applies perl's own mapping
//! from toke.c: no arguments is FUNC0, exactly one is UNIOP — a named unary
//! operator, binding tighter than comparison — and anything else is LSTOP. The
//! handful of builtins perl gives no prototype for have bespoke syntax in its
//! grammar and are listed by hand in the script, with the reason.
//!
//! Generation runs there and the result is committed, rather than running at
//! build time: a Rust crate that cannot be built without a perl installation is
//! a worse trade than a table that has to be regenerated when perl gains a
//! builtin. Re-run the script and diff.
//!
//! Being written from memory instead cost `eval`: with the name missing, the
//! parser fell through to "unknown, expect an operator next", `eval <<EOT`
//! lexed as a left shift, and the heredoc body was promoted to code.

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
    /// A block, or one operand: `eval`. Unlike `BlockList` nothing follows the
    /// block, and unlike `NamedUnary` the brace opens a block rather than an
    /// anonymous hash — `eval {` is never a hash in perl either.
    BlockOrTerm,
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
        "grep" | "map" | "sort" => (Shape::BlockList, Expect::Term),

        // -- filehandle-taking list operators
        "print" | "printf" | "say" => (Shape::FilehandleList, Expect::Term),

        // -- named unary operators — exactly one argument, binding
        // tighter than comparison (perlop)
        "abs" | "alarm" | "caller" | "chdir" | "chr" | "chroot" | "close" | "closedir" | "cos"
        | "dbmclose" | "defined" | "delete" | "each" | "eof" | "exists" | "exit" | "exp" | "fc"
        | "fileno" | "getc" | "getgrgid" | "getgrnam" | "gethostbyname" | "getnetbyname"
        | "getpeername" | "getpgrp" | "getprotobyname" | "getprotobynumber" | "getpwnam"
        | "getpwuid" | "getsockname" | "glob" | "gmtime" | "hex" | "int" | "keys" | "lc"
        | "lcfirst" | "length" | "localtime" | "lock" | "log" | "lstat" | "oct" | "ord" | "pop"
        | "pos" | "prototype" | "quotemeta" | "rand" | "readdir" | "readline" | "readlink"
        | "readpipe" | "ref" | "reset" | "rewinddir" | "rmdir" | "scalar" | "sethostent"
        | "setnetent" | "setprotoent" | "setservent" | "shift" | "sin" | "sleep" | "sqrt"
        | "srand" | "stat" | "study" | "tell" | "telldir" | "tied" | "uc" | "ucfirst" | "umask"
        | "undef" | "untie" | "values" | "write" => (Shape::NamedUnary, Expect::Term),

        // -- no arguments at all
        "endgrent" | "endhostent" | "endnetent" | "endprotoent" | "endpwent" | "endservent"
        | "fork" | "getgrent" | "gethostent" | "getlogin" | "getnetent" | "getppid"
        | "getprotoent" | "getpwent" | "getservent" | "setgrent" | "setpwent" | "time"
        | "times" | "wait" | "wantarray" => (Shape::Nullary, Expect::Operator),

        // -- ordinary list operators
        "accept" | "atan2" | "bind" | "binmode" | "bless" | "chmod" | "chomp" | "chop"
        | "chown" | "connect" | "crypt" | "dbmopen" | "die" | "exec" | "fcntl" | "flock"
        | "formline" | "gethostbyaddr" | "getnetbyaddr" | "getpriority" | "getservbyname"
        | "getservbyport" | "getsockopt" | "index" | "ioctl" | "join" | "kill" | "link"
        | "listen" | "mkdir" | "msgctl" | "msgget" | "msgrcv" | "msgsnd" | "open" | "opendir"
        | "pack" | "pipe" | "push" | "read" | "recv" | "rename" | "reverse" | "rindex" | "seek"
        | "seekdir" | "select" | "semctl" | "semget" | "semop" | "send" | "setpgrp"
        | "setpriority" | "setsockopt" | "shmctl" | "shmget" | "shmread" | "shmwrite"
        | "shutdown" | "socket" | "socketpair" | "splice" | "split" | "sprintf" | "substr"
        | "symlink" | "syscall" | "sysopen" | "sysread" | "sysseek" | "system" | "syswrite"
        | "tie" | "truncate" | "unlink" | "unpack" | "unshift" | "utime" | "vec" | "waitpid"
        | "warn" => (Shape::List, Expect::Term),

        // -- a block, or one operand
        "eval" => (Shape::BlockOrTerm, Expect::Term),

        _ => return None,
    };

    Some(Builtin {
        shape,
        expect_after_name,
    })
}

/// Whether an unknown bareword should be treated as a list operator.
///
/// GUESS: it always should.
/// Evidence: none — with no symbol table there is nothing better to go on. The
/// constant exists so that the approximation is visible rather than implicit
/// (the parser contract).
/// Wrong: the name takes no arguments, and what was written as its argument
/// list is left to be read as something else.
pub(crate) const UNKNOWN_IS_LIST_OPERATOR: bool = true;
