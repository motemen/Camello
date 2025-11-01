use std::collections::HashMap;

use crate::{comments::TriviaTable, PerlLanguage, PerlNode, SyntaxKind, T};
use rowan::{ast::SyntaxNodePtr, NodeOrToken, WalkEvent};

/// Determine whether `node` qualifies as a "simple" block that can stay on a single
/// line. A block is considered simple only when *all* of the following hold:
///
/// * it does not contain control-flow statements such as `if`, `while`, or `try`
///   (including phase blocks like `BEGIN { ... }`);
/// * it contains no comments or heredoc tokens;
/// * it contains no newline characters of any kind;
/// * the last significant token before the closing brace is not a semicolon; and
/// * every nested block encountered during the scan is itself simple.
///
/// The traversal returns immediately once a rule is violated to avoid walking the
/// entire subtree unnecessarily. Results are memoized in `cache` so nested blocks can
/// reuse their previous classification.
pub(crate) fn is_simple_block_cached(
    node: &PerlNode,
    trivia: &TriviaTable,
    cache: &mut HashMap<SyntaxNodePtr<PerlLanguage>, bool>,
) -> bool {
    let ptr = SyntaxNodePtr::new(node);
    if let Some(&cached) = cache.get(&ptr) {
        return cached;
    }

    let result = analyze_block(node, trivia, cache);
    cache.insert(ptr, result);
    result
}

fn analyze_block(
    node: &PerlNode,
    trivia: &TriviaTable,
    cache: &mut HashMap<SyntaxNodePtr<PerlLanguage>, bool>,
) -> bool {
    let mut traversal = node.preorder_with_tokens();
    let mut last_significant: Option<SyntaxKind> = None;

    while let Some(event) = traversal.next() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(child)) => {
                if child == *node {
                    continue;
                }

                let kind = child.kind();

                if kind == SyntaxKind::BLOCK_STMT {
                    if !is_simple_block_cached(&child, trivia, cache) {
                        return false;
                    }

                    traversal.skip_subtree();
                    continue;
                }

                if is_control_flow(kind) {
                    return false;
                }
            }
            WalkEvent::Enter(NodeOrToken::Token(token)) => {
                match token.kind() {
                    SyntaxKind::WHITESPACE => {
                        if token.text().contains('\n') {
                            return false;
                        }
                    }
                    SyntaxKind::NEWLINE => {
                        return false;
                    }
                    SyntaxKind::COMMENT => {
                        // Touch the trivia table so both leading and trailing comments are tracked.
                        let _ = trivia.position_of(&token);
                        return false;
                    }
                    SyntaxKind::HEREDOC_START
                    | SyntaxKind::HEREDOC_CONTENT
                    | SyntaxKind::HEREDOC_END => return false,
                    T!['{'] | T!['}'] => {}
                    _ => {
                        if token.text().contains('\n') {
                            return false;
                        }
                        last_significant = Some(token.kind());
                    }
                }
            }
            WalkEvent::Leave(NodeOrToken::Node(child)) if child == *node => {
                break;
            }
            WalkEvent::Leave(_) => {}
        }
    }

    last_significant != Some(T![;])
}

fn is_control_flow(kind: SyntaxKind) -> bool {
    use SyntaxKind::*;

    matches!(
        kind,
        IF_STMT
            | UNLESS_STMT
            | WHILE_STMT
            | UNTIL_STMT
            | FOR_STMT
            | TRY_STMT
            | GIVEN_STATEMENT
            | WHEN_CLAUSE
            | DEFAULT_CLAUSE
            | PHASE_BLOCK_STMT,
    )
}
