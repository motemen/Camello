//! CST → Doc (ADR 0008 §3).
//!
//! Every layout decision is made here. In particular the flat-or-broken state of
//! each group is decided from the source, once, at the point the group is built
//! — replacing the seven separate "does the source have a newline here"
//! predicates the old formatter had scattered across five files.

use crate::lang::{
    NodeExt, NodeKind, SyntaxElement, SyntaxNode, SyntaxToken, TokenExt, TokenKind, T,
};
use crate::parse::trivia::TriviaMap;

use super::doc::{AnchorClass, Doc, Placement, ShapeKey};
use super::FormatterOptions;

pub struct Builder<'a> {
    trivia: &'a TriviaMap,
    options: &'a FormatterOptions,
    /// Nesting depth of `=>`, so that an inner hash aligns separately from the
    /// one containing it (ADR 0008 §5).
    fat_comma_depth: u8,
}

impl<'a> Builder<'a> {
    pub fn new(trivia: &'a TriviaMap, options: &'a FormatterOptions) -> Self {
        Self {
            trivia,
            options,
            fat_comma_depth: 0,
        }
    }

    pub fn file(&mut self, root: &SyntaxNode) -> Doc {
        let mut parts = Vec::new();
        self.statements_into(root, &mut parts);
        // A file ends with exactly one newline.
        parts.push(Doc::HardLine);
        Doc::concat(parts)
    }

    /// The statements of a root or a block, plus anything sitting between them.
    ///
    /// A heredoc body is a token, not a statement: it lands wherever the line it
    /// starts on falls (ADR 0007 §7), which is between two statements. Walking
    /// only the child *nodes* would drop it.
    fn statements_into(&mut self, node: &SyntaxNode, parts: &mut Vec<Doc>) {
        for child in node.children_with_tokens() {
            match child {
                SyntaxElement::Node(statement) => self.statement_into(&statement, parts),
                SyntaxElement::Token(token) if token.token_kind().is_heredoc_body() => {
                    let terminator = token.token_kind() != TokenKind::HEREDOC_CONTENT;
                    parts.push(Doc::VerbatimLines(token));
                    if terminator {
                        parts.push(Doc::HardLine);
                    }
                }
                SyntaxElement::Token(_) => {}
            }
        }
    }

    /// A statement. Its comments and blank lines come from its tokens, which is
    /// where every comment in the output comes from (ADR 0008 §4).
    fn statement_into(&mut self, node: &SyntaxNode, parts: &mut Vec<Doc>) {
        if let Some(shape) = shape_key(node) {
            parts.push(Doc::Shape(shape));
        }
        parts.push(self.node(node));
        parts.push(self.trailing_comment_of_last_token(node));
        parts.push(Doc::HardLine);
    }

    /// Own-line comments and blank lines attached to a token.
    ///
    /// Every comment in the output is emitted from here or from
    /// [`Self::trailing_comment`] — two functions in one file, against the old
    /// formatter's two unrelated paths in two files, only one of which honoured
    /// the spacing option.
    fn leading_docs(&mut self, token: &SyntaxToken) -> Doc {
        let trivia = self.trivia.at(token.text_range().start());
        if trivia.leading.is_empty() {
            return Doc::Nil;
        }

        // The newline that ended the previous line went to that token's
        // trailing trivia (ADR 0006 §3), so every NEWLINE here is a line the
        // user left empty. The renderer collapses runs of them to one
        // (formatting.md BLANK_LINE-3).
        let mut parts = Vec::new();
        let mut items = trivia.leading.iter().peekable();
        while let Some(item) = items.next() {
            match item.kind {
                TokenKind::COMMENT => {
                    parts.push(Doc::Comment(item.text.clone(), Placement::OwnLine));
                    parts.push(Doc::HardLine);
                    // The newline that ends the comment's own line is not a
                    // blank line.
                    if items
                        .peek()
                        .is_some_and(|next| next.kind == TokenKind::NEWLINE)
                    {
                        items.next();
                    }
                }
                TokenKind::NEWLINE => parts.push(Doc::BlankLine),
                _ => {}
            }
        }
        Doc::concat(parts)
    }

    /// The comment sharing a line with this token.
    fn trailing_comment(&mut self, token: &SyntaxToken) -> Doc {
        let trivia = self.trivia.at(token.text_range().start());
        for item in &trivia.trailing {
            if item.kind == TokenKind::COMMENT {
                return Doc::Comment(item.text.clone(), Placement::Trailing);
            }
        }
        Doc::Nil
    }

