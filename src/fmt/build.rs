//! CST → Doc (the formatter contract).
//!
//! Every layout decision is made here. In particular the flat-or-broken state of
//! each group is decided from the source, once, at the point the group is built
//! — replacing the seven separate "does the source have a newline here"
//! predicates the old formatter had scattered across five files.

use rowan::TextSize;
use unicode_width::UnicodeWidthStr;

use crate::hash::OffsetMap;
use crate::lang::{
    NodeExt, NodeKind, SyntaxElement, SyntaxNode, SyntaxToken, TokenExt, TokenKind, T,
};
use crate::parse::trivia::TriviaMap;

use super::doc::{AnchorClass, Doc, Placement, ShapeKey};
use super::{DelimiterSpacing, FormatterOptions};

pub struct Builder<'a> {
    trivia: &'a TriviaMap,
    options: &'a FormatterOptions,
    /// Nesting depth of `=>`, so that an inner hash aligns separately from the
    /// one containing it (the formatter contract).
    fat_comma_depth: u8,
    /// Where every comment in the file starts, ascending.
    ///
    /// "Does this node contain a comment" is asked once per group and once per
    /// candidate flat block. Answering it by walking the node is a tree walk
    /// inside a tree walk; answering it by binary search over this is a
    /// comparison. Built once, in [`Self::file`].
    comment_starts: Vec<TextSize>,
    /// Where every line terminator in the file is, ascending. Same reason as
    /// `comment_starts`: "was this written across lines" is asked once per
    /// candidate flat block, and the alternative was allocating the node's whole
    /// text to search it.
    newline_starts: Vec<TextSize>,
    /// Where every heredoc marker is, and where every body's terminator is,
    /// both ascending. Used to count how many bodies a statement is still owed.
    heredoc_marker_starts: Vec<TextSize>,
    heredoc_end_starts: Vec<TextSize>,
    /// Where every token that a block's brace follows starts, ascending.
    ///
    /// "Does a block open right after this token" is asked of every token in the
    /// file, and answering it from the token walked up to the block's header and
    /// back down. It is a property of the pair, so it is read off the same pass
    /// as the rest: see [`Self::mark_positions`].
    brace_headers: Vec<TextSize>,
    /// Answers already given by [`Self::block_can_be_flat`], keyed on where the
    /// block starts. A block's answer depends on the blocks inside it, so
    /// without this the recursion re-derives every level from every level above.
    flat_blocks: OffsetMap<TextSize, bool>,
}

impl<'a> Builder<'a> {
    pub fn new(trivia: &'a TriviaMap, options: &'a FormatterOptions) -> Self {
        Self {
            trivia,
            options,
            fat_comma_depth: 0,
            comment_starts: Vec::new(),
            newline_starts: Vec::new(),
            heredoc_marker_starts: Vec::new(),
            heredoc_end_starts: Vec::new(),
            brace_headers: Vec::new(),
            flat_blocks: OffsetMap::default(),
        }
    }

    pub fn file(&mut self, root: &SyntaxNode) -> Doc {
        self.mark_positions(&root.green(), 0, None, &mut None);

        let mut parts = Vec::new();
        self.statements_into(root, &mut parts);
        parts.push(self.end_of_file_docs());
        // A file ends with exactly one newline.
        parts.push(Doc::HardLine);
        Doc::concat(parts)
    }

    /// Where the comments, the heredoc markers and the line ends are, collected
    /// in one pass before anything is built.
    ///
    /// Walked over the green tree rather than with a cursor: a cursor allocates
    /// a node of its own for every step it takes, and this step visits every
    /// token in the file to ask three questions of it. The green nodes carry no
    /// position, so the offset is carried down instead.
    fn mark_positions(
        &mut self,
        node: &rowan::GreenNodeData,
        start: u32,
        // Where the node containing this one begins and ends. A block's brace
        // belongs to the token before it only when that token was written
        // inside the block's header, which is this node's parent.
        parent: Option<(u32, u32)>,
        // The last token seen that was not trivia, wherever in the file it was.
        last_code: &mut Option<u32>,
    ) {
        let kind = crate::lang::SyntaxKind(node.kind().0).as_node();
        let extent = Some((start, start + u32::from(node.text_len())));
        let mut offset = start;
        for child in node.children() {
            match child {
                rowan::NodeOrToken::Node(node) => {
                    self.mark_positions(node, offset, extent, last_code);
                    offset += u32::from(node.text_len());
                }
                rowan::NodeOrToken::Token(token) => {
                    let token_kind = crate::lang::SyntaxKind(token.kind().0).as_token();
                    match token_kind {
                        Some(TokenKind::COMMENT) => {
                            self.comment_starts.push(TextSize::from(offset));
                        }
                        Some(TokenKind::HEREDOC_START) => {
                            self.heredoc_marker_starts.push(TextSize::from(offset));
                        }
                        Some(TokenKind::HEREDOC_END) => {
                            self.heredoc_end_starts.push(TextSize::from(offset));
                        }
                        _ => {}
                    }
                    if let Some(token_kind) = token_kind {
                        if !token_kind.is_trivia() {
                            if kind == Some(NodeKind::BLOCK) && token_kind == T!["{"] {
                                if let (Some((from, to)), Some(code)) = (parent, *last_code) {
                                    if (from..to).contains(&code) {
                                        self.brace_headers.push(TextSize::from(code));
                                    }
                                }
                            }
                            *last_code = Some(offset);
                        }
                    }
                    // Most tokens hold no newline at all; the ones that do are
                    // heredoc bodies, POD and multi-line literals.
                    for (index, _) in token.text().match_indices('\n') {
                        self.newline_starts
                            .push(TextSize::from(offset + index as u32));
                    }
                    offset += u32::from(token.text_len());
                }
            }
        }
    }

    /// Is the next thing after this token the opening brace of a block this
    /// token is part of the header of?
    ///
    /// Both halves matter. The brace does not move (docs/formatting.md
    /// NEWLINE-2), so a comment written before it comes out after it — but only
    /// when the comment was written *inside the construct*: `if ($x) # why`
    /// belongs to the `if`, whereas the trailing comment of `my $x = 1;` before
    /// a bare block belongs to the statement that ended. Claiming the second
    /// moved a comment across a statement boundary and, with the comment then
    /// emitted from two places, put two of them on the brace's line.
    ///
    /// Read off [`Self::mark_positions`], which sees both halves at once: the
    /// walk this replaced climbed from the token to the block's header and back
    /// down again, once for every token in the file.
    fn brace_follows(&self, token: &SyntaxToken) -> bool {
        self.brace_headers
            .binary_search(&token.text_range().start())
            .is_ok()
    }

