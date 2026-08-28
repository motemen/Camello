//! Lexical scopes, and the diagnostics that need nothing but them
//! (`docs/typecheck.md`, "Scopes").
//!
//! `my` is a declaration and `use strict` makes an undeclared name an error, so
//! unlike everything the type lattice says, this is *sound within a file*: a
//! name camello cannot see a declaration for is one perl cannot see a
//! declaration for either. Three things stop that soundness from turning into
//! noise, and each of them is a place a false positive would have come from:
//!
//! - **`strict` is read, not assumed.** Without it an undeclared name is a
//!   package variable and a legal program, so there is nothing to report. See
//!   [`strict_in_effect`].
//! - **An element names its container.** `$h{k}` reads `%h` and `$a[0]` reads
//!   `@a`. Reading them as `$h` and `$a` would report every hash in the corpus.
//! - **Strings are scanned** ([`crate::interp`]), so `"hi $who"` is a use and
//!   `$who` is not reported unused.

use std::collections::HashMap;

use camello_syntax::ast::{
    self, Args, AstNode, DeclKeyword, Literal, Sigil, SubDef, SubName, VarDecl, Variable,
};
use camello_syntax::lang::{NodeExt, NodeKind, SyntaxNode, SyntaxToken, TokenExt, TokenKind};
use rowan::{TextRange, TextSize};

use crate::diag::{Code, Diagnostic};
use crate::interp;

/// Names that are always in scope, whatever the file declares.
///
/// perl's own, not a convenience list: `$_` and `@_`, the two `sort` gives a
/// comparison block, and the globals every program has. A name here is never
/// reported, whatever its sigil — `%ENV`, `$ENV{PATH}` and `@ENV{...}` are one
/// variable read three ways.
const ALWAYS_IN_SCOPE: &[&str] = &[
    "_", "a", "b", "ARGV", "ARGVOUT", "ENV", "INC", "SIG", "STDIN", "STDOUT", "STDERR", "DATA",
    "AUTOLOAD", "ISA", "0",
];

/// Modules whose import turns `strict` on, so that a file saying `use Moose`
/// is checked the way perl checks it.
///
/// Not exhaustive and cannot be: a module that turns strict on is doing it in
/// its own `import`, which is code camello does not run (`docs/typecheck.md`,
/// non-goals). Missing one makes the checker quieter, which is the direction
/// to be wrong in.
const STRICT_BY_IMPORT: &[&str] = &[
    "strict",
    "strictures",
    "Modern::Perl",
    "common::sense",
    "Moose",
    "Moose::Role",
    "Moose::Exporter",
    "Moo",
    "Moo::Role",
    "Mouse",
    "Mouse::Role",
    "Mojo::Base",
    "Mojolicious::Lite",
    "Dancer2",
    "Object::Pad",
    "Test::Class::Moose",
    "Class::Accessor::Typed",
];

/// The variables `use English` binds, read off `English.pm`'s own glob
/// assignments.
///
/// A module that exports a variable is running code camello does not run
/// (`docs/typecheck.md`, non-goals), so in general the export list is not
/// visible until the dependency resolver of milestone 4. These two are in core
/// and are the ones the corpus actually reaches for, so they are a table.
const ENGLISH_NAMES: &[&str] = &[
    "ACCUMULATOR",
    "ARG",
    "ARRAY_BASE",
    "BASETIME",
    "CHILD_ERROR",
    "COMPILING",
    "DEBUGGING",
    "EFFECTIVE_GROUP_ID",
    "EFFECTIVE_USER_ID",
    "EGID",
    "ERRNO",
    "EUID",
    "EVAL_ERROR",
    "EXCEPTIONS_BEING_CAUGHT",
    "EXECUTABLE_NAME",
    "EXTENDED_OS_ERROR",
    "FORMAT_FORMFEED",
    "FORMAT_LINE_BREAK_CHARACTERS",
    "FORMAT_LINES_LEFT",
    "FORMAT_LINES_PER_PAGE",
    "FORMAT_NAME",
    "FORMAT_PAGE_NUMBER",
    "FORMAT_TOP_NAME",
    "GID",
    "INPLACE_EDIT",
    "INPUT_LINE_NUMBER",
    "INPUT_RECORD_SEPARATOR",
    "LAST_MATCH_END",
    "LAST_MATCH_START",
    "LAST_PAREN_MATCH",
    "LAST_REGEXP_CODE_RESULT",
    "LAST_SUBMATCH_RESULT",
    "LIST_SEPARATOR",
    "MATCH",
    "NR",
    "OFMT",
    "OFS",
    "OLD_PERL_VERSION",
    "ORS",
    "OS_ERROR",
    "OSNAME",
    "OUTPUT_AUTOFLUSH",
    "OUTPUT_FIELD_SEPARATOR",
    "OUTPUT_RECORD_SEPARATOR",
    "PERL_VERSION",
    "PERLDB",
    "PID",
    "POSTMATCH",
    "PREMATCH",
    "PROCESS_ID",
    "PROGRAM_NAME",
    "REAL_GROUP_ID",
    "REAL_USER_ID",
    "RS",
    "SUBSCRIPT_SEPARATOR",
    "SUBSEP",
    "SYSTEM_FD_MAX",
    "UID",
    "WARNING",
];

