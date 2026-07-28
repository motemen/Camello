//! CST → Doc (ADR 0008 §3).
//!
//! Every layout decision is made here. In particular the flat-or-broken state of
//! each group is decided from the source, once, at the point the group is built
//! — replacing the seven separate "does the source have a newline here"
//! predicates the old formatter had scattered across five files.

use rowan::TextSize;

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
    /// Where every comment in the file starts, ascending.
    ///
    /// "Does this node contain a comment" is asked once per group and once per
    /// candidate flat block. Answering it by walking the node is a tree walk
    /// inside a tree walk; answering it by binary search over this is a
    /// comparison. Built once, in [`Self::file`].
    comment_starts: Vec<TextSize>,
}

impl<'a> Builder<'a> {
    pub fn new(trivia: &'a TriviaMap, options: &'a FormatterOptions) -> Self {
        Self {
            trivia,
            options,
            fat_comma_depth: 0,
            comment_starts: Vec::new(),
        }
    }

    pub fn file(&mut self, root: &SyntaxNode) -> Doc {
        self.comment_starts = root
            .descendants_with_tokens()
            .filter_map(|child| child.into_token())
            .filter(|token| token.token_kind() == TokenKind::COMMENT)
            .map(|token| token.text_range().start())
            .collect();

        let mut parts = Vec::new();
        self.statements_into(root, &mut parts);
        // A file ends with exactly one newline.
        parts.push(Doc::HardLine);
        Doc::concat(parts)
    }

    /// Is there a comment anywhere inside this node?
    fn contains_comment(&self, node: &SyntaxNode) -> bool {
        let range = node.text_range();
        let index = self
            .comment_starts
            .partition_point(|&start| start < range.start());
        self.comment_starts
            .get(index)
            .is_some_and(|&start| start < range.end())
    }

