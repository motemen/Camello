//! Typed views over the CST (`docs/typecheck.md`, "The AST layer").
//!
//! The formatter walks kinds directly because its questions are about tokens
//! and trivia. The checker's questions are about structure, and asking them by
//! kind-matching over child iterators in every pass is how a checker becomes
//! unmaintainable. So: one newtype per [`NodeKind`], `cast` from a
//! [`SyntaxNode`], and accessors that return other views or tokens.
//!
//! The `cast`/`syntax` boilerplate is generated from the `nodes` section of
//! `define_language!` — [`crate::lang::ast_views`] — and the accessors are
//! hand-written below, the way `lang/predicates.rs` is hand-written beside the
//! generated enums. Nothing here changes the CST, the `SyntaxKind` numbering,
//! or anything the formatter reads.
//!
//! Two names differ from the design document, because the generated views are
//! named after their kinds and two of those names were already taken:
//! [`SubscriptChain`] is the document's `Subscript` (the generated
//! [`Subscript`] is the `SUBSCRIPT` node, one key inside one pair of braces),
//! and the document's `Call` is [`Call`], a union that the three call kinds
//! cast into.

use crate::lang::{NodeExt, NodeKind, SyntaxNode, SyntaxToken, TokenExt, TokenKind, T};

/// A view over one node kind.
///
/// `KIND_NAME` is the kind's own spelling, so a view can say what it is without
/// a second table; `dev dump` prints the other direction, [`NodeKind::view_name`].
pub trait AstNode: Sized {
    const KIND_NAME: &'static str;

    fn can_cast(kind: NodeKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;

    /// The source text of the whole node, trivia included.
    fn text(&self) -> String {
        self.syntax().text().to_string()
    }

    /// Where the node begins, for a diagnostic's span.
    fn range(&self) -> rowan::TextRange {
        self.syntax().text_range()
    }
}

crate::lang::ast_views!();

// ===== Shared navigation =====
//
// Every accessor below is one of these four questions, so they are written
// once. `children` is ordered, which matters: the first `SUB_NAME` under a
// `SUB_DEF` is the sub's name and the ones below it are barewords in its body.

/// The first child that casts to `N`.
#[must_use]
pub fn child<N: AstNode>(node: &SyntaxNode) -> Option<N> {
    node.children().find_map(N::cast)
}

/// Every child that casts to `N`, in order.
pub fn children<N: AstNode>(node: &SyntaxNode) -> impl Iterator<Item = N> {
    node.children().filter_map(N::cast)
}

/// The first direct token of `kind`.
#[must_use]
pub fn child_token(node: &SyntaxNode, kind: TokenKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.token_kind() == kind)
}

/// The direct non-trivia tokens, in order.
pub fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.token_kind().is_trivia())
}

/// The node's non-trivia token text, concatenated.
///
/// This is how a dotted name (`Foo::Bar`) is read back: the lexer emits it as
/// one `IDENT` today, but `Foo :: Bar` is three tokens and the same name.
#[must_use]
pub fn joined_text(node: &SyntaxNode) -> String {
    let mut out = String::new();
    for token in node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if !token.token_kind().is_trivia() {
            out.push_str(token.text());
        }
    }
    out
}

// ===== Files, packages, uses =====

impl Root {
    pub fn statements(&self) -> impl Iterator<Item = SyntaxNode> {
        self.0.children()
    }
}

impl Block {
    pub fn statements(&self) -> impl Iterator<Item = SyntaxNode> {
        self.0.children()
    }
}

impl PackageStmt {
    /// `package Foo::Bar;` — the name.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        child::<SubName>(&self.0).map(|name| name.text())
    }

    /// `package Foo { ... }` — the block form, if this is one.
    #[must_use]
    pub fn block(&self) -> Option<Block> {
        child(&self.0)
    }
}

impl UseStmt {
    /// The module, or `None` for `use 5.010` and `use strict` written oddly.
    #[must_use]
    pub fn module(&self) -> Option<String> {
        child::<SubName>(&self.0).map(|name| name.text())
    }

    /// The import list as one expression — `use Foo qw(a b)`, `use
    /// Class::Accessor::Typed (rw => {...})`. A declaration for the
    /// recognisers, not an argument list for the checker.
    #[must_use]
    pub fn arguments(&self) -> Option<SyntaxNode> {
        self.0
            .children()
            .find(|child| child.node_kind() != NodeKind::SUB_NAME)
    }
}

impl NoStmt {
    #[must_use]
    pub fn module(&self) -> Option<String> {
        child::<SubName>(&self.0).map(|name| name.text())
    }

    #[must_use]
    pub fn arguments(&self) -> Option<SyntaxNode> {
        self.0
            .children()
            .find(|child| child.node_kind() != NodeKind::SUB_NAME)
    }
}