/// Modules in core that export a variable rather than a sub, and what they
/// export. See [`ENGLISH_NAMES`] for why this is a table.
fn exported_variables(module: &str) -> &'static [&'static str] {
    match module {
        "English" => ENGLISH_NAMES,
        "Config" => &["Config"],
        _ => &[],
    }
}

/// How a name came to be bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    My,
    State,
    Our,
    /// `use vars qw($x)`.
    Vars,
    /// What a sub takes: a signature parameter, an `args` item, or a name in a
    /// `my (...) = @_` unpacking.
    Param,
    /// `catch ($e)` — bound by the construct whether the body wants it or not.
    Caught,
    /// The `class` feature's `field`: a per-instance slot every `method` of
    /// the class sees.
    Field,
    /// `$_`, `@_`, `%ENV` — bound by perl, not by the file.
    Implicit,
}

impl BindingKind {
    /// What never reading it is reported as, or `None` when it is not worth
    /// saying.
    ///
    /// `our` names a package variable that another file may be the reader of,
    /// and a `catch` variable is bound by the construct rather than by a
    /// choice to hold a value, so neither is unused in the sense meant here. A
    /// parameter is its own code: it is declared by the caller's shape, and it
    /// goes on saying what the sub takes whether or not the body reads it
    /// (`docs/types.md`, DIAG-12).
    const fn unused_code(self) -> Option<Code> {
        match self {
            BindingKind::My | BindingKind::State | BindingKind::Field => Some(Code::UnusedVariable),
            BindingKind::Param => Some(Code::UnusedParameter),
            _ => None,
        }
    }

    const fn reports_shadowing(self) -> bool {
        matches!(self, BindingKind::My | BindingKind::State)
    }
}

#[derive(Debug, Clone)]
struct Binding {
    sigil: Sigil,
    name: String,
    range: TextRange,
    kind: BindingKind,
    used: bool,
    /// A value held for its destructor (`docs/types.md`, DIAG-12d): bound on
    /// purpose, never read on purpose.
    guard: bool,
}

#[derive(Debug, Default)]
struct Scope {
    /// Indices into [`Pass::bindings`], newest last so a redeclaration in one
    /// scope resolves to the newer one.
    bindings: Vec<usize>,
    names: HashMap<(Sigil, String), usize>,
}

/// The result of walking one file's scopes.
pub struct ScopeReport {
    pub diagnostics: Vec<Diagnostic>,
}

/// Walk a file and report what its scopes say.
///
/// `guards` names the classes a project holds values of for their destructors,
/// beyond the ones [`GUARD_NAMES`] knows.
#[must_use]
pub fn analyse(root: &SyntaxNode, source: &str, guards: &[String]) -> ScopeReport {
    let mut pass = Pass::new(source, guards, StrictRegions::of(root));
    pass.run(root);
    ScopeReport {
        diagnostics: pass.diagnostics,
    }
}

/// Where in the file an undeclared name becomes an error.
///
/// Two departures from the design document, both of them the quiet reading and
/// both of them what perl does:
///
/// - `strict` is *read*, not assumed. A file that never asks for it is a file
///   where an undeclared name is a package variable and a legal program, so
///   there is nothing to report.
/// - it is *positional*. `use strict` is a lexical pragma, and code above it
///   is not under it: `WWW::RobotRules` sets `$VERSION` on line 3 and says
///   `use strict` on line 6, which is legal and was reported until this
///   counted offsets rather than files.
///
/// `no strict` (bare, or naming `vars`) turns it off again from where it
/// appears. Taking that to the end of the file rather than to the end of its
/// block is the quiet reading, and the one a file that says it once means.
#[derive(Debug, Default)]
pub struct StrictRegions {
    /// `(offset, on)`, sorted, so a use finds the last event above it.
    events: Vec<(TextSize, bool)>,
}

