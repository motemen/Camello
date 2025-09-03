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
            "Expected {} token in {}",
            expected_deref, input
        );
    }
}

#[test]
fn test_percent_modulo_vs_sigil() {
    // Test the critical case mentioned by Gemini: $var % other_var should be modulo
    let mut lexer = Lexer::new("$var % other_var");

    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "var")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%"))); // Should be MODULO, not PERCENT
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "other_var")));
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
fn test_array_modulo_expression() {
    // Test that "@array % hash" correctly identifies % as modulo operator
    let mut lexer = Lexer::new("@array % hash");

    assert_eq!(lexer.next_token(), Some((SyntaxKind::AT, "@")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "array")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::MODULO, "%"))); // Should be MODULO (operator)
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "hash")));
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
fn test_string_comparison_operators() {
    // Test that 'eq' is an operator when expecting an operator
    let mut lexer = Lexer::new(r#"$a eq "b""#);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_EQ, "eq")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STRING, r#""b""#)));

    // Test that 'ne' is an operator when expecting an operator
    let mut lexer = Lexer::new(r#"$a ne "b""#);
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "a")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_NE, "ne")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STRING, r#""b""#)));

    // Test that 'gt' is an operator
    let mut lexer = Lexer::new(r#"$a gt "b""#);
    lexer.next_token(); // $
    lexer.next_token(); // a
    lexer.next_token(); // whitespace
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_GT, "gt")));

    // Test that 'lt' is an operator
    let mut lexer = Lexer::new(r#"$a lt "b""#);
    lexer.next_token();
    lexer.next_token();
    lexer.next_token();
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_LT, "lt")));

    // Test that 'ge' is an operator
    let mut lexer = Lexer::new(r#"$a ge "b""#);
    lexer.next_token();
    lexer.next_token();
    lexer.next_token();
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_GE, "ge")));

    // Test that 'le' is an operator
    let mut lexer = Lexer::new(r#"$a le "b""#);
    lexer.next_token();
    lexer.next_token();
    lexer.next_token();
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_LE, "le")));

    // Test that 'cmp' is an operator
    let mut lexer = Lexer::new(r#"$a cmp "b""#);
    lexer.next_token();
    lexer.next_token();
    lexer.next_token();
    assert_eq!(lexer.next_token(), Some((SyntaxKind::STR_CMP, "cmp")));

    // Test that 'eq' is an identifier when expecting a value
    let mut lexer = Lexer::new("sub eq { }");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "eq")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::L_BRACE, "{")));

    // Test that 'ne' is an identifier when expecting a value
    let mut lexer = Lexer::new("my $ne;");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "ne")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::SEMICOLON, ";")));

    // Test that 'gt' is an identifier
    let mut lexer = Lexer::new("sub gt {}");
    lexer.next_token(); // sub
    lexer.next_token(); // whitespace
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "gt")));
}

#[test]
fn test_s_operator_disambiguation() {
    // Test that 's' is recognized as an operator when followed by delimiters (baseline for tr/y)
    let mut lexer = Lexer::new("$str s/abc/xyz/");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "str")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
}

#[test]
fn test_tr_operator_disambiguation() {
    // Test that 'tr' is recognized as an operator when followed by delimiters
    let mut lexer = Lexer::new("$str tr/abc/xyz/");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "str")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::TR_KW, "tr")));
}

#[test]
fn test_tr_as_function_name() {
    // Test that 'tr' is an identifier when used as function name
    let mut lexer = Lexer::new("sub tr {}");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "tr")));
}

#[test]
fn test_tr_as_variable_name() {
    // Test that 'tr' is an identifier when used as variable name
    let mut lexer = Lexer::new("my $tr = 1;");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "tr")));
}

#[test]
fn test_y_operator_disambiguation() {
    // Test that 'y' is recognized as an operator when followed by delimiters
    let mut lexer = Lexer::new("$str y/abc/xyz/");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "str")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::Y_KW, "y")));
}

#[test]
fn test_y_as_function_name() {
    // Test that 'y' is an identifier when used as function name
    let mut lexer = Lexer::new("sub y {}");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::SUB_KW, "sub")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "y")));
}

#[test]
fn test_y_as_variable_name() {
    // Test that 'y' is an identifier when used as variable name
    let mut lexer = Lexer::new("my $y = 1;");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::MY_KW, "my")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DOLLAR, "$")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::IDENT, "y")));
}

