//! What the checker reports (`docs/typecheck.md`, "Diagnostics").
//!
//! Every diagnostic has a stable code, a severity, a span and a message that
//! names both sides. The code is stable because it is what a `##
//! camello-disable:` comment and a config file name, and what a user greps a
//! CI log for; the severity is a property of the *kind* of contradiction, not
//! of the site, and the one place it varies is written down in
//! [`Diagnostic::downgraded`].

use std::fmt;

use rowan::TextRange;

/// How loudly a diagnostic is reported.
///
/// The split is the design's (`docs/typecheck.md`): an `error` is a
/// contradiction between two *declared* things, a `warning` has an inferred
/// thing on one side or rests on narrowing, and `info` is something a user
/// asked to be told.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "info" => Some(Severity::Info),
            "warning" | "warn" => Some(Severity::Warning),
            "error" => Some(Severity::Error),
            _ => None,
        }
    }

    /// One step quieter, for a diagnostic resting on a parser guess
    /// (`docs/architecture.md`, "Guesses"). `info` is the floor.
    #[must_use]
    pub const fn downgraded(self) -> Self {
        match self {
            Severity::Error => Severity::Warning,
            Severity::Warning | Severity::Info => Severity::Info,
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every code the checker can report.
///
/// Adding one means adding a fixture for it (`docs/typecheck.md`, "Testing");
/// [`Code::ALL`] is what the fixture test walks, so a code with no coverage is
/// a failing test rather than a gap nobody notices.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Code {
    /// A name used under `strict` that no declaration reaches.
    UndeclaredVariable,
    /// A lexical that is declared and never read.
    UnusedVariable,
    /// A lexical whose name an enclosing scope already binds.
    ShadowedVariable,
    /// A call whose argument count cannot satisfy the callee's parameter list.
    Arity,
    /// A value whose type contradicts the declared type of the slot it goes in.
    TypeMismatch,
    /// A key read off a `Dict` that has no such key, or passed to a constructor
    /// that declares no such attribute.
    UnknownKey,
    /// A method called on a class that declares no such method and has no
    /// unknown ancestor.
    UnknownMethod,
    /// A `Maybe[...]` dereferenced or called on without a narrowing check.
    /// A `warning` (`docs/types.md`, DIAG-14a): the most useful diagnostic and
    /// the most likely false positive, so the narrowing set that silences it
    /// is a list rather than a theorem.
    MaybeDeref,
    /// An annotation that does not parse. Reported because an annotation that
    /// is silently ignored is worse than none.
    BadAnnotation,
    /// A `return` whose shape contradicts the sub's `Returns:`.
    ReturnMismatch,
    /// A type or class name that nothing in the program declares.
    UnknownType,
    /// A public sub with no annotation, under `--strict-annotations`.
    MissingAnnotation,
    /// A required named argument the call does not pass.
    MissingArgument,
    /// A method call to a sub declared `()`, whose prototype — if that is what
    /// the `()` was — perl does not apply here.
    IgnoredPrototype,
    /// A parameter the body never reads. Its own code rather than
    /// [`Code::UnusedVariable`]'s, because a parameter list is a signature:
    /// the name goes on saying what the sub takes whether or not the body
    /// wants the value, and a project may reasonably want to be told about the
    /// one and not the other.
    UnusedParameter,
    /// A `return` inside a `do` block whose value is used (`docs/lint.md`,
    /// LINT-1). The codes from here down are the lint rules: what is written
    /// does not do what writing it meant, whatever the types say.
    ReturnInDoValue,
    /// `my ... if ...` (LINT-2).
    ConditionalDeclaration,
    /// `open` with two arguments (LINT-3).
    TwoArgOpen,
    /// A bareword filehandle (LINT-4).
    BarewordFilehandle,
    /// Code before `strict` is in effect (LINT-5).
    MissingStrict,
    /// Code before `warnings` is in effect (LINT-5).
    MissingWarnings,
    /// `return sort ...` (LINT-6).
    ReturnSort,
    /// `for $x (...)`, a loop variable not declared with it (LINT-7).
    NonLexicalLoopIterator,
    /// A `.pm` whose first package is not the one its path names (LINT-8).
    PackageFilenameMismatch,
    /// `;;` (LINT-9).
    DoubleSemicolon,
    /// A named `sub` inside a sub's body (LINT-10).
    NestedNamedSub,
    /// `my $x .= ...`, `local $x ||= ...` (LINT-11).
    AugmentedAssignmentInDeclaration,
    /// A `map` or `grep` block that modifies the array it is given (LINT-12).
    MutatingListFunction,
}

impl Code {
    pub const ALL: &'static [Code] = &[
        Code::UndeclaredVariable,
        Code::UnusedVariable,
        Code::ShadowedVariable,
        Code::Arity,
        Code::TypeMismatch,
        Code::UnknownKey,
        Code::UnknownMethod,
        Code::MaybeDeref,
        Code::BadAnnotation,
        Code::ReturnMismatch,
        Code::UnknownType,
        Code::MissingAnnotation,
        Code::MissingArgument,
        Code::UnusedParameter,
        Code::IgnoredPrototype,
        Code::ReturnInDoValue,
        Code::ConditionalDeclaration,
        Code::TwoArgOpen,
        Code::BarewordFilehandle,
        Code::MissingStrict,
        Code::MissingWarnings,
        Code::ReturnSort,
        Code::NonLexicalLoopIterator,
        Code::PackageFilenameMismatch,
        Code::DoubleSemicolon,
        Code::NestedNamedSub,
        Code::AugmentedAssignmentInDeclaration,
        Code::MutatingListFunction,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Code::UndeclaredVariable => "undeclared-variable",
            Code::UnusedVariable => "unused-variable",
            Code::ShadowedVariable => "shadowed-variable",
            Code::Arity => "arity",
            Code::TypeMismatch => "type-mismatch",
            Code::UnknownKey => "unknown-key",
            Code::UnknownMethod => "unknown-method",
            Code::MaybeDeref => "maybe-deref",
            Code::BadAnnotation => "bad-annotation",
            Code::ReturnMismatch => "return-mismatch",
            Code::UnknownType => "unknown-type",
            Code::MissingAnnotation => "missing-annotation",
            Code::MissingArgument => "missing-argument",
            Code::UnusedParameter => "unused-parameter",
            Code::IgnoredPrototype => "ignored-prototype",
            Code::ReturnInDoValue => "return-in-do-value",
            Code::ConditionalDeclaration => "conditional-declaration",
            Code::TwoArgOpen => "two-arg-open",
            Code::BarewordFilehandle => "bareword-filehandle",
            Code::MissingStrict => "missing-strict",
            Code::MissingWarnings => "missing-warnings",
            Code::ReturnSort => "return-sort",
            Code::NonLexicalLoopIterator => "non-lexical-loop-iterator",
            Code::PackageFilenameMismatch => "package-filename-mismatch",
            Code::DoubleSemicolon => "double-semicolon",
            Code::NestedNamedSub => "nested-named-sub",
            Code::AugmentedAssignmentInDeclaration => "augmented-assignment-in-declaration",
            Code::MutatingListFunction => "mutating-list-function",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Code::ALL.iter().copied().find(|code| code.as_str() == text)
    }

    /// The Perl::Critic policy this lint rule stands for, without
    /// `Perl::Critic::Policy::` (`docs/lint.md`, the table in §1), or `None`
    /// for a code that is not a lint rule.
    #[must_use]
    pub const fn critic_policy(self) -> Option<&'static str> {
        Some(match self {
            Code::ReturnInDoValue => "ControlStructures::ProhibitReturnInDoBlock",
            Code::ConditionalDeclaration => "Variables::ProhibitConditionalDeclarations",
            Code::TwoArgOpen => "InputOutput::ProhibitTwoArgOpen",
            Code::BarewordFilehandle => "InputOutput::ProhibitBarewordFileHandles",
            Code::MissingStrict => "TestingAndDebugging::RequireUseStrict",
            Code::MissingWarnings => "TestingAndDebugging::RequireUseWarnings",
            Code::ReturnSort => "Subroutines::ProhibitReturnSort",
            Code::NonLexicalLoopIterator => "Variables::RequireLexicalLoopIterators",
            Code::PackageFilenameMismatch => "Modules::RequireFilenameMatchesPackage",
            Code::DoubleSemicolon => "ValuesAndExpressions::ProhibitNullStatements",
            Code::NestedNamedSub => "Subroutines::ProhibitNestedSubs",
            Code::AugmentedAssignmentInDeclaration => {
                "Variables::ProhibitAugmentedAssignmentInDeclaration"
            }
            Code::MutatingListFunction => "ControlStructures::ProhibitMutatingListFunctions",
            _ => return None,
        })
    }

    /// Whether this is one of the lint rules of `docs/lint.md`.
    #[must_use]
    pub const fn is_lint(self) -> bool {
        self.critic_policy().is_some()
    }

    /// Whether this code rests on types, annotations or the classes a call
    /// resolves to, and is reported only under `--types` (`docs/types.md`,
    /// DIAG-0). What is left — the lint rules and the scope pass — reads
    /// nothing but the file and the names its imports bring in.
    #[must_use]
    pub const fn needs_types(self) -> bool {
        match self {
            Code::Arity
            | Code::TypeMismatch
            | Code::UnknownKey
            | Code::UnknownMethod
            | Code::MaybeDeref
            | Code::BadAnnotation
            | Code::ReturnMismatch
            | Code::UnknownType
            | Code::MissingAnnotation
            | Code::MissingArgument
            | Code::IgnoredPrototype => true,
            Code::UndeclaredVariable
            | Code::UnusedVariable
            | Code::ShadowedVariable
            | Code::UnusedParameter
            | Code::ReturnInDoValue
            | Code::ConditionalDeclaration
            | Code::TwoArgOpen
            | Code::BarewordFilehandle
            | Code::MissingStrict
            | Code::MissingWarnings
            | Code::ReturnSort
            | Code::NonLexicalLoopIterator
            | Code::PackageFilenameMismatch
            | Code::DoubleSemicolon
            | Code::NestedNamedSub
            | Code::AugmentedAssignmentInDeclaration
            | Code::MutatingListFunction => false,
        }
    }

    /// The severity this code is reported at when nothing downgrades it.
    #[must_use]
    pub const fn default_severity(self) -> Severity {
        match self {
            Code::UndeclaredVariable
            | Code::Arity
            | Code::TypeMismatch
            | Code::UnknownKey
            | Code::MissingArgument => Severity::Error,
            Code::ReturnMismatch
            // `$obj->m` reaching nothing is a statement about a closed world,
            // and the world is closed only where every module the class and
            // its ancestors `use` was read. The call site raises it to a
            // `warning` where it was (`docs/types.md`, DIAG-7a).
            | Code::MaybeDeref => Severity::Warning,
            // Every lint rule is a warning (`docs/lint.md`, LINT-POLICY-4):
            // printed by default, failing a run only where it asks.
            Code::ReturnInDoValue
            | Code::ConditionalDeclaration
            | Code::TwoArgOpen
            | Code::BarewordFilehandle
            | Code::MissingStrict
            | Code::MissingWarnings
            | Code::ReturnSort
            | Code::NonLexicalLoopIterator
            | Code::PackageFilenameMismatch
            | Code::DoubleSemicolon
            | Code::NestedNamedSub
            | Code::AugmentedAssignmentInDeclaration
            | Code::MutatingListFunction => Severity::Warning,
            Code::BadAnnotation
            | Code::UnknownType
            | Code::MissingAnnotation
            | Code::UnusedParameter
            | Code::IgnoredPrototype
            | Code::UnknownMethod
            // Shadowing is legal, deliberate more often than not, and a
            // matter of taste where it is not (`docs/types.md`, DIAG-3a).
            | Code::ShadowedVariable
            // Noisy out of proportion to what it catches: a name bound and
            // never read is usually deliberate — a destructuring that wanted
            // one of three slots, a `my` kept for a later edit — and where it
            // is not, it costs nothing (`docs/types.md`, DIAG-12).
            | Code::UnusedVariable => Severity::Info,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing the checker has to say about one place.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    /// The code this is *about*, as it is written: `$rows`, `$self->{cfg}`.
    ///
    /// Some diagnostics name their subject in the message and always did — a
    /// missing key, an unread variable. The ones about a *type* did not, and
    /// `` `Str|Undef` may be undefined here `` names nothing a reader can
    /// look for. It is also what `--group` groups by: twenty reports on one
    /// variable are one thing to fix.
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(with = "crate::serde_range")]
    pub range: TextRange,
    pub message: String,
}

impl Diagnostic {
    #[must_use]
    pub fn new(code: Code, range: TextRange, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: code.default_severity(),
            subject: None,
            range,
            message: message.into(),
        }
    }

    /// Name the code this is about, for the message and for `--group`.
    #[must_use]
    pub fn about(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// One step quieter, because the reading it rests on is a parser guess.
    #[must_use]
    pub fn downgraded(mut self) -> Self {
        self.severity = self.severity.downgraded();
        self
    }

    /// Report at exactly this severity.
    #[must_use]
    pub fn at(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }
}

/// A line and column, one-based, for `path:line:col:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// Where an offset falls in a source, counting columns in characters.
///
/// Built once per file and searched, rather than counting newlines per
/// diagnostic: a file with a thousand diagnostics would otherwise read itself
/// a thousand times.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        LineIndex { starts }
    }

    #[must_use]
    pub fn position(&self, source: &str, offset: usize) -> Position {
        let offset = offset.min(source.len());
        let line = match self.starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        let start = self.starts[line];
        let column = source
            .get(start..offset)
            .map_or(0, |text| text.chars().count());
        Position {
            line: line + 1,
            column: column + 1,
        }
    }
}