impl SubName {
    #[must_use]
    pub fn text(&self) -> String {
        joined_text(&self.0)
    }
}

// ===== Subs =====

impl SubDef {
    #[must_use]
    pub fn name(&self) -> Option<SubName> {
        child(&self.0)
    }

    #[must_use]
    pub fn name_text(&self) -> Option<String> {
        self.name().map(|name| name.text())
    }

    #[must_use]
    pub fn signature(&self) -> Option<SubSignature> {
        child(&self.0)
    }

    #[must_use]
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }

    pub fn attrs(&self) -> impl Iterator<Item = Attr> {
        children(&self.0)
    }

    /// Whether this is `sub f;` — a forward declaration with no body.
    #[must_use]
    pub fn is_forward_declaration(&self) -> bool {
        self.body().is_none()
    }

    /// Whether this was written `method f { ... }` rather than `sub f { ... }`.
    ///
    /// The two differ in what the parameter list means: perl gives a `method`
    /// its invocant without the signature naming one, and keeps it out of
    /// `@_`. So `method f()` takes an invocant and no arguments, while `sub
    /// f()` takes nothing at all.
    #[must_use]
    pub fn is_method(&self) -> bool {
        tokens(&self.0).any(|token| token.token_kind() == TokenKind::METHOD_KW)
    }

    /// The comment block immediately above the `sub`, in source order.
    ///
    /// The one accessor the formatter would not have wanted and the checker
    /// cannot do without: it is where `Returns:` lives. The trivia model puts
    /// an own-line comment *outside* the node it precedes, so this walks back
    /// over the preceding tokens rather than looking inside.
    ///
    /// Blank lines between the block and the `sub` are allowed and blank lines
    /// *within* it are not, which is what makes "the block immediately
    /// preceding" a decidable question rather than "every comment above".
    #[must_use]
    pub fn leading_comments(&self) -> Vec<SyntaxToken> {
        leading_comments(&self.0)
    }
}

/// The comment block immediately above `node` (see [`SubDef::leading_comments`]).
///
/// Two things make this a block rather than "every comment above": a comment
/// sharing a line with code belongs to that code (the trivia model), so it is
/// not part of anything's leading block; and a blank line inside the run ends
/// it, so the paragraph above a blank line is somebody else's.
#[must_use]
pub fn leading_comments(node: &SyntaxNode) -> Vec<SyntaxToken> {
    let Some(first) = node.first_token() else {
        return Vec::new();
    };

    // The gap between the node and the block above it: whitespace and
    // newlines, any number of them.
    let mut token = first.prev_token();
    while let Some(current) = token.clone() {
        match current.token_kind() {
            TokenKind::WHITESPACE | TokenKind::NEWLINE => token = current.prev_token(),
            _ => break,
        }
    }

    // The block itself, walked upwards. One newline separates two comment
    // lines; a second is a blank line and ends the block.
    let mut acc = Vec::new();
    let mut newlines = 0usize;
    while let Some(current) = token.clone() {
        match current.token_kind() {
            TokenKind::COMMENT => {
                if !is_own_line(&current) {
                    break;
                }
                newlines = 0;
                acc.push(current.clone());
            }
            TokenKind::NEWLINE => {
                newlines += 1;
                if newlines > 1 {
                    break;
                }
            }
            TokenKind::WHITESPACE => {}
            _ => break,
        }
        token = current.prev_token();
    }
    acc.reverse();
    acc
}

/// Whether nothing but whitespace precedes this token on its line.
fn is_own_line(token: &SyntaxToken) -> bool {
    let mut previous = token.prev_token();
    while let Some(current) = previous {
        match current.token_kind() {
            TokenKind::WHITESPACE => previous = current.prev_token(),
            TokenKind::NEWLINE => return true,
            _ => return false,
        }
    }
    true
}

impl SubSignature {
    /// The parameters, wherever the parser put them.
    ///
    /// They sit in a `LIST_EXPR` of their own, so that a signature has the
    /// `( LIST_EXPR )` shape every other bracketed list has and the
    /// formatter's one bracket rule reaches it. That is a fact about the tree
    /// and not about signatures, which is what this layer exists to hide;
    /// a parameter written as a direct child is still read.
    pub fn params(&self) -> impl Iterator<Item = SignatureParam> {
        let mut found = Vec::new();
        for child in self.0.children() {
            if let Some(param) = SignatureParam::cast(child.clone()) {
                found.push(param);
            } else if child.node_kind() == NodeKind::LIST_EXPR {
                found.extend(child.children().filter_map(SignatureParam::cast));
            }
        }
        found.into_iter()
    }
}