    /// The statements of a root or a block, plus anything sitting between them.
    ///
    /// A heredoc body is a token, not a statement: it lands wherever the line it
    /// starts on falls (ADR 0007 §7), which is between two statements. Walking
    /// only the child *nodes* would drop it.
    fn statements_into(&mut self, node: &SyntaxNode, parts: &mut Vec<Doc>) {
        for child in node.children_with_tokens() {
            match child {
                SyntaxElement::Node(statement) => {
                    // formatting.md BLANK_LINE-1: definitions and phase blocks
                    // stand apart from the code around them. The renderer drops
                    // a blank line at the start of the output and straight after
                    // an opening brace, so these are pushed unconditionally.
                    let separated = wants_surrounding_blank_lines(&statement);
                    if separated || wants_preceding_blank_line(&statement) {
                        parts.push(Doc::BlankLine);
                    }
                    self.statement_into(&statement, parts);
                    if separated && statement.next_sibling().is_some() {
                        parts.push(Doc::BlankLine);
                    }
                }
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
        parts.push(Doc::HardLine);
    }

    /// Own-line comments and blank lines attached to a token.
    ///
    /// Every comment in the output is emitted from here or from
    /// [`Self::trailing_comment`] — two functions in one file, against the old
    /// formatter's two unrelated paths in two files, only one of which honoured
    /// the spacing option.
    fn leading_docs(&mut self, token: &SyntaxToken) -> Doc {
        let trivia = self.trivia.of(token.text_range());
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
        let trivia = self.trivia.of(token.text_range());
        for item in &trivia.trailing {
            if item.kind == TokenKind::COMMENT {
                return Doc::Comment(item.text.clone(), Placement::Trailing);
            }
        }
        Doc::Nil
    }

    fn node(&mut self, node: &SyntaxNode) -> Doc {
        match node.node_kind() {
            NodeKind::BLOCK => self.block(node),
            NodeKind::POD | NodeKind::DATA_SECTION => self.verbatim(node),
            NodeKind::HEREDOC_BODY => self.verbatim(node),
            NodeKind::ERROR => self.error(node),
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
        // `->` binds tight on both sides, and so does a postfix dereference,
        // which is an arrow with its target glued on.
        if before == Some(T!["->"]) || after == Some(T!["->"]) {
            return false;
        }
        if after.is_some_and(is_postfix_deref) {
            return false;
        }
        // A sigil is part of the name that follows it — except a signature
        // placeholder, which is a bare `$` holding a slot. `$ = 1` must not
        // close up into `$= 1`, which reads as the `$=` variable.
        if before.is_some_and(TokenKind::is_sigil) {
            return parent == Some(NodeKind::SIGNATURE_PARAM);
        }

        // An argument list hugs the name it belongs to: `foo(1)`, not `foo (1)`.
        // The parenthesis is inside ARG_LIST, so the test is on the node.
        if next.as_node().is_some_and(|node| {
            matches!(node.node_kind(), NodeKind::ARG_LIST | NodeKind::ATTR_ARGS)
        }) {
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
        // `$h->{key}`, `$h{a}{b}`, `&{$code}` — braces hug their contents.
        if matches!(
            parent,
            Some(
                NodeKind::HASH_SUBSCRIPT_EXPR
                    | NodeKind::ARRAY_SUBSCRIPT_EXPR
                    | NodeKind::BLOCK_DEREF_EXPR
                    | NodeKind::POSTFIX_ARRAY_SLICE_EXPR
                    | NodeKind::POSTFIX_HASH_SLICE_EXPR
            )
        ) && (matches!(after, Some(T!["{"] | T!["}"] | T!["["] | T!["]"]))
            || matches!(before, Some(T!["{"] | T!["["])))
        {
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
        // The same, keyed on the token rather than on the node that should be
        // holding it. A DELIMITER exists only as part of a quote-like run, so it
        // is tight wherever it is found — including inside an ERROR node, where
        // the run's own node is missing and the rule above cannot fire. That is
        // how `s/xx\z//;` misparsed became `s/xx \ z /  /;`, and then grew by
        // two spaces on every further pass.
        if before == Some(TokenKind::DELIMITER) || after == Some(TokenKind::DELIMITER) {
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
            .map(|token| self.trivia.of(token.text_range()))
            .is_some_and(|trivia| trivia.trailing.iter().any(is_newline));
        if after_previous {
            return true;
        }

        first_token_of(next)
            .map(|token| self.trivia.of(token.text_range()))
            .is_some_and(|trivia| trivia.leading.iter().any(is_newline))
    }

    fn token(&mut self, token: &SyntaxToken) -> Doc {
        let kind = token.token_kind();
        let text = if kind.is_verbatim() {
            Doc::Raw(token.clone())
        } else {
            Doc::Token(token.clone())
        };

        // A quote-like operator is scanned as one atomic run (ADR 0005 §3), so
        // its parts are one lexical unit: a comment can precede the run or
        // follow it, and there is nowhere in between for one to be. Only the
        // outermost tokens ask, which leaves no interior token able to claim a
        // comment and emit it a second time inside the literal.
        let (asks_leading, asks_trailing) = run_edges(token);

        let leading = if asks_leading {
            self.leading_docs(token)
        } else {
            Doc::Nil
        };
        // A comment sitting between a header and its brace belongs after the
        // brace, because the brace does not move (formatting.md NEWLINE-2).
        // `block` emits it there.
        let trailing = if !asks_trailing || brace_follows(token) {
            Doc::Nil
        } else {
            self.trailing_comment(token)
        };
        if leading.is_nil() && trailing.is_nil() {
            return text;
        }
        Doc::concat(vec![leading, text, trailing])
    }

    /// Whatever the parser could not read, exactly as it was written.
    ///
    /// Every layout rule the formatter has is a rule about a construct it
    /// recognised. Inside an ERROR node it recognised nothing, so applying them
    /// is applying rules for a shape that is not there: a quote-like run whose
    /// node is missing gets spaced out at the delimiters, and the spaces are
    /// inside the literal on the next pass. Copying the source is the one
    /// behaviour that cannot make a file worse than it was.
    fn error(&mut self, node: &SyntaxNode) -> Doc {
        let parts = node
            .descendants_with_tokens()
            .filter_map(|child| child.into_token())
            .map(Doc::Raw)
            .collect();
        Doc::concat(parts)
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

    /// The comment that sat before this block's brace, to be emitted after it.
    ///
    /// The same predicate that suppressed it decides whether it is here, so the
    /// two cannot disagree about who owns it. They did: for a bare block after
    /// another statement, `brace_follows` claimed the previous statement's
    /// trailing comment and this re-emitted it on the brace's line.
    fn comment_before_brace(&mut self, node: &SyntaxNode) -> Doc {
        let Some(first) = first_token(node) else {
            return Doc::Nil;
        };
        let mut previous = first.prev_token();
        while let Some(token) = previous {
            if !token.token_kind().is_trivia() {
                if !brace_follows(&token) {
                    return Doc::Nil;
                }
                return self.trailing_comment(&token);
            }
            previous = token.prev_token();
        }
        Doc::Nil
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

        let header_comment = self.comment_before_brace(node);

        // Error recovery can leave a block without one or both braces; emit what
        // is there rather than assuming a shape the tree does not have.
        let open = brace(node, T!["{"], false).map(|token| self.token(&token));
        let close = brace(node, T!["}"], true).map(|token| self.token(&token));

        if flat {
            let mut parts = Vec::new();
            parts.extend(open);
            parts.push(header_comment);
            let empty = body.is_empty();
            parts.push(Doc::Space);
            if !empty {
                parts.push(Doc::concat(body));
                parts.push(Doc::Space);
            }
            parts.extend(close);
            return Doc::group(false, Doc::concat(parts));
        }

        let mut parts = Vec::new();
        parts.extend(open);
        parts.push(header_comment);
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
        // An empty block is `{ }` wherever it appears; there is nothing to put
        // on a line of its own.
        if statements.is_empty() {
            return !node
                .descendants_with_tokens()
                .filter_map(|child| child.into_token())
                .any(|token| token.token_kind() == TokenKind::COMMENT);
        }
        if statements.len() != 1 {
            return false;
        }
        if !self.options.allow_single_line_blocks {
            return false;
        }
        // A control structure's block always breaks (formatting.md NEWLINE-2).
        // `sub`, `do`, `map` and `try` blocks are not control structures: they
        // may hold a single value and stay on one line (ADR 0008 §3).
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
                        | NodeKind::GIVEN_STMT
                        | NodeKind::WHEN_CLAUSE
                        | NodeKind::DEFAULT_CLAUSE
                        | NodeKind::PHASE_BLOCK
                        | NodeKind::BLOCK_STMT
                )
            })
        {
            return false;
        }
        // A statement that was written across lines stays across lines, and a
        // statement that ends in `;` reads as a body rather than a value
        // (ADR 0008 §3: single statement, no semicolon, no comment, no source
        // newline).
        if node.text().to_string().contains('\n') {
            return false;
        }
        if statements[0]
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .any(|token| token.token_kind() == T![";"])
        {
            return false;
        }
        if node
            .descendants_with_tokens()
            .filter_map(|child| child.into_token())
            .any(|token| token.token_kind() == TokenKind::COMMENT)
        {
            return false;
        }

        // A flat group must contain no hard line break, so a block that holds a
        // block that has to break cannot itself be flat. Keeping this a property
        // of the structure is what removes the old `suppress_newlines` flag and
        // the leaks it caused (F2).
        node.descendants()
            .skip(1)
            .filter(|child| child.node_kind() == NodeKind::BLOCK)
            .all(|child| {
                let statements: Vec<SyntaxNode> = child.children().collect();
                self.block_can_be_flat(&child, &statements)
            })
    }

    /// A bracketed group: parentheses, an anonymous array or an anonymous hash.
    ///
    /// Broken when the source put a newline straight after the opening bracket
    /// (formatting.md INDENT-2) — a rule stable under re-formatting, because a
    /// broken group's own output has the newline there (ADR 0008 §6, I2) — or
    /// when the group holds a comment.
    ///
    /// The second half is not a taste judgement. A comment runs to end of line,
    /// so it *is* a hard line break; a flat group is by definition one that
    /// contains none. Leaving it out is how `my %h = ( # c\n a => 1,\n);`
    /// formatted to `my %h = ( # ca => 1,);`, with the entry commented out of
    /// existence.
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

        let broken = self.contains_comment(node)
            || opening
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
        if let Some(token) = &opening {
            parts.push(self.token(token));
        }

        let body = Doc::concat(inner);
        if body.is_nil() {
            if let Some(token) = &closing {
                parts.push(self.token(token));
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
        if let Some(token) = &closing {
            parts.push(self.token(token));
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
                    let value_on_next_line =
                        token.token_kind() == T!["=>"] && self.newline_follows(&token);
                    let last = token
                        .siblings_with_tokens(rowan::Direction::Next)
                        .skip(1)
                        .all(|sibling| {
                            sibling
                                .as_token()
                                .is_some_and(|token| token.token_kind().is_trivia())
                        });
                    let user_break = self.newline_follows(&token);
                    parts.push(self.token(&token));
                    if value_on_next_line {
                        parts.push(Doc::UserLine { broken: true });
                    } else if ends_element && !last {
                        // A broken group puts one element per line; a flat one
                        // still keeps a line break the user put here
                        // (formatting.md POLICY-4).
                        parts.push(if broken {
                            Doc::Line
                        } else if user_break {
                            Doc::UserLine { broken: true }
                        } else {
                            Doc::Space
                        });
                    } else if !ends_element {
                        parts.push(Doc::Space);
                    }
                }
            }
        }
        Doc::concat(parts)
    }

    /// Does the rest of this token's line hold no more code?
    ///
    /// A comment counts as reaching the newline, because it runs to one: there
    /// is no way to put anything after `# c` on its line and have it still be
    /// code. Reading a COMMENT as "not a newline" is what let the formatter
    /// believe a group with a comment in it could stay flat.
    fn newline_follows(&self, token: &SyntaxToken) -> bool {
        let mut cursor = token.next_sibling_or_token();
        while let Some(element) = cursor {
            match element.as_token() {
                Some(next) if next.token_kind() == TokenKind::NEWLINE => return true,
                Some(next) if next.token_kind() == TokenKind::COMMENT => return true,
                Some(next) if next.token_kind() == TokenKind::WHITESPACE => {
                    cursor = next.next_sibling_or_token();
                }
                _ => return false,
            }
        }
        false
    }
}

/// Whether this token may carry leading and trailing trivia of its own.
///
/// Both, unless it is inside an atomic quote-like run, where only the first
/// token of the run can be preceded by a comment and only the last can be
/// followed by one.
fn run_edges(token: &SyntaxToken) -> (bool, bool) {
    let Some(parent) = token.parent() else {
        return (true, true);
    };
    if !is_quote_like_node(parent.node_kind()) {
        return (true, true);
    }
    let mut children = parent
        .children_with_tokens()
        .filter_map(|child| child.into_token());
    let first = children.next();
    let last = parent
        .children_with_tokens()
        .filter_map(|child| child.into_token())
        .last();
    (first.as_ref() == Some(token), last.as_ref() == Some(token))
}

/// Is the next thing after this token the opening brace of a block this token
/// is part of the header of?
///
/// Both halves matter. The brace does not move (formatting.md NEWLINE-2), so a
/// comment written before it comes out after it — but only when the comment was
/// written *inside the construct*: `if ($x) # why` belongs to the `if`, whereas
/// the trailing comment of `my $x = 1;` before a bare block belongs to the
/// statement that ended. Claiming the second moved a comment across a statement
/// boundary and, with the comment then emitted from two places, put two of them
/// on the brace's line.
fn brace_follows(token: &SyntaxToken) -> bool {
    let mut next = token.next_token();
    while let Some(candidate) = next {
        if candidate.token_kind().is_trivia() {
            next = candidate.next_token();
            continue;
        }
        if candidate.token_kind() != T!["{"] {
            return false;
        }
        let Some(block) = candidate.parent() else {
            return false;
        };
        if block.node_kind() != NodeKind::BLOCK {
            return false;
        }
        let Some(header) = block.parent() else {
            return false;
        };
        return token.parent_ancestors().any(|ancestor| ancestor == header);
    }
    false
}

/// Statements that get a blank line on each side (formatting.md BLANK_LINE-1).
fn wants_surrounding_blank_lines(node: &SyntaxNode) -> bool {
    matches!(node.node_kind(), NodeKind::SUB_DEF | NodeKind::PHASE_BLOCK)
}

/// Statements that get a blank line before them only.
fn wants_preceding_blank_line(node: &SyntaxNode) -> bool {
    matches!(node.node_kind(), NodeKind::POD | NodeKind::DATA_SECTION)
}

fn is_postfix_deref(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::POSTFIX_DEREF_ARRAY
            | TokenKind::POSTFIX_DEREF_HASH
            | TokenKind::POSTFIX_DEREF_SCALAR
            | TokenKind::POSTFIX_DEREF_ARRAY_LAST_INDEX
            | TokenKind::POSTFIX_DEREF_CODE
            | TokenKind::POSTFIX_DEREF_GLOB
    )
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
    let declares = node
        .descendants()
        .any(|child| child.node_kind() == NodeKind::VAR_DECL);
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
        declares,
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