    fn trailing_comment_of_last_token(&mut self, node: &SyntaxNode) -> Doc {
        match last_token(node) {
            Some(token) => self.trailing_comment(&token),
            None => Doc::Nil,
        }
    }

    fn node(&mut self, node: &SyntaxNode) -> Doc {
        match node.node_kind() {
            NodeKind::BLOCK => self.block(node),
            NodeKind::POD | NodeKind::DATA_SECTION => self.verbatim(node),
            NodeKind::HEREDOC_BODY => self.verbatim(node),
            NodeKind::ARG_LIST | NodeKind::PAREN_EXPR => self.delimited(node, T!["("], T![")"]),
            NodeKind::ANON_ARRAY => self.delimited(node, T!["["], T!["]"]),
            NodeKind::ANON_HASH => self.delimited(node, T!["{"], T!["}"]),
            NodeKind::SUBSCRIPT => self.sequence(node),
            _ => self.sequence(node),
        }
    }

    /// The default: children in order, with spacing decided pairwise but with
    /// the parent node in hand, and user newlines preserved.
    fn sequence(&mut self, node: &SyntaxNode) -> Doc {
        let parent = Some(node.node_kind());
        let mut parts = Vec::new();
        let mut previous: Option<SyntaxElement> = None;

        for child in node.children_with_tokens() {
            if child
                .as_token()
                .is_some_and(|token| token.token_kind().is_trivia())
            {
                continue;
            }

            if let Some(previous) = &previous {
                parts.extend(self.separator(previous, &child, parent));
            }

            match &child {
                SyntaxElement::Node(child) => parts.push(self.node(child)),
                SyntaxElement::Token(token) => parts.push(self.token(token)),
            }
            previous = Some(child);
        }

        Doc::concat(parts)
    }

    /// What goes between two adjacent children.
    ///
    /// One function, with the parent's kind available. The old formatter used a
    /// two-token table of 31 special cases that 34 direct writer calls bypassed.
    fn separator(
        &mut self,
        previous: &SyntaxElement,
        next: &SyntaxElement,
        parent: Option<NodeKind>,
    ) -> Vec<Doc> {
        let mut parts = Vec::new();

        // An anchor goes immediately before the thing it aligns.
        if let Some(class) = self.anchor_class(next, parent) {
            parts.push(Doc::Anchor(class));
        }

        if !self.wants_space(previous, next, parent) {
            return parts;
        }

        // A newline the user put here is kept, and the continuation is indented
        // by the enclosing Indent (ADR 0008 §3(2)) — no separate rule for
        // continuation indent, and so none of ADR 0002's fourteen branches.
        // A block's opening brace is placed by the formatter, not the user
        // (formatting.md NEWLINE-2), so a newline before it is not preserved.
        let brace_follows = next
            .as_node()
            .is_some_and(|node| node.node_kind() == NodeKind::BLOCK);

        if !brace_follows && self.has_user_newline_between(previous, next) {
            parts.push(Doc::UserLine { broken: true });
        } else {
            parts.push(Doc::Space);
        }
        parts
    }

    fn anchor_class(&self, next: &SyntaxElement, parent: Option<NodeKind>) -> Option<AnchorClass> {
        let token = next.as_token()?;
        if token.token_kind().is_assignment_op() && parent == Some(NodeKind::ASSIGN_EXPR) {
            return Some(AnchorClass::Assign);
        }
        if token.token_kind() == T!["=>"] {
            return Some(AnchorClass::FatComma(self.fat_comma_depth));
        }
        if parent == Some(NodeKind::STMT_MODIFIER) && token.token_kind().is_stmt_modifier() {
            return Some(AnchorClass::PostfixKeyword);
        }
        None
    }