impl SignatureParam {
    /// The parameter variable, `$x` / `@rest` / `%opts`.
    #[must_use]
    pub fn variable(&self) -> Option<Variable> {
        self.0.children().find_map(Variable::cast)
    }

    /// `= 1` or `//= []`; its presence is what makes the parameter optional.
    #[must_use]
    pub fn default(&self) -> Option<SignatureDefault> {
        child(&self.0)
    }
}

impl Attr {
    /// `:lvalue` — the attribute's name.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        tokens(&self.0)
            .find(|token| token.token_kind() == TokenKind::IDENT)
            .map(|token| token.text().to_string())
    }
}

// ===== Declarations =====

/// `my` / `our` / `state` / `local`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKeyword {
    My,
    Our,
    State,
    Local,
}

impl VarDecl {
    #[must_use]
    pub fn keyword(&self) -> Option<DeclKeyword> {
        tokens(&self.0).find_map(|token| match token.token_kind() {
            T!["my"] => Some(DeclKeyword::My),
            T!["our"] => Some(DeclKeyword::Our),
            T!["state"] => Some(DeclKeyword::State),
            T!["local"] => Some(DeclKeyword::Local),
            _ => None,
        })
    }

    /// The variables being declared, `my ($a, %b)` included.
    #[must_use]
    pub fn targets(&self) -> Vec<Variable> {
        let Some(target) = child::<DeclTarget>(&self.0) else {
            return Vec::new();
        };
        let mut acc = Vec::new();
        collect_variables(&target.0, &mut acc);
        acc
    }
}

fn collect_variables(node: &SyntaxNode, acc: &mut Vec<Variable>) {
    for child in node.children() {
        if let Some(variable) = Variable::cast(child.clone()) {
            acc.push(variable);
        } else {
            collect_variables(&child, acc);
        }
    }
}

/// A variable of any sigil, as one view: the checker asks "which name, which
/// sigil" far more often than it asks "which of the five node kinds".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Variable {
    Scalar(ScalarVar),
    Array(ArrayVar),
    Hash(HashVar),
    Code(CodeVar),
    Typeglob(TypeglobVar),
    /// `$#array`.
    LastIndex(ArrayLastIndex),
}

/// What a variable's sigil says it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sigil {
    Scalar,
    Array,
    Hash,
    Code,
    Typeglob,
}

impl Sigil {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Sigil::Scalar => "$",
            Sigil::Array => "@",
            Sigil::Hash => "%",
            Sigil::Code => "&",
            Sigil::Typeglob => "*",
        }
    }
}

impl Variable {
    #[must_use]
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.node_kind() {
            NodeKind::SCALAR_VAR => Some(Variable::Scalar(ScalarVar(node))),
            NodeKind::ARRAY_VAR => Some(Variable::Array(ArrayVar(node))),
            NodeKind::HASH_VAR => Some(Variable::Hash(HashVar(node))),
            NodeKind::CODE_VAR => Some(Variable::Code(CodeVar(node))),
            NodeKind::TYPEGLOB_VAR => Some(Variable::Typeglob(TypeglobVar(node))),
            NodeKind::ARRAY_LAST_INDEX => Some(Variable::LastIndex(ArrayLastIndex(node))),
            _ => None,
        }
    }

    #[must_use]
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Variable::Scalar(view) => &view.0,
            Variable::Array(view) => &view.0,
            Variable::Hash(view) => &view.0,
            Variable::Code(view) => &view.0,
            Variable::Typeglob(view) => &view.0,
            Variable::LastIndex(view) => &view.0,
        }
    }

    #[must_use]
    pub fn sigil(&self) -> Sigil {
        match self {
            Variable::Scalar(_) => Sigil::Scalar,
            // `$#a` names the array, so it is an array use.
            Variable::Array(_) | Variable::LastIndex(_) => Sigil::Array,
            Variable::Hash(_) => Sigil::Hash,
            Variable::Code(_) => Sigil::Code,
            Variable::Typeglob(_) => Sigil::Typeglob,
        }
    }

    /// The name without its sigil, or `None` when the "name" is an expression
    /// — `${ $ref }`, `@{[ ... ]}` — which is a dereference, not a use.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        let node = self.syntax();
        let mut name = String::new();
        for token in tokens(node) {
            match token.token_kind() {
                TokenKind::IDENT | TokenKind::DOUBLE_COLON | TokenKind::NUMBER => {
                    name.push_str(token.text());
                }
                kind if kind.is_keyword() => name.push_str(token.text()),
                _ => {}
            }
        }
        (!name.is_empty()).then_some(name)
    }

    /// `$x` with the sigil, for a message.
    #[must_use]
    pub fn display(&self) -> String {
        match self.name() {
            Some(name) => format!("{}{name}", self.sigil().as_str()),
            None => self.sigil().as_str().to_string(),
        }
    }

    #[must_use]
    pub fn range(&self) -> rowan::TextRange {
        self.syntax().text_range()
    }
}