    /// Own-line comments written after the last statement of the file.
    ///
    /// Trivia belongs to the token that follows it (the trivia model), and here
    /// there is none; the map keeps such a run under its own name, and this is
    /// the one place it is emitted. `feature.pm` ends with
    /// `# ex: set ro ft=perl:` and lost it.
    fn end_of_file_docs(&self) -> Doc {
        let mut parts = Vec::new();
        let mut items = self.trivia.at_end().iter().peekable();
        while let Some(item) = items.next() {
            match item.kind {
                TokenKind::COMMENT => {
                    parts.push(Doc::Comment(item.text.clone(), Placement::OwnLine));
                    parts.push(Doc::HardLine);
                    // The newline ending the comment's own line is not a blank
                    // line, the same reading `leading_docs` takes.
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

    /// Is there a comment anywhere inside this node?
    fn contains_comment(&self, node: &SyntaxNode) -> bool {
        contains(&self.comment_starts, node)
    }

    /// Was this node written across more than one line?
    fn contains_newline(&self, node: &SyntaxNode) -> bool {
        contains(&self.newline_starts, node)
    }

    /// The statements of a root or a block, plus anything sitting between them.
    ///
    /// A heredoc body is a token, not a statement: it lands wherever the line it
    /// starts on falls (the parser contract), which is between two statements. Walking
    /// only the child *nodes* would drop it.
    fn statements_into(&mut self, node: &SyntaxNode, parts: &mut Vec<Doc>) {
        // Heredoc markers whose bodies have not arrived yet. A body begins on
        // the line after the one its marker is on (the parser contract), so while any
        // are outstanding the statements have to stay on that line — putting the
        // next one on a line of its own makes it the body.
        // `eval <<EOT; die $@ if $@;` in `URI::data` did exactly that: the
        // `die` became the first line of the string `eval` was handed.
        let mut owed = 0usize;
        // A definition's trailing blank line, held back because a `;` written on
        // its last line comes first. The definition ends at that semicolon, so
        // the blank line goes after it.
        let mut blank_line_owed = false;

        for child in node.children_with_tokens() {
            match child {
                SyntaxElement::Node(statement) => {
                    // docs/formatting.md BLANK_LINE-1: definitions and phase blocks
                    // stand apart from the code around them. The renderer drops
                    // a blank line at the start of the output and straight after
                    // an opening brace, so these are pushed unconditionally.
                    let separated = wants_surrounding_blank_lines(&statement);
                    if owed == 0 && (separated || wants_preceding_blank_line(&statement)) {
                        parts.push(Doc::BlankLine);
                    }
                    self.statement_into(&statement, parts);
                    owed += self.heredoc_markers_in(&statement);
                    // Nothing at all before a `;` that was written on this
                    // statement's line: not a line break, and not the blank line
                    // a definition asks for either, which would land between the
                    // brace and the semicolon it belongs to.
                    let hugged = owed == 0 && self.empty_statement_hugs(&statement);
                    parts.push(if owed > 0 {
                        Doc::Space
                    } else if hugged {
                        Doc::Nil
                    } else {
                        Doc::HardLine
                    });
                    if std::mem::take(&mut blank_line_owed) && statement.next_sibling().is_some() {
                        parts.push(Doc::BlankLine);
                    }
                    // A blank line here would land between the marker's line and
                    // the body, which is to say *inside* the body — and the body
                    // would gain a line on every pass.
                    if owed == 0 && separated && statement.next_sibling().is_some() {
                        if hugged {
                            blank_line_owed = true;
                        } else {
                            parts.push(Doc::BlankLine);
                        }
                    }
                }
                SyntaxElement::Token(token) if token.token_kind().is_heredoc_body() => {
                    if token.token_kind() != TokenKind::HEREDOC_CONTENT {
                        owed = owed.saturating_sub(1);
                    }
                    parts.push(self.token(&token));
                }
                SyntaxElement::Token(_) => {}
            }
        }
    }

    /// Does the next statement consist of a `;` written on this statement's own
    /// line?
    ///
    /// `package Foo { ... };` and `sub f { }` followed by a stray `;` both put a
    /// semicolon after a closing brace, and the parser reads it as an EMPTY_STMT
    /// of its own (the parser contract) — correctly, because that is what it is. On a
    /// line of its own it reads as a statement someone forgot to delete; on the
    /// brace's line it reads as what the writer wrote. Where the user put it is
    /// the only evidence of which one it is, and so it decides
    /// (docs/formatting.md POLICY-4).
    fn empty_statement_hugs(&self, statement: &SyntaxNode) -> bool {
        let Some(next) = statement.next_sibling() else {
            return false;
        };
        if next.node_kind() != NodeKind::EMPTY_STMT {
            return false;
        }
        !self.has_user_newline_between(
            &SyntaxElement::Node(statement.clone()),
            &SyntaxElement::Node(next),
        )
    }

    /// How many heredoc bodies this statement still owes to the line it is on.
    ///
    /// Markers it opened, less the bodies that landed inside it: a nested block
    /// takes its own, and counting those again would hold the enclosing block's
    /// closing brace on the same line as its last statement.
    fn heredoc_markers_in(&self, node: &SyntaxNode) -> usize {
        let count = |offsets: &[TextSize]| {
            let range = node.text_range();
            let from = offsets.partition_point(|&start| start < range.start());
            let to = offsets.partition_point(|&start| start < range.end());
            to - from
        };
        count(&self.heredoc_marker_starts).saturating_sub(count(&self.heredoc_end_starts))
    }

    /// A statement. Its comments and blank lines come from its tokens, which is
    /// where every comment in the output comes from (the formatter contract).
    fn statement_into(&mut self, node: &SyntaxNode, parts: &mut Vec<Doc>) {
        // Declared for every statement, including the ones with no shape of
        // their own: it is what ends the previous statement's declaration, and
        // the lines a statement breaks across all carry what it declared.
        parts.push(Doc::Shape(shape_key(node)));
        parts.push(self.node(node));
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
        // trailing trivia (the trivia model), so every NEWLINE here is a line the
        // user left empty. The renderer collapses runs of them to one
        // (docs/formatting.md BLANK_LINE-3).
        let mut parts = Vec::new();
        let mut items = trivia.leading.iter().peekable();
        // With one exception. A heredoc body is invisible to the parser
        // (the parser contract), so it holds no trivia of its own and the newline that
        // ended its terminator's line has no token to be trailing trivia of: it
        // arrives here, in front of whatever comes next. It is the terminator's
        // line ending, not a line the user left empty — the ones after it are.
        if follows_heredoc_body(token)
            && items
                .peek()
                .is_some_and(|item| item.kind == TokenKind::NEWLINE)
        {
            items.next();
        }
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
        let doc = self.node_body(node);
        // A construct that owns a block written across lines is placed from the
        // line it begins on, which is not the statement's own level once the
        // user has wrapped the line (docs/formatting.md INDENT-4).
        if self.owns_a_broken_block(node) {
            Doc::rooted(doc)
        } else {
            doc
        }
    }

    /// The construct's own document, before it is rooted.
    fn node_body(&mut self, node: &SyntaxNode) -> Doc {
        match node.node_kind() {
            NodeKind::BLOCK => self.block(node),
            NodeKind::POD | NodeKind::DATA_SECTION => self.verbatim(node),
            NodeKind::FORMAT_DECL => self.format_decl(node),
            NodeKind::HEREDOC_BODY => self.verbatim(node),
            NodeKind::ERROR => self.error(node),
            NodeKind::ARG_LIST | NodeKind::PAREN_EXPR => self.delimited(node, T!["("], T![")"]),
            NodeKind::ANON_ARRAY => self.delimited(node, T!["["], T!["]"]),
            NodeKind::ANON_HASH => self.delimited(node, T!["{"], T!["}"]),
            NodeKind::SUBSCRIPT => self.sequence(node),
            NodeKind::BLOCK_DEREF_EXPR => self.deref_block(node),
            NodeKind::LIST_CALL_EXPR => self.list_call(node),
            kind if is_quote_like_node(kind) => self.quote_like(node),
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

    /// `@{ ... }` — a sigil and a brace pair around one expression.
    ///
    /// Not a `delimited` group: that one puts its contents through the list
    /// rules and drops the tokens those do not account for, and here the sigil
    /// and the caret of `${^MATCH}` are exactly such tokens. What is shared is
    /// the shape a group takes when the user wrote a newline after the opening
    /// brace (docs/formatting.md INDENT-2): the contents take one level and the
    /// closing brace comes back to where the construct started. Left to the
    /// ordinary sequence rules both were continuation lines, and `@{\n
    /// $self->list\n}` closed one level in from the `@` that opened it.
    fn deref_block(&mut self, node: &SyntaxNode) -> Doc {
        let children: Vec<SyntaxElement> = node
            .children_with_tokens()
            .filter(|child| {
                !child
                    .as_token()
                    .is_some_and(|token| token.token_kind().is_trivia())
            })
            .collect();
        let brace = |child: &SyntaxElement, kind: TokenKind| {
            child
                .as_token()
                .is_some_and(|token| token.token_kind() == kind)
        };
        let opening = children.iter().position(|child| brace(child, T!["{"]));
        let closing = children.iter().rposition(|child| brace(child, T!["}"]));
        let (Some(opening), Some(closing)) = (opening, closing) else {
            return self.sequence(node);
        };
        let broken = self.contains_comment(node)
            || children[opening]
                .as_token()
                .is_some_and(|token| self.newline_follows(token));
        if !broken || opening + 1 >= closing {
            return self.sequence(node);
        }

        let parent = Some(node.node_kind());
        let mut parts = Vec::new();
        let mut body = Vec::new();
        let mut previous: Option<&SyntaxElement> = None;
        for (index, child) in children.iter().enumerate() {
            // The closing brace comes back to the level the construct began at,
            // but a comment written before it was written inside and stays with
            // the contents.
            let (written_inside, doc) = match child {
                SyntaxElement::Node(child) => (Doc::Nil, self.node(child)),
                SyntaxElement::Token(token) if index == closing => self.closing_delimiter(token),
                SyntaxElement::Token(token) => (Doc::Nil, self.token(token)),
            };
            // Against a brace the group's own line break is the separator; what
            // the user wrote there is the shape, not spacing.
            let separator = match previous {
                Some(_) if index == opening + 1 || index == closing => Vec::new(),
                Some(previous) => self.separator(previous, child, parent),
                None => Vec::new(),
            };
            if index == closing {
                body.push(written_inside);
                parts.push(Doc::indent(Doc::concat(
                    std::iter::once(Doc::HardLine)
                        .chain(body.drain(..))
                        .collect(),
                )));
                parts.push(Doc::HardLine);
            }
            let sink = if index > opening && index < closing {
                &mut body
            } else {
                &mut parts
            };
            sink.extend(separator);
            sink.push(doc);
            previous = Some(child);
        }
        Doc::group(true, Doc::concat(parts))
    }

    /// A bareword call whose continued arguments hang below its first one.
    ///
    /// The hanging offset is the width of the name plus a space, counted from
    /// the line's own indent — so it lands under the first argument only when
    /// the name is what the line starts with. A call written mid-line takes the
    /// ordinary continuation indent instead; measured from an indent the name is
    /// nowhere near, the offset put `bbb` two columns right of its own list.
    fn list_call(&mut self, node: &SyntaxNode) -> Doc {
        let parent = Some(node.node_kind());
        let name = node
            .children()
            .find(|child| child.node_kind() == NodeKind::SUB_NAME);
        let arguments = node
            .children()
            .find(|child| child.node_kind() == NodeKind::LIST_EXPR);
        let list_level = self.takes_the_list_level(node);
        let hanging = name
            .as_ref()
            .zip(arguments.as_ref())
            .and_then(|(name, arguments)| {
                let is_identifier = name
                    .first_token()
                    .is_some_and(|token| token.token_kind() == TokenKind::IDENT);
                let name = SyntaxElement::Node(name.clone());
                let arguments = SyntaxElement::Node(arguments.clone());
                (is_identifier && !self.has_user_newline_between(&name, &arguments))
                    .then(|| name.to_string().width() + 1)
            });
        // A filehandle and a block are placed beside the name rather than in the
        // argument list, so the list has no first argument for anything to hang
        // under — and no level of its own either: `print $fh "a",\n "b"` is one
        // call whose arguments wrap, wherever it is written.
        let placed_beside_the_name = node
            .children()
            .any(|child| matches!(child.node_kind(), NodeKind::FILEHANDLE | NodeKind::BLOCK));
        // A first argument that ends at a brace of its own gives no column to
        // hang under either: `foo {\n 1;\n}` ends its line at the `}` rather
        // than beside the name, and an offset counted from the name put what
        // followed four columns right of the brace that closed it.
        let leading_broken_block = arguments
            .as_ref()
            .and_then(|arguments| arguments.children().next())
            .is_some_and(|first| self.owns_a_broken_block(&first));

        // Zero puts the lines the call swallowed at the level of the list
        // around them — its own hanging column where it has one, and the
        // statement's level where it has none; `None` leaves them to the
        // ordinary continuation indent.
        let offset = if placed_beside_the_name {
            None
        } else if list_level {
            Some(0)
        } else if leading_broken_block {
            None
        } else {
            hanging
        };

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
                SyntaxElement::Node(child)
                    if child.node_kind() == NodeKind::LIST_EXPR && offset.is_some() =>
                {
                    parts.push(Doc::hanging(
                        offset.expect("checked above"),
                        self.node(child),
                    ));
                }
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
        if let Some((class, tail)) = self.anchor_class(next, parent) {
            parts.push(Doc::Anchor(class, tail));
        }

        let wants_space = self.wants_space(previous, next, parent);
        let deferred_terminator = !wants_space
            && next
                .as_token()
                .is_some_and(|token| token.token_kind() == T![";"])
            && self.has_user_newline_between(previous, next);
        if !wants_space && !deferred_terminator {
            return parts;
        }

        // A newline the user put here is kept, and the continuation is indented
        // by the enclosing Indent (the formatter contract) — no separate rule for
        // continuation indent, with no separate branch per syntax shape. A
        // deferred `;` is the tight-spacing exception: `$obj->method\n# why\n;`
        // still needs the break so its comment and terminator remain a
        // continuation, even though the one-line spelling has no space.
        // A block's opening brace is placed by the formatter, not the user
        // (docs/formatting.md NEWLINE-2), so a newline before it is not preserved.
        // A chained keyword goes on the closing brace's line whatever the user
        // wrote (docs/formatting.md NEWLINE-3), so the newline before one is no more
        // preserved than the one before a brace. Keeping it produced `}` and
        // then an indented `else {` with its body at the same column.
        let placed_by_the_formatter = next.as_node().is_some_and(|node| {
            matches!(
                node.node_kind(),
                NodeKind::BLOCK
                    | NodeKind::ELSIF_CLAUSE
                    | NodeKind::ELSE_CLAUSE
                    | NodeKind::CATCH_CLAUSE
                    | NodeKind::FINALLY_CLAUSE
                    | NodeKind::CONTINUE_CLAUSE
            )
        });

        if !placed_by_the_formatter && self.has_user_newline_between(previous, next) {
            // A block written across lines ended that line itself, so the
            // newline after its `}` is not a wrap and takes no continuation
            // indent. `map {\n}\nsort {\n} @array` came back with `sort {`
            // indented and its contents and closing brace not.
            let wraps = !self.ends_its_own_line(previous);
            parts.push(Doc::UserLine {
                broken: true,
                wraps,
            });
        } else if wants_space {
            parts.push(Doc::Space);
        }
        parts
    }

    /// Does this node hold a block that will be written across lines?
    ///
    /// Its own children only. A block deeper than that belongs to a construct
    /// of its own, which is rooted where *it* begins.
    fn owns_a_broken_block(&mut self, node: &SyntaxNode) -> bool {
        node.children()
            .filter(|child| child.node_kind() == NodeKind::BLOCK)
            .any(|block| self.block_breaks(&block))
    }

    /// Will this block be written across lines?
    ///
    /// The memo is consulted before the block's statements are collected: this
    /// is asked of every node that has a block under it, and by then the block
    /// has almost always answered for itself already.
    fn block_breaks(&mut self, block: &SyntaxNode) -> bool {
        if let Some(&flat) = self.flat_blocks.get(&block.text_range().start()) {
            return !flat;
        }
        let statements: Vec<SyntaxNode> = block.children().collect();
        !self.block_can_be_flat(block, &statements)
    }

    /// Does this element finish on a line of its own, at the level the
    /// statement started at?
    ///
    /// A block that cannot be flat does: its closing brace is the last thing on
    /// its line and sits where the construct began.
    fn ends_its_own_line(&mut self, element: &SyntaxElement) -> bool {
        let Some(node) = element.as_node() else {
            return false;
        };
        node.node_kind() == NodeKind::BLOCK && self.block_breaks(node)
    }

    /// The class this token is an alignment point for, and how much of it has to
    /// end at the group's column.
    fn anchor_class(
        &self,
        next: &SyntaxElement,
        parent: Option<NodeKind>,
    ) -> Option<(AnchorClass, usize)> {
        // A `use` line is a table of two columns — the module, then what is
        // taken from it — so what the lines agree on is where the second
        // begins. The anchor is on the import list, which is a node and not a
        // token: there is no operator here to align on, only a gap.
        if self.options.align_use_imports
            && matches!(parent, Some(NodeKind::USE_STMT | NodeKind::NO_STMT))
            && next
                .as_node()
                .is_some_and(|node| node.node_kind() == NodeKind::LIST_EXPR)
        {
            return Some((AnchorClass::UseImports, 0));
        }
        let token = next.as_token()?;
        if token.token_kind().is_assignment_op() && parent == Some(NodeKind::ASSIGN_EXPR) {
            // The whole operator: `=`, `-=` and `||=` line up on their `=`
            // (docs/formatting.md ALIGNMENT-2).
            return Some((AnchorClass::Assign, token.text().width()));
        }
        if token.token_kind() == T!["=>"] {
            return Some((AnchorClass::FatComma(self.fat_comma_depth), 0));
        }
        if matches!(token.token_kind(), T!["//"] | T!["||"])
            && parent == Some(NodeKind::BINARY_EXPR)
        {
            return Some((AnchorClass::Fallback, 0));
        }
        if parent == Some(NodeKind::STMT_MODIFIER) && token.token_kind().is_stmt_modifier() {
            return Some((AnchorClass::PostfixKeyword, 0));
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
        // `$invocant->&name(...)`: the `&` introduces the name and belongs to
        // it, the same way the arrow before it does.
        if before == Some(TokenKind::BITWISE_AND) && parent == Some(NodeKind::METHOD_CALL_EXPR) {
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
        ) || before.is_some_and(|kind| kind.is_heredoc_body() || kind == TokenKind::POD_CONTENT)
        {
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
        // A subscript hugs what it applies to — `$h{a}{b}` — and its brackets
        // hug a name and open up around anything else: `$h->{key}`, `$h->{$k}`,
        // but `$h->{ $o->meth }` and `@x{ 'a', 'b' }` (docs/formatting.md
        // SPACING-7). The same reading as a dereference's braces below, and as a
        // literal's: a bracket closes up around a name because the two read as
        // one word, and a subscript holding an expression is not one word.
        if matches!(
            parent,
            Some(
                NodeKind::HASH_SUBSCRIPT_EXPR
                    | NodeKind::ARRAY_SUBSCRIPT_EXPR
                    | NodeKind::POSTFIX_ARRAY_SLICE_EXPR
                    | NodeKind::POSTFIX_HASH_SLICE_EXPR
            )
        ) {
            if matches!(after, Some(T!["{"] | T!["["])) {
                return false;
            }
            if matches!(after, Some(T!["}"] | T!["]"])) || matches!(before, Some(T!["{"] | T!["["]))
            {
                let subscript = previous
                    .parent()
                    .or_else(|| next.parent())
                    .and_then(|node| {
                        node.children()
                            .find(|child| child.node_kind() == NodeKind::SUBSCRIPT)
                    });
                // No `SUBSCRIPT` node means empty brackets, which stay empty.
                let Some(subscript) = subscript else {
                    return false;
                };
                return match self.options.delimiter_spacing {
                    DelimiterSpacing::Tight => false,
                    DelimiterSpacing::Standard => {
                        Self::item_count(&subscript) >= 2
                            || sole_item(&subscript).is_some_and(|item| !is_simple_term(&item))
                    }
                    DelimiterSpacing::Loose => true,
                };
            }
        }
        // A dereference's braces hug a name and open up around anything else:
        // `@{$x}`, but `@{ $ref->{bar} }` (SPACING-7). The sigil rule above has
        // already closed the gap between the sigil and the brace.
        if parent == Some(NodeKind::BLOCK_DEREF_EXPR)
            && (matches!(after, Some(T!["}"])) || matches!(before, Some(T!["{"])))
        {
            // No child node at all means the braces hold something that is not
            // an expression — `${^MATCH}`, whose name is the caret and the word
            // together. A space there names a different variable, which no
            // token-stream comparison can see.
            let deref = previous.parent().or_else(|| next.parent());
            return deref
                .and_then(|node| node.children().next())
                .is_some_and(|inner| !is_simple_term(&inner));
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
        // `LOOP:` — a label's colon hugs its name (SPACING-11). Everywhere
        // else `:` is the ternary's and takes spaces.
        if parent == Some(NodeKind::LABEL) && after == Some(T![":"]) {
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
    /// the next token's leading trivia (the trivia model), and because no node's
    /// range includes trivia, that is the whole gap — no guessing from node
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
        let text = if kind.is_heredoc_body() {
            // A heredoc body owns whole lines and starts in column 0, wherever
            // the marker that opened it was written (the parser contract). Reached
            // through a list or an argument list it used to arrive as a `Raw`,
            // which starts at the current column: the first line was indented
            // into the string and the terminator was indented out of being a
            // terminator, and perl could not read the output back.
            let terminator = kind != TokenKind::HEREDOC_CONTENT;
            let lines = Doc::VerbatimLines(token.text().into());
            if terminator {
                // Nothing else belongs on the terminator's line.
                Doc::concat(vec![lines, Doc::HardLine])
            } else {
                lines
            }
        } else if kind.is_verbatim() {
            Doc::Raw(token.text().into())
        } else {
            Doc::Token(token.clone())
        };

        // A quote-like operator is scanned as one atomic run (the lexer contract), so
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
        // brace, because the brace does not move (docs/formatting.md NEWLINE-2).
        // `block` emits it there.
        let trailing = if !asks_trailing || self.brace_follows(token) {
            Doc::Nil
        } else {
            self.trailing_comment(token)
        };
        if leading.is_nil() && trailing.is_nil() {
            return text;
        }
        Doc::concat(vec![leading, text, trailing])
    }

    /// A `format` declaration: header laid out, picture lines untouched.
    ///
    /// The header is ordinary code and takes the statement's indentation. The
    /// picture lines are not: `@<<<<` is a field five characters wide, so every
    /// character of them, leading whitespace included, is reproduced where it
    /// was.
    fn format_decl(&mut self, node: &SyntaxNode) -> Doc {
        let mut parts = vec![self.leading_docs_of(node)];
        for token in node
            .children_with_tokens()
            .filter_map(|child| child.into_token())
        {
            match token.token_kind() {
                TokenKind::FORMAT_CONTENT => parts.push(Doc::VerbatimLines(token.text().into())),
                kind if kind.is_trivia() => {}
                // No `Doc::Space` between them: the header arrives as the
                // keyword and one raw span that already holds the writer's
                // spacing, so inserting more would double it.
                _ => parts.push(Doc::Token(token)),
            }
        }
        Doc::concat(parts)
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
            .map(|token| Doc::Raw(token.text().into()))
            .collect();
        Doc::concat(parts)
    }

    /// A quote-like operator: one lexical run (the lexer contract), and so one atom.
    ///
    /// Emitted token by token, the closing delimiter of a run whose content
    /// spans lines is a token like any other: it starts a line, and a line that
    /// starts gets the enclosing indentation. `q{\nalpha\n}` came out with its
    /// `}` indented — inside the string, where it changed the value, and where
    /// `dev check` saw the verbatim content it was supposed to preserve change
    /// under it. The run's own source text is the one rendering that cannot be
    /// wrong.
    fn quote_like(&mut self, node: &SyntaxNode) -> Doc {
        let leading = self.leading_docs_of(node);
        let trailing = match last_token(node) {
            Some(token) => self.trailing_comment(&token),
            None => Doc::Nil,
        };
        Doc::concat(vec![
            leading,
            Doc::Raw(node.text().to_string().into()),
            trailing,
        ])
    }

    /// POD, `__DATA__`, a `format` picture and heredoc bodies: the region's
    /// source text, reproduced where it was.
    ///
    /// One `VerbatimLines` for the whole node rather than one per token. These
    /// constructs exist in column 0 and nowhere else — the lexer recognises
    /// `=head1` and `__END__` at a line start and there only (the lexer contract) — so
    /// indenting one produces output that no longer contains it. Emitting them
    /// token by token put the first in column 0 and left the rest to pick up the
    /// enclosing block's indentation, which is the same bug one token along.
    fn verbatim(&mut self, node: &SyntaxNode) -> Doc {
        let leading = self.leading_docs_of(node);
        Doc::concat(vec![
            leading,
            Doc::VerbatimLines(node.text().to_string().into()),
        ])
    }

    /// Does this bracketed construct break at its seed — a newline straight
    /// after the opening delimiter (docs/formatting.md INDENT-2), or a comment
    /// it has to break for?
    fn breaks_at_its_seed(&self, node: &SyntaxNode, opening: Option<&SyntaxToken>) -> bool {
        self.contains_comment(node)
            || (self.heredoc_markers_in(node) == 0
                && opening.is_some_and(|token| self.newline_follows(token)))
    }

    /// Is this node an element of a list whose brackets break?
    ///
    /// Such a list puts one element per line ([`Self::list_items`]), so an
    /// element written after a `,` begins a line whether or not the writer put
    /// it on one.
    fn element_of_a_broken_list(&self, node: &SyntaxNode) -> bool {
        let Some(list) = node
            .parent()
            .filter(|parent| parent.node_kind() == NodeKind::LIST_EXPR)
        else {
            return false;
        };
        let Some(brackets) = list.parent().filter(|parent| {
            matches!(
                parent.node_kind(),
                NodeKind::ARG_LIST
                    | NodeKind::PAREN_EXPR
                    | NodeKind::ANON_ARRAY
                    | NodeKind::ANON_HASH
            )
        }) else {
            return false;
        };
        // No node's range begins on trivia (the trivia model), so the first
        // token is the opening bracket.
        let opening = brackets.first_token();
        self.breaks_at_its_seed(&brackets, opening.as_ref())
    }

    /// Should the lines this call swallowed sit at the level of the list around
    /// it, rather than hang under its first argument?
    ///
    /// A list whose brackets break puts every element on a line at the
    /// brackets' level, so anything below one of them belongs there too — they
    /// are elements to whoever wrote them, whatever camello made of them. Where
    /// the brackets do not break, the output keeps the writer's own lines, and
    /// a call written after a separator on a line it shares is the same case
    /// one line at a time.
    ///
    /// The second half asks the input, and may: with the brackets flat there is
    /// no break for the formatter to add, so the answer is the same on the next
    /// pass (the formatter contract, I2). The first half does not ask at all.
    fn takes_the_list_level(&self, node: &SyntaxNode) -> bool {
        if self.element_of_a_broken_list(node) {
            return true;
        }
        list_separator_before(node).is_some() && !begins_its_line(node)
    }

    /// A closing delimiter, split into what was written inside it and the
    /// delimiter itself.
    ///
    /// The two go in different places. An own-line comment takes the
    /// indentation of where it is (docs/formatting.md COMMENT-2), and where it
    /// is, is inside the brackets — it is the last line of the contents, not the
    /// first of whatever the delimiter closes onto. Left attached to the
    /// delimiter it came back at the enclosing level, so `#>>>` sat in column 0
    /// under a `#<<<` the contents had indented.
    fn closing_delimiter(&mut self, token: &SyntaxToken) -> (Doc, Doc) {
        let inside = self.leading_docs(token);
        let trailing = if self.brace_follows(token) {
            Doc::Nil
        } else {
            self.trailing_comment(token)
        };
        let delimiter = if trailing.is_nil() {
            Doc::Token(token.clone())
        } else {
            Doc::concat(vec![Doc::Token(token.clone()), trailing])
        };
        (inside, delimiter)
    }

    /// The own-line comments and blank lines written before this node.
    ///
    /// Nodes emitted as one verbatim region never reach [`Self::token`], so
    /// without this the comment above a `__DATA__` or a `=head1` is simply not
    /// emitted — which no invariant noticed until comment preservation became
    /// one.
    fn leading_docs_of(&mut self, node: &SyntaxNode) -> Doc {
        match first_token(node) {
            Some(token) => self.leading_docs(&token),
            None => Doc::Nil,
        }
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
                if !self.brace_follows(&token) {
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
        //
        // Both comments end up on the brace's line, and they keep the order they
        // were written in: the one from before the brace first, the brace's own
        // trailing comment after it. `DBI::DBD::SqlEngine` writes a sentence
        // across the two and had it come back in reverse.
        let open = match brace(node, T!["{"], false) {
            Some(token) => {
                let leading = self.leading_docs(&token);
                let trailing = self.trailing_comment(&token);
                Some(Doc::concat(vec![
                    leading,
                    Doc::Token(token),
                    header_comment,
                    trailing,
                ]))
            }
            None => Some(header_comment),
        };
        // A comment written before the closing brace is the block's last line,
        // not the brace's first: it goes inside, at the statements' level.
        let (written_inside, close) = match brace(node, T!["}"], true) {
            Some(token) => {
                let (inside, delimiter) = self.closing_delimiter(&token);
                (inside, Some(delimiter))
            }
            None => (Doc::Nil, None),
        };

        if flat {
            let mut parts = Vec::new();
            parts.extend(open);
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
        parts.push(Doc::HardLine);
        body.push(written_inside);
        parts.push(Doc::indent(Doc::concat(body)));
        parts.extend(close);
        Doc::group(true, Doc::concat(parts))
    }

    /// GUESS: a block written on one line was meant to stay on one line.
    /// Evidence: one statement at most, no `;`, no comment, and no newline in
    /// the source (docs/formatting.md NEWLINE-2).
    /// Wrong: only the shape changes, never the meaning.
    ///
    /// The single rule that replaces `is_simple_block`'s seven rejections plus
    /// the `suppress_newlines` flag that leaked past them.
    ///
    /// Memoised, and asking only about the *nearest* nested blocks. Without
    /// either, twenty nested `sub {` — forty characters of input — took over
    /// ninety seconds: recursing into every descendant block meant each level
    /// re-answered the question for every level below it, and each answer
    /// allocated the node's whole text to look for a newline in it.
    fn block_can_be_flat(&mut self, node: &SyntaxNode, statements: &[SyntaxNode]) -> bool {
        let key = node.text_range().start();
        if let Some(&answer) = self.flat_blocks.get(&key) {
            return answer;
        }
        let answer = self.compute_block_can_be_flat(node, statements);
        self.flat_blocks.insert(key, answer);
        answer
    }

    fn compute_block_can_be_flat(&mut self, node: &SyntaxNode, statements: &[SyntaxNode]) -> bool {
        // Error recovery can leave a block with no closing brace. There is no
        // `{ x }` to fit on a line, so there is nothing to be flat, and saying
        // otherwise makes the output re-read as a different shape on the next
        // pass.
        if brace(node, T!["}"], true).is_none() {
            return false;
        }
        // A control structure's block always breaks (docs/formatting.md NEWLINE-2),
        // and an empty one is still a block: `if (1) {} else { 1; }` kept `{ }`
        // on the `if` line while the `else` broke, so the two branches of one
        // statement read as two different shapes.
        // `sub`, `do`, `map` and `try` blocks are not control structures: they
        // may hold a single value and stay on one line (the formatter contract).
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
                        | NodeKind::CONTINUE_CLAUSE
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
        // An empty block elsewhere is `{ }`; there is nothing to put on a line
        // of its own.
        if statements.is_empty() {
            return !self.contains_comment(node);
        }
        if statements.len() != 1 {
            return false;
        }
        if !self.options.allow_single_line_blocks {
            return false;
        }
        // A statement that was written across lines stays across lines, and a
        // statement that ends in `;` reads as a body rather than a value
        // (the formatter contract: single statement, no semicolon, no comment, no source
        // newline).
        if self.contains_newline(node) {
            return false;
        }
        if statements[0]
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .any(|token| token.token_kind() == T![";"])
        {
            return false;
        }
        if self.contains_comment(node) {
            return false;
        }

        // A flat group must contain no hard line break, so a block that holds a
        // block that has to break cannot itself be flat. Keeping this a property
        // of the structure is what removes the old `suppress_newlines` flag and
        // the leaks it caused (F2).
        //
        // Only the nearest nested blocks: each of them asks the same question of
        // its own, so the answer covers every depth without this level walking
        // there itself.
        for child in nearest_blocks(node) {
            let statements: Vec<SyntaxNode> = child.children().collect();
            if !self.block_can_be_flat(&child, &statements) {
                return false;
            }
        }
        true
    }

    /// A bracketed group: parentheses, an anonymous array or an anonymous hash.
    ///
    /// GUESS: a newline straight after the opening bracket means the group was
    /// meant to stand open (docs/formatting.md INDENT-2).
    /// Evidence: that newline, and nothing else. The rule is stable under
    /// re-formatting because a broken group's own output has the newline there
    /// (the formatter contract, I2).
    /// Wrong: only the shape changes, never the meaning.
    ///
    /// A group holding a comment is broken too, and that half is no guess: a
    /// comment runs to end of line, so it *is* a hard line break, and a flat
    /// group is by definition one that contains none. Leaving it out is how
    /// `my %h = ( # c\n a => 1,\n);` formatted to `my %h = ( # ca => 1,);`,
    /// with the entry commented out of existence.
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

        // A group that opened a heredoc owes the body to the line its marker is
        // on, and that body is emitted after the whole statement — so a group
        // that breaks puts it after the closing bracket instead, which is a
        // different program. Specio writes `sprintf(\n    <<'EOF', $x, $y );`
        // and got its format string back as two arguments and a stray block.
        // A comment still wins: a flat group would comment out the code after
        // it, and that is the greater loss.
        let broken = self.breaks_at_its_seed(node, opening.as_ref());

        let is_hash = open == T!["{"];
        if is_hash {
            self.fat_comma_depth = self.fat_comma_depth.saturating_add(1);
        }

        // `children_with_tokens`, not `children`: a heredoc body is a token, and
        // it can be a direct child of the bracket its marker was written inside
        // — `f(\n    <<'A'\nbody\nA\n);` puts one right there. Walking only the
        // child nodes dropped it from the output, string and terminator both.
        let mut inner = Vec::new();
        for child in node.children_with_tokens() {
            match child {
                SyntaxElement::Node(child) => {
                    // `print( {$fh} @data )` and `map({ $_ } @list)`: the
                    // handle and the block are children of their own, beside the
                    // list, and nothing separates the two but the space perl
                    // needs to tell them apart. A `,` written after the block is
                    // part of the list and brings its own spacing.
                    if matches!(child.node_kind(), NodeKind::FILEHANDLE | NodeKind::BLOCK)
                        && !comma_follows(&child)
                    {
                        inner.push(self.list_items(&child, broken));
                        inner.push(Doc::Space);
                        continue;
                    }
                    inner.push(self.list_items(&child, broken));
                }
                SyntaxElement::Token(token) if token.token_kind().is_heredoc_body() => {
                    inner.push(self.token(&token));
                }
                SyntaxElement::Token(_) => {}
            }
        }

        if is_hash {
            self.fat_comma_depth = self.fat_comma_depth.saturating_sub(1);
        }

        let mut parts = Vec::new();
        if let Some(token) = &opening {
            parts.push(self.token(token));
        }

        // What was written between the last element and the closing bracket
        // belongs with the elements, at their level.
        let (written_inside, closing_doc) = match &closing {
            Some(token) => {
                let (inside, delimiter) = self.closing_delimiter(token);
                (inside, Some(delimiter))
            }
            None => (Doc::Nil, None),
        };

        let body = Doc::concat(inner);
        if body.is_nil() {
            // Empty but for a comment: the brackets break so that the comment
            // has a line of its own to be indented on. Empty but for a blank
            // line is not that — there is nothing on either side for it to
            // separate, and the brackets close up (BLANK_LINE-3).
            if self.contains_comment(node) {
                parts.push(Doc::indent(Doc::concat(vec![
                    Doc::HardLine,
                    written_inside,
                ])));
                parts.extend(closing_doc);
                return Doc::group(true, Doc::concat(parts));
            }
            parts.push(written_inside);
            parts.extend(closing_doc);
            return Doc::group(false, Doc::concat(parts));
        }

        if broken {
            parts.push(Doc::indent(Doc::concat(vec![
                Doc::SoftLine,
                body,
                written_inside,
            ])));
            parts.push(Doc::SoftLine);
        } else {
            // docs/formatting.md SPACING-7: whether a flat literal pads its inside
            // depends on the configured spacing and how many items it holds.
            // An `a => 1` pair is two items, so `{ a => 1 }` keeps its spaces
            // under Standard while `[$x]` stays tight. Parentheses are always
            // tight, whatever the setting.
            let spacious = open != T!["("]
                && match self.options.delimiter_spacing {
                    DelimiterSpacing::Tight => false,
                    // A lone item closes the brackets up only where it is a
                    // name: `[$x]` and `{ $single }` are what the arity rule was
                    // for, and `[ map { $_->foo } @$list ]`, `{ $obj->qux }` and
                    // `[ foo($body) ]` are what it caught by accident — a
                    // literal with something inside it, squeezed against its own
                    // brackets because it held one thing.
                    DelimiterSpacing::Standard => {
                        Self::item_count(node) >= 2
                            || sole_item(node).is_some_and(|item| !is_simple_term(&item))
                    }
                    DelimiterSpacing::Loose => true,
                };
            if spacious {
                parts.push(Doc::Space);
            }
            // The contents are the continuation scope, and the closing bracket
            // is outside it: a bracket the user put on a line of its own belongs
            // at the column the construct started from, not at the level of the
            // arguments it closes. Without the break, `] ) )` collapsed onto one
            // line and nothing in the output showed which closed what.
            let own_line = closing
                .as_ref()
                .is_some_and(|token| self.closes_on_its_own_line(token, &body));
            parts.push(Doc::continuation(body));
            if own_line {
                parts.push(Doc::HardLine);
            } else if spacious {
                parts.push(Doc::Space);
            }
        }
        parts.extend(closing_doc);
        if broken {
            return Doc::group(true, Doc::concat(parts));
        }
        // Flat, because the writer put something after the opening bracket and
        // so seeded no break — but written across lines all the same, and its
        // anchors have the several lines that alignment is a relation between.
        // `f($o,` and the `key => value` lines under it are a table, and were
        // the one table camello would not lay out.
        if self.contains_newline(node) {
            return Doc::group_across_lines(Doc::concat(parts));
        }
        Doc::group(false, Doc::concat(parts))
    }

    /// How many items a delimited literal holds, where both `,` and `=>`
    /// separate items (SPACING-7 counts `key => 'val'` as two).
    fn item_count(node: &SyntaxNode) -> usize {
        node.children()
            .map(|child| {
                if child.node_kind() == NodeKind::LIST_EXPR {
                    child.children().count()
                } else {
                    1
                }
            })
            .sum()
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
                    // A list element is not a statement, so it declares a shape
                    // only where it happens to be one. Declaring `None` here
                    // instead would end the enclosing statement's declaration
                    // partway through its own list.
                    if let Some(shape) = shape_key(&node) {
                        parts.push(Doc::Shape(Some(shape)));
                    }
                    parts.push(self.node(&node));
                }
                SyntaxElement::Token(token) if token.token_kind().is_trivia() => {}
                // A heredoc body sits between two elements of the list its
                // marker was written in. It places itself — whole lines, from
                // column 0 — so none of the separator rules below apply to it,
                // and the `Doc::Space` they would add would be written at the
                // start of the line after the terminator.
                SyntaxElement::Token(token) if token.token_kind().is_heredoc_body() => {
                    parts.push(self.token(&token));
                }
                SyntaxElement::Token(token) => {
                    // `=>` joins a key to its value; only `,` ends an element.
                    let ends_element = token.token_kind() == T![","];
                    // Two separators in a row are an empty element: `f(1,, 2)`,
                    // `f(k =>, 1)`, `f('a', => 1)`. The element between them is
                    // what the space would have gone around, so each separator
                    // spaces itself as it always does and neither adds a space
                    // for an element that is not there: a `,` hugs what is on
                    // its left, so nothing precedes it, and a `=>` whose left
                    // neighbour is a separator has its space already and no
                    // column of its own to align to.
                    let empty_before = adjacent_separator(&token, rowan::Direction::Prev).is_some();
                    let empty_after =
                        adjacent_separator(&token, rowan::Direction::Next) == Some(T![","]);
                    if token.token_kind() == T!["=>"] && !empty_before {
                        parts.push(Doc::Anchor(AnchorClass::FatComma(self.fat_comma_depth), 0));
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
                    if empty_after {
                        // Nothing at all: the next separator follows straight on.
                    } else if value_on_next_line {
                        parts.push(Doc::UserLine {
                            broken: true,
                            wraps: true,
                        });
                    } else if ends_element && !last {
                        // A broken group puts one element per line; a flat one
                        // still keeps a line break the user put here
                        // (docs/formatting.md POLICY-4).
                        parts.push(if broken {
                            Doc::Line
                        } else if user_break {
                            Doc::UserLine {
                                broken: true,
                                wraps: true,
                            }
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

    /// Does this closing bracket deserve the line of its own it was written on?
    ///
    /// Only where the contents in front of it wrapped too. A bracket closing
    /// something that fitted on one line is put back on that line — `func({}\n)`
    /// is one line's worth of code and comes out as one — but where the
    /// arguments broke across lines, the closer that ends them shows which
    /// bracket closes what, and the alternative is `] ) )` run together at the
    /// end of the last argument.
    ///
    /// Walks tokens rather than siblings: a closing bracket's left neighbour is
    /// inside the node before it.
    fn closes_on_its_own_line(&self, closing: &SyntaxToken, body: &Doc) -> bool {
        let mut cursor = closing.prev_token();
        loop {
            match cursor {
                Some(token) if token.token_kind() == TokenKind::NEWLINE => break,
                Some(token) if token.token_kind().is_trivia() => cursor = token.prev_token(),
                _ => return false,
            }
        }
        // …and only where the contents are going to occupy more than one line.
        // Asked of the document rather than of the source, because that is what
        // decides it: `Mail::Mailer` writes its list with the newline in front
        // of each comma, which is not a break the formatter keeps, so the
        // contents come out on one line — and a closer left on the next line
        // would be pulled back up by the pass after that (the formatter contract, I2).
        breaks(body)
    }
}

/// The one thing this bracket holds, if it holds exactly one.
fn sole_item(node: &SyntaxNode) -> Option<SyntaxNode> {
    let only = |node: &SyntaxNode| {
        let mut children = node.children();
        let first = children.next()?;
        children.next().is_none().then_some(first)
    };
    let child = only(node)?;
    if child.node_kind() == NodeKind::LIST_EXPR {
        only(&child)
    } else {
        Some(child)
    }
}

/// A term a bracket closes up around: a variable or a literal, and nothing with
/// its own structure (docs/formatting.md SPACING-7).
fn is_simple_term(node: &SyntaxNode) -> bool {
    match node.node_kind() {
        NodeKind::SCALAR_VAR
        | NodeKind::ARRAY_VAR
        | NodeKind::HASH_VAR
        | NodeKind::CODE_VAR
        | NodeKind::TYPEGLOB_VAR
        | NodeKind::LITERAL
        // `${^MATCH}` is one name spelled with a caret in it, and `@{name}` a
        // symbolic reference to another: a space inside those braces names a
        // different variable, which no token-stream comparison can see.
        | NodeKind::SUB_NAME => true,
        // `-1` is a number with a sign on it and `\$x` a variable with a
        // backslash: what the brackets close up around is the operand. `$$sv`
        // and `@$pair` are the same shape, a variable with a sigil in front of
        // it.
        NodeKind::PREFIX_EXPR | NodeKind::REFERENCE_EXPR | NodeKind::DEREF_EXPR => node
            .children()
            .next()
            .is_some_and(|inner| is_simple_term(&inner)),
        // A bareword standing alone is a name — `$time->[c_sec]`, `$h->{-key}`.
        // With arguments beside it, it is a call, and a call has structure of
        // its own. A `qw(a b)` is not one of these: it is one lexical run
        // (the lexer contract) but it spells a list, and a list opens up.
        NodeKind::LIST_CALL_EXPR => {
            let mut children = node.children();
            let name = children.next();
            children.next().is_none()
                && name.is_some_and(|child| child.node_kind() == NodeKind::SUB_NAME)
        }
        _ => false,
    }
}

/// Will this document put anything on a line of its own?
///
/// A `Line` or `SoftLine` answers for the group that holds it, which is why the
/// question is asked of the group and not of them.
fn breaks(doc: &Doc) -> bool {
    match doc {
        Doc::HardLine | Doc::BlankLine | Doc::VerbatimLines(_) | Doc::Comment(_, _) => true,
        Doc::UserLine { broken, .. } => *broken,
        Doc::Raw(text) => text.contains('\n'),
        Doc::Group { broken, body, .. } => *broken || breaks(body),
        Doc::Indent(body) | Doc::Continuation(body) | Doc::Rooted(body) => breaks(body),
        Doc::Concat(parts) => parts.iter().any(breaks),
        _ => false,
    }
}

/// Does any of these ascending offsets fall inside the node?
fn contains(offsets: &[TextSize], node: &SyntaxNode) -> bool {
    let range = node.text_range();
    let index = offsets.partition_point(|&start| start < range.start());
    offsets.get(index).is_some_and(|&start| start < range.end())
}

/// The blocks inside this node that no other block lies between.
///
/// Answering a question about "every nested block" by asking it of these, and
/// letting each of them do the same, is what keeps the recursion linear: the
/// alternative visits a block once for every ancestor it has.
fn nearest_blocks(node: &SyntaxNode) -> Vec<SyntaxNode> {
    let mut found = Vec::new();
    let mut pending: Vec<SyntaxNode> = node.children().collect();
    while let Some(child) = pending.pop() {
        if child.node_kind() == NodeKind::BLOCK {
            found.push(child);
        } else {
            pending.extend(child.children());
        }
    }
    found
}

/// Whether this token may carry leading and trailing trivia of its own.
///
/// Both, unless it is inside an atomic quote-like run, where only the first
/// token of the run can be preceded by a comment and only the last can be
/// followed by one.
/// Is the nearest thing written before this token a heredoc body?
fn follows_heredoc_body(token: &SyntaxToken) -> bool {
    let mut previous = token.prev_token();
    while let Some(candidate) = previous {
        if !candidate.token_kind().is_trivia() {
            return candidate.token_kind().is_heredoc_body();
        }
        previous = candidate.prev_token();
    }
    false
}

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

/// Statements that get a blank line on each side (docs/formatting.md BLANK_LINE-1).
fn wants_surrounding_blank_lines(node: &SyntaxNode) -> bool {
    matches!(node.node_kind(), NodeKind::SUB_DEF | NodeKind::PHASE_BLOCK)
}

/// Statements that get a blank line before them only.
fn wants_preceding_blank_line(node: &SyntaxNode) -> bool {
    matches!(node.node_kind(), NodeKind::POD | NodeKind::DATA_SECTION)
}

/// The list separator written directly against this one, if there is one.
///
/// Two separators with nothing between them are an empty element, which perl
/// allows and drops.
fn adjacent_separator(token: &SyntaxToken, direction: rowan::Direction) -> Option<TokenKind> {
    token
        .siblings_with_tokens(direction)
        .skip(1)
        .find(|sibling| match sibling.as_token() {
            Some(token) => !token.token_kind().is_trivia(),
            None => true,
        })
        .and_then(SyntaxElement::into_token)
        .map(|sibling| sibling.token_kind())
        .filter(|kind| matches!(kind, T![","] | T!["=>"]))
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
    // A `use` and a `no` have nothing to walk for: what they declare is that
    // the statement above was one too, which is what keeps a block of them one
    // alignment group and ends it where the block does. One shape for both,
    // because `use` and `no` are written in one block and read as one table —
    // the keywords are the same width, so the columns were already agreeing on
    // paper before anything lined them up.
    if matches!(kind, NodeKind::USE_STMT | NodeKind::NO_STMT) {
        return Some(ShapeKey {
            statement: NodeKind::USE_STMT,
            declares: false,
            list_assignment: false,
        });
    }
    if !matches!(kind, NodeKind::EXPR_STMT | NodeKind::VAR_DECL_STMT) {
        return None;
    }
    // One walk, not two: this is asked of every statement in the file, and the
    // two questions it answers are both about nodes it passes on the way.
    let mut declares = false;
    let mut list_assignment = false;
    for child in node.descendants() {
        match child.node_kind() {
            NodeKind::VAR_DECL => declares = true,
            NodeKind::DECL_TARGET => {
                list_assignment = list_assignment
                    || child
                        .first_token()
                        .is_some_and(|token| token.token_kind() == T!["("]);
            }
            _ => {}
        }
        if declares && list_assignment {
            break;
        }
    }

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

/// The node's first token that is not trivia.
///
/// Walked from the node's own edge rather than by iterating its descendants: the
/// spacing rules ask this of both sides of every adjacent pair, and a subtree
/// walk makes that quadratic in the size of the statement.
fn first_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    edge_token(node, node.first_token()?, rowan::Direction::Next)
}

/// The node's last token that is not trivia.
fn last_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    edge_token(node, node.last_token()?, rowan::Direction::Prev)
}

/// From `token`, the first non-trivia token in `direction` that is still inside
/// `node`. Stepping past the node's own range is how a node of nothing but
/// trivia would otherwise answer with its neighbour's token.
fn edge_token(
    node: &SyntaxNode,
    token: SyntaxToken,
    direction: rowan::Direction,
) -> Option<SyntaxToken> {
    let range = node.text_range();
    let mut token = token;
    while token.token_kind().is_trivia() {
        token = match direction {
            rowan::Direction::Next => token.next_token()?,
            rowan::Direction::Prev => token.prev_token()?,
        };
        if !range.contains_range(token.text_range()) {
            return None;
        }
    }
    Some(token)
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

/// The list separator this node was written after, if it is an element of a
/// list at all.
///
/// GUESS: the lines under a bareword call written along a list are the list's,
/// not the call's arguments.
/// Evidence: none — `a => f Str,` and the `bbb => 1` written under it are one
/// argument list to camello, because an unknown bareword is a list operator
/// (`grammar/builtins.rs`), and perl's own answer is in a prototype camello
/// cannot see.
/// Wrong: only the indent of those lines, never the meaning — they keep the
/// list's level instead of hanging under an argument list perl may not agree
/// the call has.
fn list_separator_before(node: &SyntaxNode) -> Option<TokenKind> {
    let mut cursor = node.prev_sibling_or_token();
    while let Some(element) = cursor {
        match element {
            SyntaxElement::Token(token) if token.token_kind().is_trivia() => {
                cursor = token.prev_sibling_or_token();
            }
            SyntaxElement::Token(token) => {
                return matches!(token.token_kind(), T![","] | T!["=>"])
                    .then(|| token.token_kind());
            }
            SyntaxElement::Node(_) => return None,
        }
    }
    None
}

/// Is this node the first thing written on its line?
fn begins_its_line(node: &SyntaxNode) -> bool {
    let Some(first) = node.first_token() else {
        return true;
    };
    let mut token = first.prev_token();
    while let Some(current) = token {
        if current.token_kind() == TokenKind::NEWLINE {
            return true;
        }
        if !current.token_kind().is_trivia() {
            return false;
        }
        token = current.prev_token();
    }
    true
}

/// Is the next thing written after this node a `,`?
///
/// Asked of the block in `map({ $_ }, @list)`, where the comma is the first
/// token of the list beside it rather than a sibling of its own.
fn comma_follows(node: &SyntaxNode) -> bool {
    node.next_sibling_or_token().is_some_and(|next| match next {
        SyntaxElement::Token(token) => token.token_kind() == T![","],
        SyntaxElement::Node(node) => {
            first_token(&node).is_some_and(|token| token.token_kind() == T![","])
        }
    })
}