    fn wants_space(
        &self,
        previous: &SyntaxElement,
        next: &SyntaxElement,
        parent: Option<NodeKind>,
    ) -> bool {
        let before = previous.as_token().map(TokenExt::token_kind);
        let after = next.as_token().map(TokenExt::token_kind);

        // Nothing hugs a `;` or a `,` on its left.
        if matches!(after, Some(T![";"] | T![","])) {
            return false;
        }
        // `->` binds tight on both sides.
        if before == Some(T!["->"]) || after == Some(T!["->"]) {
            return false;
        }
        // A sigil is part of the name that follows it.
        if before.is_some_and(TokenKind::is_sigil) {
            return false;
        }
        // An argument list hugs the name it belongs to: `foo(1)`, not `foo (1)`.
        // The parenthesis is inside ARG_LIST, so the test is on the node.
        if next
            .as_node()
            .is_some_and(|node| node.node_kind() == NodeKind::ARG_LIST)
        {
            return false;
        }

        // Heredoc bodies, POD and `__DATA__` start where they start. Inserting
        // anything before one would change what it contains.
        if matches!(
            after,
            Some(
                TokenKind::HEREDOC_CONTENT
                    | TokenKind::HEREDOC_END
                    | TokenKind::POD_CONTENT
                    | TokenKind::DATA_CONTENT
            )
        ) || matches!(
            before,
            Some(TokenKind::HEREDOC_CONTENT | TokenKind::POD_CONTENT)
        ) {
            return false;
        }

        // Subscripts and call parentheses hug what they apply to.
        if matches!(after, Some(T!["["] | T!["("]))
            && matches!(
                parent,
                Some(
                    NodeKind::ARRAY_SUBSCRIPT_EXPR
                        | NodeKind::HASH_SUBSCRIPT_EXPR
                        | NodeKind::CODE_CALL_EXPR
                        | NodeKind::CALL_EXPR
                        | NodeKind::METHOD_CALL_EXPR
                        | NodeKind::SUB_SIGNATURE
                        | NodeKind::SUB_PROTOTYPE
                        | NodeKind::ATTR_ARGS
                )
            )
        {
            return false;
        }
        if after == Some(T!["{"]) && parent == Some(NodeKind::HASH_SUBSCRIPT_EXPR) {
            return false;
        }
        // Nothing between a prefix operator and its operand.
        if parent == Some(NodeKind::PREFIX_EXPR) || parent == Some(NodeKind::REFERENCE_EXPR) {
            if let Some(kind) = before {
                if matches!(
                    kind,
                    T!["!"] | T!["~"] | T!["\\"] | T!["-"] | T!["+"] | T!["++"] | T!["--"]
                ) {
                    return false;
                }
            }
        }
        // `$i++`.
        if parent == Some(NodeKind::POSTFIX_EXPR) && matches!(after, Some(T!["++"] | T!["--"])) {
            return false;
        }
        // Inside quote-like runs nothing is inserted at all; the delimiters and
        // the content are one lexical unit.
        if parent.is_some_and(is_quote_like_node) {
            return false;
        }
        // `Foo::Bar` and `$#array`.
        if before == Some(T!["::"]) || after == Some(T!["::"]) {
            return false;
        }
        if before == Some(TokenKind::FILE_TEST_OP) {
            return true;
        }
        // An empty delimiter pair stays empty.
        if matches!(before, Some(T!["("] | T!["["])) || matches!(after, Some(T![")"] | T!["]"])) {
            return false;
        }
        true
    }

    /// Whether the source has a line break between two adjacent children.
    ///
    /// The gap between them is exactly the previous token's trailing trivia plus
    /// the next token's leading trivia (ADR 0006 §3), and because no node's
    /// range includes trivia (§4) that is the whole gap — no guessing from node
    /// extents, and nothing from *after* `next` can leak in.
    fn has_user_newline_between(&self, previous: &SyntaxElement, next: &SyntaxElement) -> bool {
        let is_newline = |item: &crate::parse::trivia::Trivia| item.kind == TokenKind::NEWLINE;

        let after_previous = last_token_of(previous)
            .map(|token| self.trivia.at(token.text_range().start()))
            .is_some_and(|trivia| trivia.trailing.iter().any(is_newline));
        if after_previous {
            return true;
        }

        first_token_of(next)
            .map(|token| self.trivia.at(token.text_range().start()))
            .is_some_and(|trivia| trivia.leading.iter().any(is_newline))
    }

    fn token(&mut self, token: &SyntaxToken) -> Doc {
        let kind = token.token_kind();
        let text = if kind.is_verbatim() {
            Doc::Raw(token.clone())
        } else {
            Doc::Token(token.clone())
        };

        let leading = self.leading_docs(token);
        if leading.is_nil() {
            return text;
        }
        Doc::concat(vec![leading, text])
    }

    /// POD, `__DATA__` and heredoc bodies: every token, trivia included.
    ///
    /// Dropping the newline between `__DATA__` and its contents would join them
    /// into one line, and the result would not even re-parse the same way.
    fn verbatim(&mut self, node: &SyntaxNode) -> Doc {
        let parts = node
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .map(Doc::Raw)
            .collect();
        Doc::concat(parts)
    }