// ===== Calls =====

/// A call to a named sub, in any of the three shapes the parser distinguishes
/// (the parser contract): `foo(...)`, `foo ...`, `$code->(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// `foo(1, 2)` — parenthesised.
    Paren(CallExpr),
    /// `foo 1, 2` — a list operator, and also every bareword: `name` on its own
    /// is a `LIST_CALL_EXPR` with no arguments (the parser contract's guess).
    List(ListCallExpr),
    /// `$code->(1)`.
    Code(CodeCallExpr),
}

impl Call {
    #[must_use]
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.node_kind() {
            NodeKind::CALL_EXPR => Some(Call::Paren(CallExpr(node))),
            NodeKind::LIST_CALL_EXPR => Some(Call::List(ListCallExpr(node))),
            NodeKind::CODE_CALL_EXPR => Some(Call::Code(CodeCallExpr(node))),
            _ => None,
        }
    }

    #[must_use]
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Call::Paren(view) => &view.0,
            Call::List(view) => &view.0,
            Call::Code(view) => &view.0,
        }
    }

    /// The callee's name, or `None` for `$code->(...)`.
    #[must_use]
    pub fn callee(&self) -> Option<SubName> {
        match self {
            Call::Paren(view) => child(&view.0),
            Call::List(view) => child(&view.0),
            Call::Code(_) => None,
        }
    }

    #[must_use]
    pub fn callee_name(&self) -> Option<String> {
        self.callee().map(|name| name.text())
    }

    /// The range of the callee's name, which is where a diagnostic about the
    /// call belongs — not the whole call, which may be a screenful.
    #[must_use]
    pub fn callee_range(&self) -> rowan::TextRange {
        self.callee()
            .map_or_else(|| self.syntax().text_range(), |name| name.0.text_range())
    }

    /// A filehandle argument is a child of its own rather than an unexplained
    /// first argument (the parser contract), so it is never in [`Call::args`].
    #[must_use]
    pub fn filehandle(&self) -> Option<Filehandle> {
        child(self.syntax())
    }

    /// The argument list, flat, with the separators still readable.
    #[must_use]
    pub fn arg_list(&self) -> Option<SyntaxNode> {
        match self {
            // `foo(...)` and `$code->(...)`: an ARG_LIST wrapping a LIST_EXPR.
            Call::Paren(_) | Call::Code(_) => child::<ArgList>(self.syntax())
                .and_then(|args| args.0.children().find(Args::is_list)),
            // `foo 1, 2`: the LIST_EXPR is the call's own child.
            Call::List(view) => view.0.children().find(Args::is_list),
        }
    }

    /// The arguments as a flat list, fat-comma pairs left in place.
    #[must_use]
    pub fn args(&self) -> Vec<SyntaxNode> {
        self.arg_list()
            .map(|list| Args::elements(&list))
            .unwrap_or_default()
    }

    /// The arguments read as `key => value` pairs where the key is a bareword
    /// or a string, positional otherwise (`docs/typecheck.md`, "The AST layer").
    #[must_use]
    pub fn pairs(&self) -> Vec<Arg> {
        self.arg_list()
            .map(|list| Args::pairs(&list))
            .unwrap_or_default()
    }
}

impl MethodCallExpr {
    /// What the method is called on: `$obj`, `Foo::Bar`, `$class`.
    #[must_use]
    pub fn invocant(&self) -> Option<SyntaxNode> {
        self.0.children().next()
    }

    /// The method's name, or `None` when it is `$obj->$name(...)` — dynamic,
    /// and therefore opaque (`docs/typecheck.md`, non-goals).
    #[must_use]
    pub fn method(&self) -> Option<SubName> {
        self.0.children().skip(1).find_map(SubName::cast)
    }

    #[must_use]
    pub fn method_name(&self) -> Option<String> {
        self.method().map(|name| name.text())
    }

    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.method().is_none()
    }

    #[must_use]
    pub fn arg_list(&self) -> Option<SyntaxNode> {
        child::<ArgList>(&self.0).and_then(|args| args.0.children().find(Args::is_list))
    }

    #[must_use]
    pub fn args(&self) -> Vec<SyntaxNode> {
        self.arg_list()
            .map(|list| Args::elements(&list))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn pairs(&self) -> Vec<Arg> {
        self.arg_list()
            .map(|list| Args::pairs(&list))
            .unwrap_or_default()
    }

    /// Where a diagnostic about the call belongs.
    #[must_use]
    pub fn method_range(&self) -> rowan::TextRange {
        self.method()
            .map_or_else(|| self.0.text_range(), |name| name.0.text_range())
    }
}

