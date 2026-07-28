use super::*;

/// The whole point of ADR 0004 §1: the two spaces must round-trip through the
/// single `u16` rowan sees, and must never overlap.
#[test]
fn token_and_node_kinds_round_trip_through_syntax_kind() {
    for raw in 0..TOKEN_COUNT {
        let token = unsafe { std::mem::transmute::<u16, TokenKind>(raw) };
        let kind = SyntaxKind::from(token);
        assert_eq!(kind.as_token(), Some(token));
        assert_eq!(kind.as_node(), None, "{token:?} must not read as a node");
    }

    for raw in 0..NODE_COUNT {
        let node = unsafe { std::mem::transmute::<u16, NodeKind>(raw) };
        let kind = SyntaxKind::from(node);
        assert_eq!(kind.as_node(), Some(node));
        assert_eq!(kind.as_token(), None, "{node:?} must not read as a token");
    }
}

#[test]
fn sections_partition_the_token_space() {
    for raw in 0..TOKEN_COUNT {
        let token = unsafe { std::mem::transmute::<u16, TokenKind>(raw) };
        let classes = [token.is_keyword(), token.is_punct(), token.is_trivia()]
            .into_iter()
            .filter(|flag| *flag)
            .count();
        assert!(
            classes <= 1,
            "{token:?} is in more than one section of define_language!"
        );
    }
}

#[test]
fn keyword_lookup_agrees_with_the_keyword_section() {
    for raw in 0..TOKEN_COUNT {
        let token = unsafe { std::mem::transmute::<u16, TokenKind>(raw) };
        let Some(text) = token.text() else { continue };
        if token.is_keyword() {
            assert_eq!(
                TokenKind::from_keyword(text),
                Some(token),
                "{token:?} is a keyword but does not round-trip through from_keyword"
            );
        } else {
            assert_eq!(
                TokenKind::from_keyword(text),
                None,
                "{token:?} is not a keyword but from_keyword({text:?}) resolves"
            );
        }
    }
}

#[test]
fn t_macro_resolves_to_the_declared_kind() {
    assert_eq!(T!["if"], TokenKind::IF_KW);
    assert_eq!(T!["{"], TokenKind::L_BRACE);
    assert_eq!(T!["=>"], TokenKind::FAT_COMMA);
    assert_eq!(T!["//="], TokenKind::DEFINED_OR_EQ);
    assert_eq!(T!["->@*"], TokenKind::POSTFIX_DEREF_ARRAY);
}

/// Diagnostics must not leak internal enum names (ADR 0004 §2, ADR 0007 §3).
#[test]
fn display_names_are_human_readable() {
    assert_eq!(TokenKind::R_BRACE.to_string(), "`}`");
    assert_eq!(TokenKind::IDENT.to_string(), "identifier");
    assert_eq!(
        TokenKind::UNTERMINATED_REGEX.to_string(),
        "unterminated regex"
    );

    for raw in 0..TOKEN_COUNT {
        let token = unsafe { std::mem::transmute::<u16, TokenKind>(raw) };
        let name = token.display_name();
        assert!(!name.is_empty(), "{token:?} has an empty display name");

        match token.text() {
            // Kinds with a spelling quote it: `__END__` is the real source text,
            // not an enum name that leaked.
            Some(text) => assert_eq!(name, format!("`{text}`")),
            // Everything else must read as prose.
            None => assert!(
                !name.contains('_') && name.chars().any(char::is_lowercase),
                "{token:?} display name {name:?} looks like an enum variant"
            ),
        }
    }
}

/// The context-sensitive spellings must stay out of `T!` (see `punct_ctx`).
#[test]
fn context_sensitive_spellings_keep_distinct_kinds() {
    assert_ne!(TokenKind::HASH_SIGIL, TokenKind::MODULO);
    assert_eq!(TokenKind::HASH_SIGIL.text(), Some("%"));
    assert_eq!(TokenKind::MODULO.text(), Some("%"));
    assert!(TokenKind::MODULO.is_punct());
    assert!(TokenKind::HASH_SIGIL.is_sigil());
}

#[test]
fn trivia_classification_matches_adr_0006() {
    assert!(TokenKind::WHITESPACE.is_trivia());
    assert!(TokenKind::NEWLINE.is_trivia());
    assert!(TokenKind::COMMENT.is_trivia());
    assert!(!TokenKind::IDENT.is_trivia());
    assert!(SyntaxKind::from(TokenKind::COMMENT).is_trivia());
    assert!(!SyntaxKind::from(NodeKind::ROOT).is_trivia());
}