impl StrictRegions {
    #[must_use]
    pub fn of(root: &SyntaxNode) -> Self {
        let mut events = Vec::new();
        for node in root.descendants() {
            match node.node_kind() {
                NodeKind::USE_STMT => {
                    let statement = ast::UseStmt::cast(node.clone()).expect("kind checked");
                    let on = match statement.module() {
                        Some(module) => STRICT_BY_IMPORT.contains(&module.as_str()),
                        None => version_enables_strict(&node),
                    };
                    if on {
                        events.push((node.text_range().end(), true));
                    }
                }
                NodeKind::NO_STMT => {
                    let statement = ast::NoStmt::cast(node.clone()).expect("kind checked");
                    if statement.module().as_deref() == Some("strict") && disables_vars(&statement)
                    {
                        events.push((node.text_range().start(), false));
                    }
                }
                _ => {}
            }
        }
        events.sort_by_key(|(offset, _)| *offset);
        StrictRegions { events }
    }

    #[must_use]
    pub fn at(&self, offset: TextSize) -> bool {
        self.events
            .iter()
            .take_while(|(at, _)| *at <= offset)
            .last()
            .is_some_and(|(_, on)| *on)
    }

    #[must_use]
    pub fn anywhere(&self) -> bool {
        self.events.iter().any(|(_, on)| *on)
    }
}

/// `use v5.12` and up imply `use strict` (perl 5.11.0 onwards).
fn version_enables_strict(node: &SyntaxNode) -> bool {
    let Some(token) = ast::tokens(node).find(|token| token.token_kind() == TokenKind::VERSION)
    else {
        return false;
    };
    let text = token.text().trim_start_matches('v');
    let mut parts = text.split('.');
    let major: u32 = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    // `use 5.012` and `use v5.12` are the same request written two ways.
    let minor = if minor >= 100 { minor / 10 } else { minor };
    major > 5 || (major == 5 && minor >= 11)
}

/// Whether a `no strict ...` turns off the part this pass depends on.
fn disables_vars(statement: &ast::NoStmt) -> bool {
    let Some(arguments) = statement.arguments() else {
        return true;
    };
    let text = arguments.text().to_string();
    text.contains("vars") || text.trim().is_empty()
}

struct Pass<'a> {
    source: &'a str,
    /// Guard classes beyond [`GUARD_NAMES`], from `camello.toml`.
    guards: &'a [String],
    bindings: Vec<Binding>,
    scopes: Vec<Scope>,
    diagnostics: Vec<Diagnostic>,
    strict: StrictRegions,
    /// Whether the heredoc body starting here interpolates, keyed by the body
    /// token's start. Built once: the marker that says so is on another line.
    heredocs: HashMap<TextSize, bool>,
}

impl<'a> Pass<'a> {
    fn new(source: &'a str, guards: &'a [String], strict: StrictRegions) -> Self {
        Pass {
            source,
            guards,
            bindings: Vec::new(),
            scopes: Vec::new(),
            diagnostics: Vec::new(),
            strict,
            heredocs: HashMap::new(),
        }
    }

    fn run(&mut self, root: &SyntaxNode) {
        self.heredocs = heredoc_interpolation(root);
        self.push_scope();
        for name in ALWAYS_IN_SCOPE {
            for sigil in [
                Sigil::Scalar,
                Sigil::Array,
                Sigil::Hash,
                Sigil::Code,
                Sigil::Typeglob,
            ] {
                self.declare_raw(
                    sigil,
                    (*name).to_string(),
                    TextRange::default(),
                    BindingKind::Implicit,
                );
            }
        }
        self.walk_children(root);
        self.pop_scope();
        self.diagnostics.sort_by_key(|d| (d.range.start(), d.code));
    }

    // ----- scopes -----

    fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        for index in scope.bindings {
            let binding = &self.bindings[index];
            if binding.used || binding.guard || !reports_unused_name(&binding.name) {
                continue;
            }
            let Some(code) = binding.kind.unused_code() else {
                continue;
            };
            let display = format!("{}{}", binding.sigil.as_str(), binding.name);
            let message = if code == Code::UnusedParameter {
                format!("`{display}` is taken as a parameter and never read")
            } else {
                format!("`{display}` is declared and never read")
            };
            self.diagnostics
                .push(Diagnostic::new(code, binding.range, message));
        }
    }

    fn declare_raw(
        &mut self,
        sigil: Sigil,
        name: String,
        range: TextRange,
        kind: BindingKind,
    ) -> usize {
        let index = self.bindings.len();
        self.bindings.push(Binding {
            sigil,
            name: name.clone(),
            range,
            kind,
            used: false,
            guard: false,
        });
        let scope = self.scopes.last_mut().expect("a scope is always open");
        scope.bindings.push(index);
        scope.names.insert((sigil, name), index);
        index
    }

    fn declare(&mut self, variable: &Variable, kind: BindingKind) -> Option<usize> {
        let name = variable.name()?;
        if kind.reports_shadowing() && reports_unused_name(&name) {
            if let Some(outer) = self.lookup_outer(variable.sigil(), &name) {
                let display = format!("{}{name}", variable.sigil().as_str());
                let outer_line = self.line_of(self.bindings[outer].range.start());
                self.diagnostics.push(Diagnostic::new(
                    Code::ShadowedVariable,
                    variable.range(),
                    format!("`{display}` shadows the one declared on line {outer_line}"),
                ));
            }
        }
        Some(self.declare_raw(variable.sigil(), name, variable.range(), kind))
    }

    /// The binding this name would have resolved to before the declaration,
    /// looking past the innermost scope only when the innermost does not bind
    /// it — a second `my $x` in one block is a redeclaration, not shadowing.
    fn lookup_outer(&self, sigil: Sigil, name: &str) -> Option<usize> {
        let key = (sigil, name.to_string());
        for scope in self.scopes.iter().rev() {
            if let Some(index) = scope.names.get(&key) {
                return (self.bindings[*index].kind != BindingKind::Implicit).then_some(*index);
            }
        }
        None
    }

    fn resolve(&mut self, sigil: Sigil, name: &str) -> bool {
        let key = (sigil, name.to_string());
        for scope in self.scopes.iter().rev() {
            if let Some(index) = scope.names.get(&key) {
                self.bindings[*index].used = true;
                return true;
            }
        }
        false
    }

    fn line_of(&self, offset: TextSize) -> usize {
        self.source[..usize::from(offset).min(self.source.len())]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1
    }

    // ----- uses -----

    fn use_name(&mut self, sigil: Sigil, name: &str, range: TextRange) {
        // A sub, a glob, and a package-qualified name are all things `my` never
        // declared, so none of them is this diagnostic's business.
        if matches!(sigil, Sigil::Code | Sigil::Typeglob)
            || name.contains("::")
            || name.chars().all(|ch| ch.is_ascii_digit())
            || name.starts_with('^')
            || ALWAYS_IN_SCOPE.contains(&name)
        {
            let _ = self.resolve(sigil, name);
            return;
        }
        if self.resolve(sigil, name) || !self.strict.at(range.start()) {
            return;
        }
        let display = format!("{}{name}", sigil.as_str());
        self.diagnostics.push(Diagnostic::new(
            Code::UndeclaredVariable,
            range,
            format!("`{display}` is not declared in this scope"),
        ));
    }

    fn use_variable(&mut self, variable: &Variable, sigil: Sigil) {
        let Some(name) = variable.name() else {
            return;
        };
        self.use_name(sigil, &name, variable.range());
    }

    // ----- the walk -----

    fn walk_children(&mut self, node: &SyntaxNode) {
        for element in node.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Node(child) => self.walk(&child),
                rowan::NodeOrToken::Token(token) => self.scan_token(&token),
            }
        }
    }

    fn walk_scoped(&mut self, node: &SyntaxNode) {
        self.push_scope();
        self.walk_children(node);
        self.pop_scope();
    }

    fn walk(&mut self, node: &SyntaxNode) {
        match node.node_kind() {
            NodeKind::BLOCK
            | NodeKind::IF_STMT
            | NodeKind::LOOP_STMT
            | NodeKind::TRY_STMT
            | NodeKind::GIVEN_STMT
            | NodeKind::CATCH_CLAUSE
            | NodeKind::WHEN_CLAUSE
            | NodeKind::DO_BLOCK_EXPR
            | NodeKind::PHASE_BLOCK => self.walk_scoped(node),

            NodeKind::SUB_DEF => self.walk_sub(node),
            NodeKind::ANON_SUB_EXPR => self.walk_anon_sub(node),

            NodeKind::VAR_DECL => self.walk_var_decl(node),
            NodeKind::CATCH_PARAM => {
                for variable in node.children().filter_map(Variable::cast) {
                    self.declare(&variable, BindingKind::Caught);
                }
            }
            NodeKind::SIGNATURE_PARAM => {
                // The default is evaluated in the caller's scope conceptually,
                // but it may refer to an earlier parameter; either way it is
                // read before this one is bound.
                for child in node.children() {
                    if child.node_kind() == NodeKind::SIGNATURE_DEFAULT {
                        self.walk(&child);
                    }
                }
                if let Some(variable) =
                    ast::SignatureParam::cast(node.clone()).and_then(|param| param.variable())
                {
                    self.declare(&variable, BindingKind::Param);
                }
            }

            // `my $x = $x` reads the outer `$x`: a declaration takes effect at
            // the end of the statement it is in, so the value comes first.
            NodeKind::ASSIGN_EXPR => {
                let children: Vec<_> = node.children().collect();
                for child in children.iter().skip(1) {
                    self.walk(child);
                }
                if let Some(first) = children.first() {
                    self.walk(first);
                }
                for token in ast::tokens(node) {
                    self.scan_token(&token);
                }
            }

            NodeKind::HASH_SUBSCRIPT_EXPR => self.walk_subscript(node, Sigil::Hash),
            NodeKind::ARRAY_SUBSCRIPT_EXPR => self.walk_subscript(node, Sigil::Array),

            NodeKind::BLOCK_DEREF_EXPR => self.walk_block_deref(node),

            NodeKind::SCALAR_VAR
            | NodeKind::ARRAY_VAR
            | NodeKind::HASH_VAR
            | NodeKind::CODE_VAR
            | NodeKind::TYPEGLOB_VAR
            | NodeKind::ARRAY_LAST_INDEX => {
                let variable = Variable::cast(node.clone()).expect("kind checked");
                let sigil = variable.sigil();
                self.use_variable(&variable, sigil);
                self.walk_children(node);
            }

            NodeKind::USE_STMT => self.walk_use(node),

            // A name is not a variable, and its tokens hold nothing to scan.
            NodeKind::SUB_NAME | NodeKind::LABEL | NodeKind::ATTR => {}

            _ => self.walk_children(node),
        }
    }

    /// `$h{k}` is an element of `%h`; only an arrow keeps the scalar.
    fn walk_subscript(&mut self, node: &SyntaxNode, container: Sigil) {
        let arrow = ast::tokens(node).any(|token| token.token_kind() == TokenKind::ARROW);
        let mut children = node.children();
        if let Some(base) = children.next() {
            match Variable::cast(base.clone()) {
                Some(variable) if !arrow => {
                    let sigil = match variable.sigil() {
                        // `&h{...}` and `*h{...}` are neither of these.
                        Sigil::Code | Sigil::Typeglob => variable.sigil(),
                        _ => container,
                    };
                    self.use_variable(&variable, sigil);
                    self.walk_children(&base);
                }
                _ => self.walk(&base),
            }
        }
        for child in children {
            self.walk(&child);
        }
        for token in ast::tokens(node) {
            self.scan_token(&token);
        }
    }

    /// `${name}` is `$name`; `${ EXPR }` is a dereference of what `EXPR` says.
    fn walk_block_deref(&mut self, node: &SyntaxNode) {
        let sigil = ast::tokens(node).find_map(|token| match token.token_kind() {
            TokenKind::SCALAR_SIGIL => Some(Sigil::Scalar),
            TokenKind::ARRAY_SIGIL => Some(Sigil::Array),
            TokenKind::HASH_SIGIL => Some(Sigil::Hash),
            TokenKind::CODE_SIGIL => Some(Sigil::Code),
            TokenKind::TYPEGLOB_SIGIL => Some(Sigil::Typeglob),
            _ => None,
        });
        let mut children = node.children();
        let inner = children.next();
        match (sigil, inner) {
            (Some(sigil), Some(inner)) if inner.node_kind() == NodeKind::SUB_NAME => {
                let name = SubName::cast(inner).expect("kind checked").text();
                self.use_name(sigil, &name, node.text_range());
            }
            (_, Some(inner)) => self.walk(&inner),
            (_, None) => {}
        }
        for child in children {
            self.walk(&child);
        }
    }

    fn walk_var_decl(&mut self, node: &SyntaxNode) {
        let declaration = VarDecl::cast(node.clone()).expect("kind checked");
        let kind = match declaration.keyword() {
            Some(DeclKeyword::My) => BindingKind::My,
            Some(DeclKeyword::State) => BindingKind::State,
            Some(DeclKeyword::Our) => BindingKind::Our,
            Some(DeclKeyword::Field) => BindingKind::Field,
            // `local` does not declare (`docs/typecheck.md`, "Scopes"). It
            // does not use, either: `local $x` names a package variable, and
            // whether that variable is one `strict` would have complained
            // about is a question about the `our` somewhere else, not here.
            Some(DeclKeyword::Local) | None => {
                for child in node.children() {
                    if child.node_kind() != NodeKind::DECL_TARGET {
                        self.walk(&child);
                    } else {
                        // Still read the keys: `local $SIG{$name}` uses `$name`.
                        for inner in child.descendants() {
                            if inner.node_kind() == NodeKind::SUBSCRIPT {
                                self.walk_children(&inner);
                            }
                        }
                    }
                }
                return;
            }
        };
        // A `my` that unpacks `@_`, or that stands inside an `args` list, is
        // a parameter however it is written: it says what the sub takes, and
        // a body that does not read it is a different thing to be told about
        // than a value nobody wanted.
        let kind = if kind == BindingKind::My && declares_parameters(node) {
            BindingKind::Param
        } else {
            kind
        };
        // An attribute on a `field` hands the name to something outside the
        // class body — `:param` to the constructor, `:reader` to a generated
        // accessor — so a body that never reads it is not the mistake
        // `unused-variable` is about (`docs/types.md`, DIAG-2a).
        let unread_by_design = (kind == BindingKind::Field && has_attribute(node))
            || (kind != BindingKind::Param && holds_a_guard(node, self.guards));
        for variable in declaration.targets() {
            if let Some(index) = self.declare(&variable, kind) {
                self.bindings[index].guard = unread_by_design;
            }
        }
    }

    fn walk_sub(&mut self, node: &SyntaxNode) {
        let definition = SubDef::cast(node.clone()).expect("kind checked");
        self.push_scope();
        // `@_` and `$_` are bound inside every sub whatever its shape.
        self.declare_raw(
            Sigil::Array,
            "_".to_string(),
            TextRange::default(),
            BindingKind::Implicit,
        );
        if let Some(signature) = definition.signature() {
            self.walk(signature.syntax());
        }
        if let Some(body) = definition.body() {
            // The body's own BLOCK scope would hide the parameters from an
            // `unused` report that belongs to the sub, so the block is walked
            // without a second scope.
            self.walk_children(body.syntax());
        }
        self.pop_scope();
    }

    fn walk_anon_sub(&mut self, node: &SyntaxNode) {
        let definition = ast::AnonSubExpr::cast(node.clone()).expect("kind checked");
        self.push_scope();
        self.declare_raw(
            Sigil::Array,
            "_".to_string(),
            TextRange::default(),
            BindingKind::Implicit,
        );
        if let Some(signature) = definition.signature() {
            self.walk(signature.syntax());
        }
        if let Some(body) = definition.body() {
            self.walk_children(body.syntax());
        }
        self.pop_scope();
    }

    /// `use vars qw($x @y)` and an import list naming variables both declare;
    /// every other `use` is walked for the uses its arguments hold.
    fn walk_use(&mut self, node: &SyntaxNode) {
        let statement = ast::UseStmt::cast(node.clone()).expect("kind checked");
        let Some(module) = statement.module() else {
            return;
        };

        // `use English` binds a long name to each punctuation variable.
        for name in exported_variables(&module) {
            for sigil in [Sigil::Scalar, Sigil::Array, Sigil::Hash] {
                self.declare_file_scope(
                    sigil,
                    (*name).to_string(),
                    node.text_range(),
                    BindingKind::Vars,
                );
            }
        }

        if let Some(arguments) = statement.arguments() {
            // A name with a sigil in an import list is a variable this file
            // now has: `use POSIX qw($errno)`, and `use vars` itself, which
            // the pragma's own documentation calls package-wide rather than
            // lexical — `Time::Zone` declares `%zoneOff` inside a block and
            // reads it in a sub two hundred lines later.
            for name in declared_names(&arguments) {
                let (sigil, bare) = split_sigil(&name);
                if let Some(sigil) = sigil {
                    self.declare_file_scope(sigil, bare, node.text_range(), BindingKind::Vars);
                }
            }
        }
        if module != "vars" {
            self.walk_children(node);
        }
    }

    /// Declare at the outermost scope, which is what a package-wide
    /// declaration means to a file.
    fn declare_file_scope(
        &mut self,
        sigil: Sigil,
        name: String,
        range: TextRange,
        kind: BindingKind,
    ) {
        let index = self.bindings.len();
        self.bindings.push(Binding {
            sigil,
            name: name.clone(),
            range,
            kind,
            used: false,
            guard: false,
        });
        let scope = self.scopes.first_mut().expect("the file scope is open");
        scope.bindings.push(index);
        scope.names.insert((sigil, name), index);
    }

    // ----- quoted constructs -----

    fn scan_token(&mut self, token: &SyntaxToken) {
        let mut extended = false;
        let text = match token.token_kind() {
            // A double-quoted string interpolates; a single-quoted one is one
            // token of the same kind and does not.
            TokenKind::STRING => {
                if token.text().starts_with('"') {
                    token.text()
                } else {
                    return;
                }
            }
            // The replacement of an `s///e` is Perl code, not a string: the
            // `my $indent` in it is a declaration this pass never saw, so
            // scanning it reported the use two lines later as undeclared.
            TokenKind::INTERPOLATED_STRING => {
                if evaluated_replacement(token) {
                    return;
                }
                token.text()
            }
            TokenKind::REGEX_PATTERN => {
                if !delimiter_interpolates(token) {
                    return;
                }
                extended = has_flag(token, 'x');
                token.text()
            }
            TokenKind::HEREDOC_CONTENT
                if self
                    .heredocs
                    .get(&token.text_range().start())
                    .copied()
                    .unwrap_or(true) =>
            {
                token.text()
            }
            _ => return,
        };
        let start = usize::from(token.text_range().start());
        let found = if extended {
            interp::scan_extended(text)
        } else {
            interp::scan(text)
        };
        for found in found {
            let range = TextRange::new(
                TextSize::from((start + found.offset) as u32),
                TextSize::from((start + found.offset + found.len) as u32),
            );
            self.use_name(found.sigil, &found.name, range);
        }
    }
}