/// The document's `MethodCall`.
pub type MethodCall = MethodCallExpr;
/// The document's `Assign`.
pub type Assign = AssignExpr;
/// The document's `AnonSub`.
pub type AnonSub = AnonSubExpr;

/// One argument: a `key => value` pair, or a value on its own.
#[derive(Debug, Clone)]
pub enum Arg {
    Pair {
        key: SyntaxNode,
        /// The key read as a name, when it is a bareword or a plain string.
        key_text: Option<String>,
        value: SyntaxNode,
    },
    Positional(SyntaxNode),
}

impl Arg {
    #[must_use]
    pub fn node(&self) -> &SyntaxNode {
        match self {
            Arg::Pair { value, .. } | Arg::Positional(value) => value,
        }
    }

    #[must_use]
    pub fn key(&self) -> Option<&str> {
        match self {
            Arg::Pair { key_text, .. } => key_text.as_deref(),
            Arg::Positional(_) => None,
        }
    }

    #[must_use]
    pub fn range(&self) -> rowan::TextRange {
        match self {
            Arg::Pair { key, value, .. } => key.text_range().cover(value.text_range()),
            Arg::Positional(value) => value.text_range(),
        }
    }
}

/// Reading a comma series (the parser contract: always a `LIST_EXPR`).
pub struct Args;

impl Args {
    /// A `LIST_EXPR`, or a `PAREN_EXPR` around one — `has x => (is => 'ro')`
    /// puts the options behind parentheses and means the same list.
    #[must_use]
    pub fn is_list(node: &SyntaxNode) -> bool {
        matches!(node.node_kind(), NodeKind::LIST_EXPR | NodeKind::PAREN_EXPR)
    }

    /// The elements of a comma series, parentheses and single elements alike.
    #[must_use]
    pub fn elements(node: &SyntaxNode) -> Vec<SyntaxNode> {
        match node.node_kind() {
            NodeKind::PAREN_EXPR => node
                .children()
                .next()
                .map(|inner| Args::elements(&inner))
                .unwrap_or_default(),
            NodeKind::LIST_EXPR => {
                // A list holding one parenthesised list is that list: `use Foo
                // (a => 1)` and `use Foo a => 1` are the same import, and perl
                // flattens `f((1, 2))` to two arguments too.
                let mut children = node.children();
                match (children.next(), children.next()) {
                    (Some(only), None) if only.node_kind() == NodeKind::PAREN_EXPR => {
                        Args::elements(&only)
                    }
                    _ => node.children().collect(),
                }
            }
            _ => vec![node.clone()],
        }
    }

    /// The same series, with `=>` pairs joined up.
    ///
    /// A fat comma is a token of the `LIST_EXPR`, so the pairing is read off
    /// the separators rather than guessed from the shape of the elements.
    #[must_use]
    pub fn pairs(node: &SyntaxNode) -> Vec<Arg> {
        let list = match node.node_kind() {
            NodeKind::PAREN_EXPR => match node.children().next() {
                Some(inner) => return Args::pairs(&inner),
                None => return Vec::new(),
            },
            NodeKind::LIST_EXPR => {
                // See `elements`: one parenthesised list is that list.
                let mut children = node.children();
                match (children.next(), children.next()) {
                    (Some(only), None) if only.node_kind() == NodeKind::PAREN_EXPR => {
                        return Args::pairs(&only)
                    }
                    _ => node.clone(),
                }
            }
            _ => return vec![Arg::Positional(node.clone())],
        };

        let mut acc: Vec<Arg> = Vec::new();
        let mut pending: Option<SyntaxNode> = None;
        let mut fat = false;
        for element in list.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    if token.token_kind() == T!["=>"] {
                        fat = true;
                    } else if token.token_kind() == T![","] {
                        if let Some(node) = pending.take() {
                            acc.push(Arg::Positional(node));
                        }
                        fat = false;
                    }
                }
                rowan::NodeOrToken::Node(node) => {
                    if fat {
                        let key = pending.take().unwrap_or_else(|| node.clone());
                        acc.push(Arg::Pair {
                            key_text: key_text(&key),
                            key,
                            value: node,
                        });
                        fat = false;
                    } else if let Some(previous) = pending.replace(node) {
                        acc.push(Arg::Positional(previous));
                    }
                }
            }
        }
        if let Some(node) = pending {
            acc.push(Arg::Positional(node));
        }
        acc
    }
}

