use super::*;
use crate::SyntaxKind;

#[test]
fn test_postfix_dereference_lexing() {
    let test_cases = [
        ("$ref->@*", SyntaxKind::POSTFIX_DEREF_ARRAY),
        ("$ref->%*", SyntaxKind::POSTFIX_DEREF_HASH),
        ("$ref->$*", SyntaxKind::POSTFIX_DEREF_SCALAR),
    ];

    for (input, expected_deref) in test_cases {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some((kind, text)) = lexer.next_token() {
            tokens.push((kind, text));
        }

        println!("Input: {}, Tokens: {:?}", input, tokens);

        // Should have: $, identifier, and then the postfix dereference token
        assert!(
            tokens.len() >= 3,
            "Expected at least 3 tokens for {}",
            input
        );

        // Find the postfix dereference token
        let found_deref = tokens.iter().any(|(kind, _)| *kind == expected_deref);
        assert!(
            found_deref,
            "Expected {:?} token in {:?}",
            expected_deref, input
        );
    }
}

#[test]
fn test_x_after_sub_keyword() {
    // Test the case mentioned by Gemini: sub x { ... } where x should be IDENT
    let mut lexer = Lexer::new("sub x {");

    assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "x"))); // Should be IDENT, not X
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::L_BRACE, "{")));
}

#[test]
fn test_hash_declaration() {
    // Test that "my %hash" correctly identifies % as sigil
    let mut lexer = Lexer::new("my %hash");

    assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::PERCENT, "%"))); // Should be PERCENT (sigil)
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "hash")));
}

#[test]
fn test_substitution_with_flags() {
    // Test s/pattern/replacement/flags lexing - this demonstrates the current issue
    let mut lexer = Lexer::new("s/world/universe/gi");

    // Parse through the substitution operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
    lexer.begin_quote_like(SyntaxKind::S_KW, crate::lexer::QuoteLikeMode::S);
    lexer.begin_quote_like(SyntaxKind::S_KW, crate::lexer::QuoteLikeMode::S);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::REGEX_PATTERN, "world"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::INTERPOLATED_STRING, "universe"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

    // Now the lexer should parse flags as a single token
    let flags_token = lexer.next_token();
    println!("Flags token: {:?}", flags_token);

    // This should now correctly parse as S_FLAGS
    assert_eq!(flags_token, Some((SyntaxKind::S_FLAGS, "gi")));
}

#[test]
fn test_simple_heredoc_lexing() {
    let mut lexer = Lexer::new("print <<EOF;\nHello\nEOF\n");
    let mut kinds = Vec::new();
    while let Some((k, _)) = lexer.next_token() {
        kinds.push(k);
    }
    assert!(kinds.contains(&SyntaxKind::HEREDOC_START));
    assert!(kinds.contains(&SyntaxKind::HEREDOC_CONTENT));
    assert!(kinds.contains(&SyntaxKind::HEREDOC_END));
}

#[test]
fn test_heredoc_terminator_requires_own_line() {
    let src = "print <<EOF;\nHello\nEOF not term\nEOF\n";
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    while let Some((k, t)) = lexer.next_token() {
        tokens.push((k, t));
    }

    let ends: Vec<_> = tokens
        .iter()
        .filter(|(k, _)| *k == SyntaxKind::HEREDOC_END)
        .collect();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].1, "EOF\n");
}

#[test]
fn test_unterminated_heredoc_does_not_consume_following_code() {
    let src = "print <<EOF;\nline1\nprint 1;\n";
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    while let Some((k, t)) = lexer.next_token() {
        tokens.push((k, t));
    }

    assert!(tokens.iter().any(|(k, _)| *k == SyntaxKind::HEREDOC_START));
    assert!(tokens
        .iter()
        .any(|(k, _)| *k == SyntaxKind::HEREDOC_CONTENT));
    assert!(!tokens.iter().any(|(k, _)| *k == SyntaxKind::HEREDOC_END));

    let content_idx = tokens
        .iter()
        .position(|(k, _)| *k == SyntaxKind::HEREDOC_CONTENT)
        .unwrap();
    assert!(tokens[content_idx + 1..]
        .iter()
        .any(|(k, t)| *k == SyntaxKind::IDENT && *t == "print"));
}