/// Classes and functions that hand back a value held for its destructor.
///
/// `my $guard = Scope::Guard->new(sub { ... })` binds a name it will never
/// read: the value's whole job is to go out of scope. Recognition is by what
/// produced it rather than by what the name is, so a project that calls it
/// `$_g` gets the same answer as one that calls it `$guard`; a project with a
/// guard class of its own names it in `camello.toml`.
pub const GUARD_NAMES: &[&str] = &[
    "Scope::Guard",
    "Guard",
    "guard",
    "scope_guard",
    "SCOPE_GUARD",
];

/// The value a `my` was given, or `None` when it was given none.
fn initialiser(node: &SyntaxNode) -> Option<SyntaxNode> {
    let assign = ast::Assign::cast(node.parent()?)?;
    let target = assign.target()?;
    if target.text_range() != node.text_range() {
        return None;
    }
    assign.value()
}

/// Whether this `my` is the sub's parameter list rather than a local.
///
/// The two shapes `docs/types.md` calls unpacking (ANNOT-6) — `my (...) = @_` and `my $x
/// = shift` — and Smart::Args' `args my $x => T`, which is a `my` written
/// inside a call.
fn declares_parameters(node: &SyntaxNode) -> bool {
    for ancestor in node.ancestors().skip(1) {
        match ancestor.node_kind() {
            NodeKind::EXPR_STMT | NodeKind::VAR_DECL_STMT | NodeKind::BLOCK => break,
            _ => {}
        }
        let named_args = ast::Call::cast(ancestor)
            .and_then(|call| call.callee_name())
            .is_some_and(|name| name == "args" || name == "args_pos");
        if named_args {
            return true;
        }
    }
    let Some(value) = initialiser(node) else {
        return false;
    };
    if value.node_kind() == NodeKind::ARRAY_VAR {
        return names_the_argument_array(&value);
    }
    let Some(call) = ast::Call::cast(value) else {
        return false;
    };
    if call.callee_name().as_deref() != Some("shift") {
        return false;
    }
    // `shift @list` is a list operation; `shift` and `shift @_` are the
    // parameter list, one name at a time.
    //
    // A one-argument list call holds its argument as a child of its own, with
    // no `LIST_EXPR` around it, so `Call::args` is empty there and the
    // children are what has to be read.
    let operands: Vec<SyntaxNode> = match call.args().as_slice() {
        [] => call
            .syntax()
            .children()
            .filter(|child| !matches!(child.node_kind(), NodeKind::SUB_NAME | NodeKind::ARG_LIST))
            .collect(),
        found => found.to_vec(),
    };
    match operands.as_slice() {
        [] => true,
        [only] => names_the_argument_array(only),
        _ => false,
    }
}