    /// A block. Control-structure blocks always break; a `map`/`sub`/`do` block
    /// may stay on one line.
    fn block(&mut self, node: &SyntaxNode) -> Doc {
        let statements: Vec<SyntaxNode> = node.children().collect();
        let flat = self.block_can_be_flat(node, &statements);

        let mut body = Vec::new();
        if flat {
            for statement in &statements {
                body.push(self.node(statement));
            }
        } else {
            self.statements_into(node, &mut body);
        }

        // Error recovery can leave a block without one or both braces; emit what
        // is there rather than assuming a shape the tree does not have.
        let open = brace(node, T!["{"], false).map(Doc::Token);
        let close = brace(node, T!["}"], true).map(Doc::Token);

        if flat {
            let mut parts = Vec::new();
            parts.extend(open);
            parts.push(Doc::Space);
            parts.push(Doc::concat(body));
            if close.is_some() {
                parts.push(Doc::Space);
            }
            parts.extend(close);
            return Doc::group(false, Doc::concat(parts));
        }

        let mut parts = Vec::new();
        parts.extend(open);
        parts.push(Doc::HardLine);
        if !body.is_empty() {
            parts.push(Doc::indent(Doc::concat(body)));
        }
        parts.extend(close);
        Doc::group(true, Doc::concat(parts))
    }

    /// The single rule that replaces `is_simple_block`'s seven rejections plus
    /// its memoisation plus the `suppress_newlines` flag that leaked past both.
    fn block_can_be_flat(&self, node: &SyntaxNode, statements: &[SyntaxNode]) -> bool {
        if statements.len() != 1 {
            return false;
        }
        if !self.options.allow_single_line_blocks {
            return false;
        }
        // A block belonging to a control structure always breaks (NEWLINE-2).
        if node
            .parent()
            .map(|parent| parent.node_kind())
            .is_some_and(|kind| {
                matches!(
                    kind,
                    NodeKind::IF_STMT
                        | NodeKind::LOOP_STMT
                        | NodeKind::ELSIF_CLAUSE
                        | NodeKind::ELSE_CLAUSE
                        | NodeKind::TRY_STMT
                        | NodeKind::CATCH_CLAUSE
                        | NodeKind::FINALLY_CLAUSE
                        | NodeKind::GIVEN_STMT
                        | NodeKind::WHEN_CLAUSE
                        | NodeKind::DEFAULT_CLAUSE
                        | NodeKind::SUB_DEF
                        | NodeKind::PHASE_BLOCK
                        | NodeKind::BLOCK_STMT
                )
            })
        {
            return false;
        }
        // A statement that was written across lines stays across lines, and a
        // comment anywhere forces the block open.
        if node.text().to_string().contains('\n') {
            return false;
        }
        !node
            .descendants_with_tokens()
            .filter_map(|child| child.into_token())
            .any(|token| token.token_kind() == TokenKind::COMMENT)
    }

    /// A bracketed group: parentheses, an anonymous array or an anonymous hash.
    ///
    /// Broken exactly when the source put a newline straight after the opening
    /// bracket (formatting.md INDENT-2). That rule is stable under
    /// re-formatting, because a broken group's own output has the newline there
    /// (ADR 0008 §6, I2).
    fn delimited(&mut self, node: &SyntaxNode, open: TokenKind, close: TokenKind) -> Doc {
        let opening = node
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .find(|token| token.token_kind() == open);
        let closing = node
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .filter(|token| token.token_kind() == close)
            .last();

        let broken = opening
            .as_ref()
            .is_some_and(|token| self.newline_follows(token));

        let is_hash = open == T!["{"];
        if is_hash {
            self.fat_comma_depth = self.fat_comma_depth.saturating_add(1);
        }

        let inner: Vec<Doc> = node
            .children()
            .map(|child| self.list_items(&child, broken))
            .collect();

        if is_hash {
            self.fat_comma_depth = self.fat_comma_depth.saturating_sub(1);
        }

        let mut parts = Vec::new();
        if let Some(token) = opening {
            parts.push(Doc::Token(token));
        }

        let body = Doc::concat(inner);
        if body.is_nil() {
            if let Some(token) = closing {
                parts.push(Doc::Token(token));
            }
            return Doc::group(false, Doc::concat(parts));
        }

        if broken {
            parts.push(Doc::indent(Doc::concat(vec![Doc::SoftLine, body])));
            parts.push(Doc::SoftLine);
        } else {
            if is_hash {
                parts.push(Doc::Space);
            }
            parts.push(body);
            if is_hash {
                parts.push(Doc::Space);
            }
        }
        if let Some(token) = closing {
            parts.push(Doc::Token(token));
        }
        Doc::group(broken, Doc::concat(parts))
    }