#[test]
fn test_tr_with_flags() {
    // Test tr/searchlist/replacementlist/flags lexing
    let mut lexer = Lexer::new("tr/abc/XYZ/d");

    // Parse through the transliteration operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::TR_KW, "tr")));
    lexer.begin_quote_like(SyntaxKind::TR_KW, crate::lexer::QuoteLikeMode::TR);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::TR_SEARCH_LIST, "abc"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::TR_REPLACEMENT_LIST, "XYZ"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

    // Now the lexer should parse flags as a single token
    let flags_token = lexer.next_token();
    println!("TR Flags token: {:?}", flags_token);

    // This should now correctly parse as TR_FLAGS
    assert_eq!(flags_token, Some((SyntaxKind::TR_FLAGS, "d")));
}

#[test]
fn test_substitution_various_flags() {
    // Test s/// with different flag combinations

    // Test all valid s/// flags
    let test_cases = [
        ("s/a/b/m", "m"),
        ("s/a/b/s", "s"),
        ("s/a/b/i", "i"),
        ("s/a/b/x", "x"),
        ("s/a/b/p", "p"),
        ("s/a/b/o", "o"),
        ("s/a/b/d", "d"),
        ("s/a/b/u", "u"),
        ("s/a/b/a", "a"),
        ("s/a/b/l", "l"),
        ("s/a/b/n", "n"),
        ("s/a/b/g", "g"),
        ("s/a/b/c", "c"),
        ("s/a/b/e", "e"),
        ("s/a/b/r", "r"),
        // Multiple flags
        ("s/a/b/gi", "gi"),
        ("s/a/b/gim", "gim"),
        ("s/a/b/msixpodualngcer", "msixpodualngcer"), // All flags
    ];

    for (input, expected_flags) in test_cases {
        let mut lexer = Lexer::new(input);

        // Skip to the flags token
        assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
        lexer.begin_quote_like(SyntaxKind::S_KW, crate::lexer::QuoteLikeMode::S);
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::REGEX_PATTERN, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(
            lexer.next_token(),
            Some((SyntaxKind::INTERPOLATED_STRING, "b"))
        );
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

        // Test flags
        let flags_token = lexer.next_token();
        assert_eq!(
            flags_token,
            Some((SyntaxKind::S_FLAGS, expected_flags)),
            "Failed for input: {}",
            input
        );
    }
}

#[test]
fn test_range_operator_lexing() {
    let mut lexer = Lexer::new("1..2 3...4");

    assert_eq!(lexer.next_token(), Some((SyntaxKind::NUMBER, "1")));
    assert_eq!(
        lexer.next_token_with_context(LexContext::Operator),
        Some((SyntaxKind::RANGE, ".."))
    );
    assert_eq!(
        lexer.next_token_with_context(LexContext::Value),
        Some((SyntaxKind::NUMBER, "2"))
    );
    assert_eq!(
        lexer.next_token_with_context(LexContext::Value),
        Some((SyntaxKind::WHITESPACE, " "))
    );
    assert_eq!(
        lexer.next_token_with_context(LexContext::Value),
        Some((SyntaxKind::NUMBER, "3"))
    );
    assert_eq!(
        lexer.next_token_with_context(LexContext::Operator),
        Some((SyntaxKind::RANGE_EXCLUSIVE, "..."))
    );
    assert_eq!(
        lexer.next_token_with_context(LexContext::Value),
        Some((SyntaxKind::NUMBER, "4"))
    );
}

#[test]
fn test_ellipsis_statement_lexing() {
    let mut lexer = Lexer::new("...");
    assert_eq!(
        lexer.next_token_with_context(LexContext::Value),
        Some((SyntaxKind::ELLIPSIS, "..."))
    );
}

#[test]
fn test_tr_various_flags() {
    // Test tr/// with different flag combinations

    let test_cases = [
        ("tr/a/b/c", "c"),
        ("tr/a/b/d", "d"),
        ("tr/a/b/s", "s"),
        ("tr/a/b/r", "r"),
        // Multiple flags
        ("tr/a/b/cd", "cd"),
        ("tr/a/b/sr", "sr"),
        ("tr/a/b/cdsr", "cdsr"), // All flags
    ];

    for (input, expected_flags) in test_cases {
        let mut lexer = Lexer::new(input);

        // Skip to the flags token
        assert_eq!(lexer.next_token(), Some((SyntaxKind::TR_KW, "tr")));
        lexer.begin_quote_like(SyntaxKind::TR_KW, crate::lexer::QuoteLikeMode::TR);
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::TR_SEARCH_LIST, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(
            lexer.next_token(),
            Some((SyntaxKind::TR_REPLACEMENT_LIST, "b"))
        );
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

        // Test flags
        let flags_token = lexer.next_token();
        assert_eq!(
            flags_token,
            Some((SyntaxKind::TR_FLAGS, expected_flags)),
            "Failed for input: {}",
            input
        );
    }
}