/// Whether this node is `@_` itself.
fn names_the_argument_array(node: &SyntaxNode) -> bool {
    node.node_kind() == NodeKind::ARRAY_VAR
        && Variable::cast(node.clone())
            .and_then(|variable| variable.name())
            .as_deref()
            == Some("_")
}

/// Whether this `my` holds a value for its destructor ([`GUARD_NAMES`]).
/// Whether a declaration carries an attribute — `field $x :param`.
fn has_attribute(node: &SyntaxNode) -> bool {
    node.children()
        .any(|child| child.node_kind() == NodeKind::ATTR)
}

fn holds_a_guard(node: &SyntaxNode, extra: &[String]) -> bool {
    let Some(value) = initialiser(node) else {
        return false;
    };
    let is_guard =
        |name: &str| GUARD_NAMES.contains(&name) || extra.iter().any(|guard| guard == name);
    if value.node_kind() == NodeKind::METHOD_CALL_EXPR {
        // `Scope::Guard->new(...)`, and any other constructor on the class.
        return ast::MethodCallExpr::cast(value)
            .and_then(|call| call.invocant())
            .and_then(ast::Call::cast)
            .and_then(|invocant| invocant.callee_name())
            .is_some_and(|name| is_guard(&name));
    }
    let Some(name) = ast::Call::cast(value).and_then(|call| call.callee_name()) else {
        return false;
    };
    // `guard { ... }`, `Guard::guard { ... }`, `Scope::Guard::guard(...)`.
    is_guard(&name)
        || name
            .rsplit_once("::")
            .is_some_and(|(package, _)| is_guard(package))
}