/// What a `+` disambiguator wraps, and the node itself where there is none.
///
/// `+{ ... }` is how a writer tells perl that the brace opens a hashref and
/// not a block, and `+(...)` that the parentheses are not a call's argument
/// list. The `+` says nothing about the value, so everything that reads a
/// value by its shape has to look through it — `args my $x => +{ isa => 'Int',
/// optional => 1 }` declares the same rule as the one written without it.
#[must_use]
pub fn without_plus(node: &SyntaxNode) -> SyntaxNode {
    if node.node_kind() != NodeKind::PREFIX_EXPR {
        return node.clone();
    }
    if tokens(node).next().map(|token| token.token_kind()) != Some(TokenKind::PLUS) {
        return node.clone();
    }
    node.children().next().unwrap_or_else(|| node.clone())
}

/// A node read as a hash key: a bareword, or a string with nothing in it that
/// would have to be interpolated.
#[must_use]
pub fn key_text(node: &SyntaxNode) -> Option<String> {
    match node.node_kind() {
        // A bareword is a `LIST_CALL_EXPR` with a name and no arguments.
        NodeKind::LIST_CALL_EXPR => {
            let name = child::<SubName>(node)?;
            (node.children().count() == 1).then(|| name.text())
        }
        NodeKind::SUB_NAME => Some(joined_text(node)),
        NodeKind::LITERAL => Literal(node.clone()).as_string(),
        NodeKind::Q_EXPR | NodeKind::QQ_EXPR => quote_like_content(node),
        _ => None,
    }
}

/// The body of a `q{...}` / `qq{...}`, when it holds no interpolation.
fn quote_like_content(node: &SyntaxNode) -> Option<String> {
    let content = tokens(node).find(|token| {
        matches!(
            token.token_kind(),
            TokenKind::LITERAL_STRING | TokenKind::INTERPOLATED_STRING
        )
    })?;
    let text = content.text();
    (!text.contains('$') && !text.contains('@')).then(|| text.to_string())
}

impl Literal {
    /// The one token a literal is.
    #[must_use]
    pub fn token(&self) -> Option<SyntaxToken> {
        tokens(&self.0).next()
    }

    #[must_use]
    pub fn is_number(&self) -> bool {
        self.token()
            .is_some_and(|token| token.token_kind() == TokenKind::NUMBER)
    }

    /// The literal's text with its quotes removed, when it is a string that
    /// says exactly what it holds — no interpolation, no escapes beyond the
    /// two a single-quoted string has.
    #[must_use]
    pub fn as_string(&self) -> Option<String> {
        let token = self.token()?;
        if token.token_kind() != TokenKind::STRING {
            return None;
        }
        let text = token.text();
        let mut chars = text.chars();
        let open = chars.next()?;
        let body = &text[open.len_utf8()..text.len().checked_sub(1)?];
        match open {
            '\'' => Some(body.replace("\\'", "'").replace("\\\\", "\\")),
            '"' => {
                // An interpolating string is a use of whatever it names; only
                // one with nothing to interpolate is a literal (the
                // interpolation scanner is `sema`'s, not this layer's).
                (!body.contains('$') && !body.contains('@') && !body.contains('\\'))
                    .then(|| body.to_string())
            }
            _ => None,
        }
    }

    /// The number's text, for `42` against an `Int` slot.
    #[must_use]
    pub fn as_number(&self) -> Option<String> {
        let token = self.token()?;
        (token.token_kind() == TokenKind::NUMBER).then(|| token.text().to_string())
    }
}