#[test]
fn test_keywords_still_work_in_normal_context() {
    // Test that keywords are still recognized as keywords in normal contexts
    let mut lexer = Lexer::new("if ($condition) { package Foo; }");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IF_KW, "if")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::L_PAREN, "(")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "condition")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::R_PAREN, ")")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::L_BRACE, "{")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::PACKAGE_KW, "package"))
    );
}

#[test]
fn test_invalid_flags_rejected() {
    // Test that invalid flags are not consumed as flags

    let mut lexer = Lexer::new("s/a/b/xyz"); // 'z' is not a valid s/// flag

    // Parse through the substitution operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
    lexer.begin_quote_like(SyntaxKind::S_KW, crate::lexer::QuoteLikeMode::S);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::REGEX_PATTERN, "a")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::INTERPOLATED_STRING, "b"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

    // Invalid flags should be treated as an ERROR token
    let flags_token = lexer.next_token();
    println!("Flags token: {:?}", flags_token);
    assert_eq!(flags_token, Some((SyntaxKind::ERROR, "xyz")));

    // After error token, should be at end of input or continue with next statement
    let next_token = lexer.next_token();
    println!("Next token: {:?}", next_token);
    assert_eq!(next_token, None);
}

#[test]
fn test_mixed_valid_invalid_flags() {
    // Test that even one invalid flag makes the entire sequence an error

    let mut lexer = Lexer::new("s/a/b/giz"); // 'z' is not a valid s/// flag

    // Parse through the substitution operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
    lexer.begin_quote_like(SyntaxKind::S_KW, crate::lexer::QuoteLikeMode::S);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::REGEX_PATTERN, "a")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::INTERPOLATED_STRING, "b"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

    // The entire flag sequence should be treated as an error
    let flags_token = lexer.next_token();
    assert_eq!(flags_token, Some((SyntaxKind::ERROR, "giz")));

    // Should be at end of input
    assert_eq!(lexer.next_token(), None);
}

// TODO: Update this test for the new unified QuoteLike API
// #[test]
fn _disabled_test_asymmetric_delimiters() {
    // This test needs to be updated for the new unified QuoteLike API
    // TODO: Reimplement using the new LexerContext::QuoteLike variant
}

#[test]
fn test_qw_basic_parsing() {
    // Test basic qw() parsing with parentheses
    let mut lexer = Lexer::new("qw(hello world)");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "hello")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "world")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_qw_with_colon_content() {
    // Test the specific case that was broken: qw(:common)
    let mut lexer = Lexer::new("qw(:common)");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, ":common")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_qw_with_multiple_words() {
    // Test qw with multiple words including special characters
    let mut lexer = Lexer::new("qw(a:b c:d e)");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "a:b")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "c:d")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "e")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_qw_with_different_delimiters() {
    // Test qw with slash delimiters
    let mut lexer = Lexer::new("qw/x:y z/");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "x:y")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "z")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), None);

    // Test qw with bracket delimiters
    let mut lexer = Lexer::new("qw[foo bar]");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "[")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "foo")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "bar")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "]")));
    assert_eq!(lexer.next_token(), None);

    // Test qw with brace delimiters
    let mut lexer = Lexer::new("qw{alpha beta}");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "{")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "alpha")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "beta")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "}")));
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_qw_empty() {
    // Test empty qw()
    let mut lexer = Lexer::new("qw()");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_qw_with_whitespace() {
    // Test qw with extra whitespace
    let mut lexer = Lexer::new("qw(  hello   world  )");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    lexer.begin_quote_like(SyntaxKind::QW_KW, crate::lexer::QuoteLikeMode::QW);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, "  ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "hello")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, "   ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "world")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, "  ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_tr_y_with_different_delimiters() {
    // Test tr with various delimiters
    let test_cases = [
        ("tr/abc/xyz/", SyntaxKind::TR_KW),
        ("tr(abc)(xyz)", SyntaxKind::TR_KW),
        ("tr[abc][xyz]", SyntaxKind::TR_KW),
        ("tr{abc}{xyz}", SyntaxKind::TR_KW),
        ("y/abc/xyz/", SyntaxKind::Y_KW),
        ("y(abc)(xyz)", SyntaxKind::Y_KW),
        ("y[abc][xyz]", SyntaxKind::Y_KW),
        ("y{abc}{xyz}", SyntaxKind::Y_KW),
    ];

    for (input, expected_kind) in test_cases {
        let mut lexer = Lexer::new(input);
        assert_eq!(
            lexer.next_token(),
            Some((
                expected_kind,
                &input[..if input.starts_with("tr") { 2 } else { 1 }]
            ))
        );
    }
}