/// Whether never reading this name is worth a word.
///
/// A leading underscore is the language-wide way of saying "bound on purpose,
/// not read", and `$self` / `$class` are unpacked because the shape of `@_`
/// says to, not because the body was going to use them.
fn reports_unused_name(name: &str) -> bool {
    !name.starts_with('_') && name != "self" && name != "class"
}

/// `$x` → `(Some(Scalar), "x")`.
fn split_sigil(text: &str) -> (Option<Sigil>, String) {
    let mut chars = text.chars();
    let sigil = match chars.next() {
        Some('$') => Some(Sigil::Scalar),
        Some('@') => Some(Sigil::Array),
        Some('%') => Some(Sigil::Hash),
        Some('&') => Some(Sigil::Code),
        Some('*') => Some(Sigil::Typeglob),
        _ => None,
    };
    (sigil, chars.as_str().to_string())
}

/// The names in a `use vars` argument list, however it is written.
fn declared_names(arguments: &SyntaxNode) -> Vec<String> {
    let mut acc = Vec::new();
    for element in Args::elements(arguments) {
        match element.node_kind() {
            NodeKind::QW_EXPR => {
                acc.extend(ast::QwExpr::cast(element).expect("kind checked").words());
            }
            NodeKind::LITERAL => {
                if let Some(text) = Literal::cast(element).and_then(|literal| literal.as_string()) {
                    acc.push(text);
                }
            }
            _ => {}
        }
    }
    acc
}