impl QwExpr {
    /// The words, split the way perl splits them.
    #[must_use]
    pub fn words(&self) -> Vec<String> {
        tokens(&self.0)
            .find(|token| token.token_kind() == TokenKind::QW_STRING)
            .map(|token| {
                token
                    .text()
                    .split_whitespace()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl AnonHash {
    /// The `{ k => v }` contents as pairs.
    #[must_use]
    pub fn pairs(&self) -> Vec<Arg> {
        self.0
            .children()
            .find(Args::is_list)
            .map(|list| Args::pairs(&list))
            .unwrap_or_default()
    }
}

impl AnonArray {
    #[must_use]
    pub fn elements(&self) -> Vec<SyntaxNode> {
        self.0
            .children()
            .find(Args::is_list)
            .map(|list| Args::elements(&list))
            .unwrap_or_default()
    }
}

impl AnonSubExpr {
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }

    #[must_use]
    pub fn signature(&self) -> Option<SubSignature> {
        child(&self.0)
    }
}

impl AssignExpr {
    /// The left-hand side.
    #[must_use]
    pub fn target(&self) -> Option<SyntaxNode> {
        self.0.children().next()
    }

    /// The right-hand side, or `None` for `my $x;`.
    #[must_use]
    pub fn value(&self) -> Option<SyntaxNode> {
        self.0.children().nth(1)
    }

    /// The operator: `=`, `//=`, `||=` … Compound assignment is one token
    /// (the language model), so this is the whole answer.
    #[must_use]
    pub fn operator(&self) -> Option<TokenKind> {
        tokens(&self.0)
            .map(|token| token.token_kind())
            .find(|kind| {
                matches!(
                    kind,
                    T!["="]
                        | T!["+="]
                        | T!["-="]
                        | T!["*="]
                        | T!["/="]
                        | T!["%="]
                        | T!["**="]
                        | T![".="]
                        | T!["x="]
                        | T!["//="]
                        | T!["||="]
                        | T!["&&="]
                        | T!["|="]
                        | T!["&="]
                        | T!["^="]
                        | T!["<<="]
                        | T![">>="]
                )
            })
    }

    /// Whether the assignment is a plain `=`, which is the only one that
    /// replaces the target's type rather than combining with it.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.operator() == Some(T!["="])
    }
}

// ===== Subscript chains =====

/// One step of a subscript chain.
#[derive(Debug, Clone)]
pub enum Step {
    /// `->{k}` / `{k}`; the key when it is a literal or bareword.
    Hash {
        key: Option<String>,
        node: SyntaxNode,
    },
    /// `->[0]` / `[0]`; the index when it is a literal.
    Array {
        index: Option<i64>,
        node: SyntaxNode,
    },
    /// `->@*`, `->%*`, `->$*`, and the slice forms.
    Deref { sigil: Sigil, node: SyntaxNode },
    /// `@h{...}`, `@a[...]` — a slice yields a list, not one element.
    Slice { node: SyntaxNode },
}

impl Step {
    #[must_use]
    pub fn node(&self) -> &SyntaxNode {
        match self {
            Step::Hash { node, .. }
            | Step::Array { node, .. }
            | Step::Deref { node, .. }
            | Step::Slice { node } => node,
        }
    }
}

/// The document's `Subscript`: `$x->{a}[0]{b}` as a base plus a list of steps.
///
/// The CST nests these left-to-right, so the outermost node is the *last*
/// step; this unwinds that into the order a reader would say them in.
#[derive(Debug, Clone)]
pub struct SubscriptChain {
    base: SyntaxNode,
    steps: Vec<Step>,
}

impl SubscriptChain {
    #[must_use]
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if !Self::is_step_kind(node.node_kind()) {
            return None;
        }
        let mut steps = Vec::new();
        let mut current = node;
        loop {
            let step = Self::step_of(&current)?;
            steps.push(step);
            let inner = current.children().next()?;
            if Self::is_step_kind(inner.node_kind()) {
                current = inner;
            } else {
                steps.reverse();
                return Some(SubscriptChain { base: inner, steps });
            }
        }
    }

    fn is_step_kind(kind: NodeKind) -> bool {
        matches!(
            kind,
            NodeKind::HASH_SUBSCRIPT_EXPR
                | NodeKind::ARRAY_SUBSCRIPT_EXPR
                | NodeKind::POSTFIX_DEREF_EXPR
                | NodeKind::SLICE_EXPR
                | NodeKind::POSTFIX_ARRAY_SLICE_EXPR
                | NodeKind::POSTFIX_HASH_SLICE_EXPR
        )
    }

    fn step_of(node: &SyntaxNode) -> Option<Step> {
        let subscript = child::<Subscript>(node).map(|view| view.0);
        match node.node_kind() {
            NodeKind::HASH_SUBSCRIPT_EXPR => {
                let key = subscript
                    .as_ref()
                    .and_then(|inner| inner.children().next())
                    .and_then(|inner| key_text(&inner));
                Some(Step::Hash {
                    key,
                    node: node.clone(),
                })
            }
            NodeKind::ARRAY_SUBSCRIPT_EXPR => {
                let index = subscript
                    .as_ref()
                    .and_then(|inner| inner.children().next())
                    .and_then(Literal::cast)
                    .and_then(|literal| literal.as_number())
                    .and_then(|text| text.parse().ok());
                Some(Step::Array {
                    index,
                    node: node.clone(),
                })
            }
            NodeKind::POSTFIX_DEREF_EXPR => {
                let sigil = tokens(node).find_map(|token| match token.token_kind() {
                    T!["->@*"] | T!["->$#*"] => Some(Sigil::Array),
                    T!["->%*"] => Some(Sigil::Hash),
                    T!["->$*"] => Some(Sigil::Scalar),
                    T!["->&*"] => Some(Sigil::Code),
                    T!["->**"] => Some(Sigil::Typeglob),
                    _ => None,
                })?;
                Some(Step::Deref {
                    sigil,
                    node: node.clone(),
                })
            }
            NodeKind::SLICE_EXPR
            | NodeKind::POSTFIX_ARRAY_SLICE_EXPR
            | NodeKind::POSTFIX_HASH_SLICE_EXPR => Some(Step::Slice { node: node.clone() }),
            _ => None,
        }
    }

    #[must_use]
    pub fn base(&self) -> &SyntaxNode {
        &self.base
    }

    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

// ===== The expression a pass branches on =====

/// The shapes the checker actually asks about, with everything else behind
/// [`Expr::Other`].
///
/// This is not a closed grammar of Perl expressions and is not meant to be:
/// an `Other` is walked for its children and contributes `Unknown`, which is
/// the rule that keeps the checker quiet (`docs/typecheck.md`, "The lattice").
#[derive(Debug, Clone)]
pub enum Expr {
    Call(Call),
    MethodCall(MethodCall),
    Subscript(SubscriptChain),
    Assign(Assign),
    Literal(Literal),
    AnonHash(AnonHash),
    AnonArray(AnonArray),
    AnonSub(AnonSub),
    Variable(Variable),
    Paren(ParenExpr),
    List(ListExpr),
    Binary(BinaryExpr),
    Ternary(TernaryExpr),
    Prefix(PrefixExpr),
    Reference(ReferenceExpr),
    Qw(QwExpr),
    Other(SyntaxNode),
}

impl Expr {
    #[must_use]
    pub fn cast(node: SyntaxNode) -> Self {
        if let Some(chain) = SubscriptChain::cast(node.clone()) {
            return Expr::Subscript(chain);
        }
        if let Some(call) = Call::cast(node.clone()) {
            return Expr::Call(call);
        }
        if let Some(variable) = Variable::cast(node.clone()) {
            return Expr::Variable(variable);
        }
        match node.node_kind() {
            NodeKind::METHOD_CALL_EXPR => Expr::MethodCall(MethodCallExpr(node)),
            NodeKind::ASSIGN_EXPR => Expr::Assign(AssignExpr(node)),
            NodeKind::LITERAL => Expr::Literal(Literal(node)),
            NodeKind::ANON_HASH => Expr::AnonHash(AnonHash(node)),
            NodeKind::ANON_ARRAY => Expr::AnonArray(AnonArray(node)),
            NodeKind::ANON_SUB_EXPR => Expr::AnonSub(AnonSubExpr(node)),
            NodeKind::PAREN_EXPR => Expr::Paren(ParenExpr(node)),
            NodeKind::LIST_EXPR => Expr::List(ListExpr(node)),
            NodeKind::BINARY_EXPR => Expr::Binary(BinaryExpr(node)),
            NodeKind::TERNARY_EXPR => Expr::Ternary(TernaryExpr(node)),
            NodeKind::PREFIX_EXPR => Expr::Prefix(PrefixExpr(node)),
            NodeKind::REFERENCE_EXPR => Expr::Reference(ReferenceExpr(node)),
            NodeKind::QW_EXPR => Expr::Qw(QwExpr(node)),
            _ => Expr::Other(node),
        }
    }

    #[must_use]
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Call(view) => view.syntax(),
            Expr::MethodCall(view) => &view.0,
            Expr::Subscript(view) => {
                // The chain's own node is the outermost step.
                view.steps
                    .last()
                    .map_or_else(|| &view.base, |step| step.node())
            }
            Expr::Assign(view) => &view.0,
            Expr::Literal(view) => &view.0,
            Expr::AnonHash(view) => &view.0,
            Expr::AnonArray(view) => &view.0,
            Expr::AnonSub(view) => &view.0,
            Expr::Variable(view) => view.syntax(),
            Expr::Paren(view) => &view.0,
            Expr::List(view) => &view.0,
            Expr::Binary(view) => &view.0,
            Expr::Ternary(view) => &view.0,
            Expr::Prefix(view) => &view.0,
            Expr::Reference(view) => &view.0,
            Expr::Qw(view) => &view.0,
            Expr::Other(node) => node,
        }
    }
}

impl BinaryExpr {
    #[must_use]
    pub fn operator(&self) -> Option<TokenKind> {
        tokens(&self.0)
            .map(|token| token.token_kind())
            .find(|kind| kind.is_punct() || kind.is_keyword())
    }

    #[must_use]
    pub fn left(&self) -> Option<SyntaxNode> {
        self.0.children().next()
    }

    #[must_use]
    pub fn right(&self) -> Option<SyntaxNode> {
        self.0.children().nth(1)
    }
}

impl ParenExpr {
    #[must_use]
    pub fn inner(&self) -> Option<SyntaxNode> {
        self.0.children().next()
    }
}

impl ListExpr {
    pub fn elements(&self) -> impl Iterator<Item = SyntaxNode> {
        self.0.children()
    }
}

#[cfg(test)]
mod tests;