    /// The elements of a list, one per line when the group is broken.
    fn list_items(&mut self, list: &SyntaxNode, broken: bool) -> Doc {
        if list.node_kind() != NodeKind::LIST_EXPR {
            return self.node(list);
        }

        let mut parts = Vec::new();
        for child in list.children_with_tokens() {
            match child {
                SyntaxElement::Node(node) => {
                    if let Some(shape) = shape_key(&node) {
                        parts.push(Doc::Shape(shape));
                    }
                    parts.push(self.node(&node));
                }
                SyntaxElement::Token(token) if token.token_kind().is_trivia() => {}
                SyntaxElement::Token(token) => {
                    // `=>` joins a key to its value; only `,` ends an element.
                    let ends_element = token.token_kind() == T![","];
                    if token.token_kind() == T!["=>"] {
                        parts.push(Doc::Anchor(AnchorClass::FatComma(self.fat_comma_depth)));
                        parts.push(Doc::Space);
                    }
                    let last = token.next_sibling_or_token().is_none()
                        || token
                            .siblings_with_tokens(rowan::Direction::Next)
                            .skip(1)
                            .all(|sibling| {
                                sibling
                                    .as_token()
                                    .is_some_and(|token| token.token_kind().is_trivia())
                            });
                    parts.push(Doc::Token(token));
                    if ends_element && !last {
                        parts.push(if broken { Doc::Line } else { Doc::Space });
                    } else if !ends_element {
                        parts.push(Doc::Space);
                    }
                }
            }
        }
        Doc::concat(parts)
    }

    fn newline_follows(&self, token: &SyntaxToken) -> bool {
        let mut cursor = token.next_sibling_or_token();
        while let Some(element) = cursor {
            match element.as_token() {
                Some(next) if next.token_kind() == TokenKind::NEWLINE => return true,
                Some(next) if next.token_kind() == TokenKind::WHITESPACE => {
                    cursor = next.next_sibling_or_token();
                }
                _ => return false,
            }
        }
        false
    }
}

fn is_quote_like_node(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Q_EXPR
            | NodeKind::QQ_EXPR
            | NodeKind::QX_EXPR
            | NodeKind::QW_EXPR
            | NodeKind::M_EXPR
            | NodeKind::QR_EXPR
            | NodeKind::S_EXPR
            | NodeKind::TR_EXPR
    )
}

fn shape_key(node: &SyntaxNode) -> Option<ShapeKey> {
    let kind = node.node_kind();
    if !matches!(kind, NodeKind::EXPR_STMT | NodeKind::VAR_DECL_STMT) {
        return None;
    }
    let declaration = node
        .descendants()
        .find(|child| child.node_kind() == NodeKind::VAR_DECL)
        .and_then(|decl| first_token(&decl))
        .map(|token| token.token_kind());
    let list_assignment = node
        .descendants()
        .filter(|child| child.node_kind() == NodeKind::DECL_TARGET)
        .any(|target| {
            target
                .first_token()
                .is_some_and(|token| token.text() == "(")
        });

    Some(ShapeKey {
        statement: kind,
        declaration,
        list_assignment,
    })
}

fn first_token_of(element: &SyntaxElement) -> Option<SyntaxToken> {
    match element {
        SyntaxElement::Token(token) => Some(token.clone()),
        SyntaxElement::Node(node) => first_token(node),
    }
}

fn last_token_of(element: &SyntaxElement) -> Option<SyntaxToken> {
    match element {
        SyntaxElement::Token(token) => Some(token.clone()),
        SyntaxElement::Node(node) => last_token(node),
    }
}

fn first_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| !token.token_kind().is_trivia())
}

fn last_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|child| child.into_token())
        .filter(|token| !token.token_kind().is_trivia())
        .last()
}

fn brace(node: &SyntaxNode, kind: TokenKind, last: bool) -> Option<SyntaxToken> {
    let mut matching = node
        .children_with_tokens()
        .filter_map(|child| child.into_token())
        .filter(|token| token.token_kind() == kind);
    if last {
        matching.last()
    } else {
        matching.next()
    }
}