#[test]
fn test_q_basic_parsing() {
    // Test basic q() parsing with parentheses
    let mut lexer = Lexer::new("q(hello)");
    // Begin quote-like mode for lexer-only test
    lexer.begin_quote_like(SyntaxKind::Q_KW, crate::lexer::QuoteLikeMode::Q);
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!("Token: {:?}, Text: {:?}", kind, text);
        tokens.push((kind, text));
    }

    // Debug the token sequence
    assert_eq!(tokens.len(), 4); // Should have: Q_KW, DELIMITER, Q_STRING, DELIMITER
    assert_eq!(tokens[0], (SyntaxKind::Q_KW, "q"));
    assert_eq!(tokens[1], (SyntaxKind::DELIMITER, "("));
    assert_eq!(tokens[2], (SyntaxKind::LITERAL_STRING, "hello"));
    assert_eq!(tokens[3], (SyntaxKind::DELIMITER, ")"));
}

#[test]
fn test_full_q_expression() {
    // Test a complete q expression as would appear in a statement
    let mut lexer = Lexer::new("print q(hello);");
    // Step like the parser: IDENT, WS, Q_KW then begin quote-like
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "print")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::Q_KW, "q")));
    lexer.begin_quote_like(SyntaxKind::Q_KW, crate::lexer::QuoteLikeMode::Q);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::LITERAL_STRING, "hello"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::SEMICOLON, ";")));
}

#[test]
fn test_debug_q_simple() {
    let mut lexer = Lexer::new("q(hello)");
    lexer.begin_quote_like(SyntaxKind::Q_KW, crate::lexer::QuoteLikeMode::Q);
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!("Token: {:?}, Text: {:?}", kind, text);
        tokens.push((kind, text));
    }

    // Check that we get the expected tokens
    assert_eq!(tokens[0], (SyntaxKind::Q_KW, "q"));
    assert_eq!(tokens[1], (SyntaxKind::DELIMITER, "("));
    assert_eq!(tokens[2], (SyntaxKind::LITERAL_STRING, "hello"));
    assert_eq!(tokens[3], (SyntaxKind::DELIMITER, ")"));
}

#[test]
fn test_debug_print_q_tokens() {
    let mut lexer = Lexer::new("print q(hello);");
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!("Token: {:?}, Text: {:?}", kind, text);
        tokens.push((kind, text));
    }

    // Print all tokens for debugging
    println!("All tokens: {:?}", tokens);
}

#[test]
fn test_debug_parser_lexer_sync() {
    // Test that mimics how the parser uses the lexer
    let mut lexer = Lexer::new("print q(hello);");

    // Mimic parser initialization: get first token
    let mut current_token = lexer.next_token();
    println!("Initial token: {:?}", current_token);

    // Mimic parser.bump() for each token
    while let Some((kind, text)) = current_token {
        println!("Parser processing token: {:?}, Text: {:?}", kind, text);
        current_token = lexer.next_token(); // This is what parser.bump() does
    }
}

#[test]
fn test_qw_context_debug() {
    // Test QW lexer context transitions for debugging
    let mut lexer = Lexer::new("my @a = qw(a);");
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!("Token: {:?}, Text: {:?}", kind, text);
        tokens.push((kind, text));
    }

    // Print all tokens for debugging
    println!("All tokens: {:?}", tokens);

    // Check the last few tokens to see what's happening
    let len = tokens.len();
    if len >= 3 {
        println!("Last 3 tokens: {:?}", &tokens[len - 3..]);
    }
}