#[test]
fn test_substitution_lexing() {
    // Test basic s/pattern/replacement/ lexing
    let mut lexer = Lexer::new("s/world/universe/");

    // First token should be S_KW
    let token1 = lexer.next_token();
    println!(
        "Token 1: {:?}, Context: {:?}, State: {:?}",
        token1, lexer.context, lexer.quote_like_state
    );
    assert_eq!(token1, Some((SyntaxKind::S_KW, "s")));

    // Should be in substitution context now
    assert_eq!(lexer.context, LexerContext::InQuoteLike);

    // Next should be opening delimiter
    let token2 = lexer.next_token();
    println!(
        "Token 2: {:?}, Context: {:?}, State: {:?}",
        token2, lexer.context, lexer.quote_like_state
    );
    assert_eq!(token2, Some((SyntaxKind::DELIMITER, "/")));

    // After first delimiter, should be in InSearchList state
    if let Some(ref state) = lexer.quote_like_state {
        assert_eq!(state.part, QuoteLikePart::InSearchList);
    }

    // Pattern content
    let token3 = lexer.next_token();
    println!(
        "Token 3: {:?}, Context: {:?}, State: {:?}",
        token3, lexer.context, lexer.quote_like_state
    );
    assert_eq!(token3, Some((SyntaxKind::S_PATTERN, "world")));

    // Middle delimiter
    let token4 = lexer.next_token();
    println!(
        "Token 4: {:?}, Context: {:?}, State: {:?}",
        token4, lexer.context, lexer.quote_like_state
    );
    assert_eq!(token4, Some((SyntaxKind::DELIMITER, "/")));

    // Replacement content
    let token5 = lexer.next_token();
    println!(
        "Token 5: {:?}, Context: {:?}, State: {:?}",
        token5, lexer.context, lexer.quote_like_state
    );
    assert_eq!(token5, Some((SyntaxKind::S_REPLACEMENT, "universe")));

    // Closing delimiter
    let token6 = lexer.next_token();
    println!(
        "Token 6: {:?}, Context: {:?}, State: {:?}",
        token6, lexer.context, lexer.quote_like_state
    );
    assert_eq!(token6, Some((SyntaxKind::DELIMITER, "/")));

    // Should be in ExpectingQuoteLikeFlags context after the closing delimiter
    assert_eq!(lexer.context, LexerContext::ExpectingQuoteLikeFlags);
}

#[test]
fn test_substitution_with_flags() {
    // Test s/pattern/replacement/flags lexing - this demonstrates the current issue
    let mut lexer = Lexer::new("s/world/universe/gi");

    // Parse through the substitution operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_PATTERN, "world")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(
        lexer.next_token(),
        Some((SyntaxKind::S_REPLACEMENT, "universe"))
    );
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

    // Now the lexer should parse flags as a single token
    let flags_token = lexer.next_token();
    println!(
        "Flags token: {:?}, Context: {:?}",
        flags_token, lexer.context
    );

    // This should now correctly parse as S_FLAGS
    assert_eq!(flags_token, Some((SyntaxKind::S_FLAGS, "gi")));
}

#[test]
fn test_tr_with_flags() {
    // Test tr/searchlist/replacementlist/flags lexing
    let mut lexer = Lexer::new("tr/abc/XYZ/d");

    // Parse through the transliteration operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::TR_KW, "tr")));
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
    println!(
        "TR Flags token: {:?}, Context: {:?}",
        flags_token, lexer.context
    );

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
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::S_PATTERN, "a")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
        assert_eq!(lexer.next_token(), Some((SyntaxKind::S_REPLACEMENT, "b")));
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
fn test_no_flags_handling() {
    // Test operators without flags

    let mut lexer = Lexer::new("s/a/b/");

    // Parse through the substitution operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_PATTERN, "a")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_REPLACEMENT, "b")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

    // Should be in ExpectingQuoteLikeFlags context after the closing delimiter
    assert_eq!(lexer.context, LexerContext::ExpectingQuoteLikeFlags);

    // The next call to next_token should handle the empty flags case and return None
    // This should also transition the context back to ExpectingOperator
    assert_eq!(lexer.next_token(), None);
    assert_eq!(lexer.context, LexerContext::ExpectingOperator);
}