/// Whether the quote-like operator this token belongs to carries `flag`.
fn has_flag(token: &SyntaxToken, flag: char) -> bool {
    token.parent().is_some_and(|parent| {
        ast::tokens(&parent)
            .find(|sibling| sibling.token_kind() == TokenKind::REGEX_FLAGS)
            .is_some_and(|flags| flags.text().contains(flag))
    })
}

/// Whether this is the replacement half of an `s///e`.
fn evaluated_replacement(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };
    if parent.node_kind() != NodeKind::S_EXPR {
        return false;
    }
    has_flag(token, 'e')
}

/// `m'...'` does not interpolate; every other delimiter does.
fn delimiter_interpolates(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return true;
    };
    ast::tokens(&parent)
        .find(|sibling| sibling.token_kind() == TokenKind::DELIMITER)
        .is_none_or(|delimiter| delimiter.text() != "'")
}

/// Which heredoc bodies interpolate, keyed by where each body starts.
///
/// The marker that decides it (`<<'EOT'` against `<<"EOT"`) is on the line
/// above and belongs to another node — the body is a token that lands between
/// two statements (the parser contract). Pairing them in document order is
/// what perl does too: bodies arrive in the order their markers were written.
fn heredoc_interpolation(root: &SyntaxNode) -> HashMap<TextSize, bool> {
    let mut markers = Vec::new();
    let mut bodies = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        match token.token_kind() {
            TokenKind::HEREDOC_START => {
                markers.push(!token.text().contains('\''));
            }
            TokenKind::HEREDOC_CONTENT => bodies.push(token.text_range().start()),
            _ => {}
        }
    }
    bodies.into_iter().zip(markers).collect::<HashMap<_, _>>()
}