#[test]
fn test_invalid_flags_rejected() {
    // Test that invalid flags are not consumed as flags

    let mut lexer = Lexer::new("s/a/b/xyz"); // 'z' is not a valid s/// flag

    // Parse through the substitution operator
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_KW, "s")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_PATTERN, "a")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_REPLACEMENT, "b")));
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
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_PATTERN, "a")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::S_REPLACEMENT, "b")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));

    // The entire flag sequence should be treated as an error
    let flags_token = lexer.next_token();
    assert_eq!(flags_token, Some((SyntaxKind::ERROR, "giz")));

    // Should be at end of input
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_asymmetric_delimiters() {
    let input = "$text =~ s{old}{new}g;";
    let mut lexer = Lexer::new(input);

    println!("Testing asymmetric delimiters: {}", input);

    let mut token_num = 1;
    while let Some((token, text)) = lexer.next_token() {
        println!(
            "Token {}: Some(({:?}, {:?})), Context: {:?}, State: {:?}",
            token_num, token, text, lexer.context, lexer.quote_like_state
        );
        token_num += 1;

        if token_num > 20 {
            break;
        }
    }
}

#[test]
fn test_qw_basic_parsing() {
    // Test basic qw() parsing with parentheses
    let mut lexer = Lexer::new("qw(hello world)");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
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
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "x:y")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "z")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "/")));
    assert_eq!(lexer.next_token(), None);

    // Test qw with bracket delimiters
    let mut lexer = Lexer::new("qw[foo bar]");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "[")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "foo")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::WHITESPACE, " ")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_STRING, "bar")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "]")));
    assert_eq!(lexer.next_token(), None);

    // Test qw with brace delimiters
    let mut lexer = Lexer::new("qw{alpha beta}");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
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
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, "(")));
    assert_eq!(lexer.next_token(), Some((SyntaxKind::DELIMITER, ")")));
    assert_eq!(lexer.next_token(), None);
}

#[test]
fn test_qw_with_whitespace() {
    // Test qw with extra whitespace
    let mut lexer = Lexer::new("qw(  hello   world  )");
    assert_eq!(lexer.next_token(), Some((SyntaxKind::QW_KW, "qw")));
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
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!(
            "Token: {:?}, Text: {:?}, Context: {:?}",
            kind, text, lexer.context
        );
        tokens.push((kind, text));
    }

    // Debug the token sequence
    assert_eq!(tokens.len(), 4); // Should have: Q_KW, DELIMITER, Q_STRING, DELIMITER
    assert_eq!(tokens[0], (SyntaxKind::Q_KW, "q"));
    assert_eq!(tokens[1], (SyntaxKind::DELIMITER, "("));
    assert_eq!(tokens[2], (SyntaxKind::Q_STRING, "hello"));
    assert_eq!(tokens[3], (SyntaxKind::DELIMITER, ")"));
}

#[test]
fn test_full_q_expression() {
    // Test a complete q expression as would appear in a statement
    let mut lexer = Lexer::new("print q(hello);");
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!(
            "Full expression Token: {:?}, Text: {:?}, Context: {:?}",
            kind, text, lexer.context
        );
        tokens.push((kind, text));
    }

    // Should have: IDENT, WHITESPACE, Q_KW, DELIMITER, Q_STRING, DELIMITER, SEMICOLON
    assert!(tokens.len() >= 5);
    // Find the Q_KW token
    let q_pos = tokens
        .iter()
        .position(|(kind, _)| *kind == SyntaxKind::Q_KW)
        .unwrap();
    assert_eq!(tokens[q_pos], (SyntaxKind::Q_KW, "q"));
    assert_eq!(tokens[q_pos + 1], (SyntaxKind::DELIMITER, "("));
    assert_eq!(tokens[q_pos + 2], (SyntaxKind::Q_STRING, "hello"));
    assert_eq!(tokens[q_pos + 3], (SyntaxKind::DELIMITER, ")"));
}

#[test]
fn test_debug_q_simple() {
    let mut lexer = Lexer::new("q(hello)");
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!(
            "Token: {:?}, Text: {:?}, Context: {:?}",
            kind, text, lexer.context
        );
        tokens.push((kind, text));
    }

    // Check that we get the expected tokens
    assert_eq!(tokens[0], (SyntaxKind::Q_KW, "q"));
    assert_eq!(tokens[1], (SyntaxKind::DELIMITER, "("));
    assert_eq!(tokens[2], (SyntaxKind::Q_STRING, "hello"));
    assert_eq!(tokens[3], (SyntaxKind::DELIMITER, ")"));
}

#[test]
fn test_debug_print_q_tokens() {
    let mut lexer = Lexer::new("print q(hello);");
    let mut tokens = Vec::new();

    while let Some((kind, text)) = lexer.next_token() {
        println!(
            "Token: {:?}, Text: {:?}, Context: {:?}",
            kind, text, lexer.context
        );
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
        println!(
            "Parser processing token: {:?}, Text: {:?}, Lexer context: {:?}",
            kind, text, lexer.context
        );
        current_token = lexer.next_token(); // This is what parser.bump() does
    }
}
