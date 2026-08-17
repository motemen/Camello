//! Lexer tests, including the seven reproduced lexing bugs from
//! `notes/2026-07-28-redesign-assessment.md` appendix A (D1-D7).

use super::{Expect, Lexer};
use crate::lang::{TokenKind, T};

/// A stand-in for the parser's `set_expect` discipline.
///
/// The real parser sets `expect` from grammar position; here we approximate it
/// from the token just consumed, which is enough to exercise every lexical rule.
/// Tokens produced inside an atomic run keep `Term` because the whole run was
/// scanned under it (ADR 0005 §3).
fn expect_after(kind: TokenKind) -> Expect {
    match kind {
        TokenKind::IDENT
        | TokenKind::NUMBER
        | TokenKind::VERSION
        | TokenKind::STRING
        | TokenKind::REGEX_FLAGS
        | TokenKind::HEREDOC_END
        | TokenKind::RAW_CONTENT
        | T!["}"]
        | T![")"]
        | T!["]"] => Expect::Operator,
        _ => Expect::Term,
    }
}

/// Every token, trivia included, as `(kind, text)`.
fn lex(source: &str) -> Vec<(TokenKind, String)> {
    let mut lexer = Lexer::new(source);
    while let Some(token) = lexer.bump() {
        lexer.set_expect(expect_after(token.kind));
    }
    lexer
        .scan_all()
        .iter()
        .map(|token| (token.kind, token.text(source).to_string()))
        .collect()
}

/// Just the kinds, trivia dropped.
fn kinds(source: &str) -> Vec<TokenKind> {
    strip_trivia(lex(source))
}

fn strip_trivia(tokens: Vec<(TokenKind, String)>) -> Vec<TokenKind> {
    tokens
        .into_iter()
        .filter(|(kind, _)| !kind.is_trivia())
        .map(|(kind, _)| kind)
        .collect()
}

/// Concatenating every token must reproduce the input exactly (ADR 0006 §6).
#[track_caller]
fn assert_lossless(source: &str) {
    let rebuilt: String = lex(source).into_iter().map(|(_, text)| text).collect();
    assert_eq!(rebuilt, source, "token stream is not lossless");
}

#[test]
fn basic_tokens() {
    assert_eq!(
        kinds("my $x = 1;"),
        vec![
            T!["my"],
            TokenKind::SCALAR_SIGIL,
            TokenKind::IDENT,
            T!["="],
            TokenKind::NUMBER,
            T![";"],
        ]
    );
    assert_lossless("my $x = 1;\n");
}

#[test]
fn trivia_splits_newlines_one_at_a_time() {
    // ADR 0006 §1: blank lines have to survive as consecutive NEWLINEs or the
    // formatter is forced back to re-reading the source to count them.
    let tokens = lex("1;\n\n\n2;");
    let newlines = tokens
        .iter()
        .filter(|(kind, _)| *kind == TokenKind::NEWLINE)
        .count();
    assert_eq!(newlines, 3);
    assert!(tokens
        .iter()
        .all(|(kind, text)| *kind != TokenKind::WHITESPACE || !text.contains('\n')));
}

#[test]
fn compound_assignment_is_one_token() {
    // ADR 0005 §5 / ADR 0004 §4: no COMPOUND_ASSIGNMENT node, because there is
    // nothing left to compose.
    assert_eq!(
        kinds("$x //= 1;"),
        vec![
            TokenKind::SCALAR_SIGIL,
            TokenKind::IDENT,
            T!["//="],
            TokenKind::NUMBER,
            T![";"]
        ]
    );
    assert_eq!(kinds("$x **= 2;")[2], T!["**="]);
    assert_eq!(kinds("$x .= 2;")[2], T![".="]);
}

// ===== D1: an indented `=head1` is not POD =====

#[test]
fn pod_is_recognised_only_in_column_zero() {
    let indented = "sub f {\n    =head1 x\n}\n";
    let tokens = kinds(indented);
    assert!(
        !tokens.contains(&TokenKind::POD_CONTENT),
        "indented `=head1` must stay code: perl only starts POD in column 0"
    );
    // And the block still closes, rather than the rest of the file being eaten.
    assert_eq!(tokens.last(), Some(&T!["}"]));

    let column_zero = "sub f {\n}\n=head1 NAME\n\ntext\n\n=cut\n";
    assert!(kinds(column_zero).contains(&TokenKind::POD_CONTENT));
    assert_lossless(column_zero);
}

// ===== D2: an unterminated quote-like does not poison the rest of the file ====

#[test]
fn unterminated_quote_like_yields_one_error_token() {
    let source = "q{unterminated;\nmy $x = 1;\n";
    assert_eq!(
        kinds(source),
        vec![TokenKind::UNTERMINATED_QUOTE_LIKE],
        "one construct, one token, one diagnostic — and no mode left switched on"
    );
    assert_lossless(source);
}

#[test]
fn unterminated_regex_yields_one_error_token() {
    let source = "my $x = /abc\n";
    assert_eq!(kinds(source).last(), Some(&TokenKind::UNTERMINATED_REGEX));
    assert_lossless(source);
}

#[test]
fn unterminated_string_yields_one_error_token() {
    let source = "my $x = \"abc\n";
    assert_eq!(kinds(source).last(), Some(&TokenKind::UNTERMINATED_STRING));
    assert_lossless(source);
}

// ===== D3: lookahead sees quote-like operators =====

#[test]
fn lookahead_sees_through_quote_like_operators() {
    // The old lexer implemented lookahead by cloning itself, so a mode the
    // parser was about to switch on was invisible and `qq{x}` came back as a
    // fictional token run. Here the run is already in the buffer, so peeking
    // past it is ordinary indexing.
    let mut lexer = Lexer::new("{ qq{x} => 1 };");
    assert_eq!(lexer.peek_kind(0), Some(T!["{"]));
    assert_eq!(lexer.peek_kind(1), Some(T!["qq"]));
    assert_eq!(lexer.peek_kind(2), Some(TokenKind::DELIMITER));
    assert_eq!(lexer.peek_kind(3), Some(TokenKind::INTERPOLATED_STRING));
    assert_eq!(lexer.peek_kind(4), Some(TokenKind::DELIMITER));
    // The token that decides hash-ref versus block is reachable by lookahead.
    assert_eq!(lexer.peek_kind(5), Some(T!["=>"]));
}

// ===== D4: whether `/` starts a regex does not depend on the rest of the file =

#[test]
fn slash_disambiguation_is_local() {
    // `$total / 2` lexes the same whether or not a second `/` appears later.
    let short = kinds("$total / 2;");
    let long = kinds("$total / 2 + $count / 3;");
    let shared = short.len() - 1;
    assert_eq!(
        &long[..shared],
        &short[..shared],
        "the shared prefix must lex identically"
    );
    assert!(short.contains(&T!["/"]), "operator position means division");

    // In term position `/` commits to a match without searching first.
    assert_eq!(
        kinds("$x =~ /abc/;"),
        vec![
            TokenKind::SCALAR_SIGIL,
            TokenKind::IDENT,
            T!["=~"],
            TokenKind::DELIMITER,
            TokenKind::REGEX_PATTERN,
            TokenKind::DELIMITER,
            T![";"],
        ]
    );
}

// ===== D5: `q #hello#` is `q` followed by a comment =====

#[test]
fn hash_after_space_is_a_comment_not_a_delimiter() {
    // Perl skips a whitespace-separated `#` as a comment and keeps looking for
    // the delimiter, so the string here is `hello` and `#not a delimiter` is a
    // comment. The old lexer took the `#` as the delimiter.
    let source = "q #not a delimiter\n{hello}";
    let tokens = lex(source);
    assert!(
        tokens
            .iter()
            .any(|(kind, text)| *kind == TokenKind::COMMENT && text.contains("not a delimiter")),
        "got {tokens:?}"
    );
    assert!(tokens
        .iter()
        .any(|(kind, text)| *kind == TokenKind::LITERAL_STRING && text == "hello"));
    assert_lossless(source);

    // With no whitespace between, `#` really is the delimiter.
    assert_eq!(
        kinds("q#hello#"),
        vec![
            T!["q"],
            TokenKind::DELIMITER,
            TokenKind::LITERAL_STRING,
            TokenKind::DELIMITER
        ]
    );

    // And `q #hello#` on its own really is unterminated, exactly as in perl —
    // the comment ate the line and no delimiter ever appeared.
    assert_eq!(
        kinds("q #hello#\n"),
        vec![TokenKind::UNTERMINATED_QUOTE_LIKE]
    );
}

// ===== D6: unusual prototype characters are legal =====

#[test]
fn prototypes_are_raw_text() {
    for prototype in ["(_)", "(+)", "(\\[$@])", "($$;$)", "()"] {
        let source = format!("sub f{prototype} {{}}");
        let mut lexer = Lexer::new(&source);
        lexer.bump(); // `sub`
        lexer.bump(); // `f`
        let (open, body, close) = lexer
            .take_raw_parens()
            .unwrap_or_else(|| panic!("no paren group in {source}"));
        assert_eq!(open.kind, T!["("]);
        assert_eq!(body.kind, TokenKind::RAW_CONTENT);
        assert_eq!(close.kind, T![")"]);
        assert_eq!(
            body.text(&source),
            &prototype[1..prototype.len() - 1],
            "prototype body must survive verbatim"
        );
    }
}

// ===== D7: `foo %h` and `foo % h` are decided by expect alone =====

#[test]
fn sigils_win_in_term_position_regardless_of_spacing() {
    let mut spaced = Lexer::new("%  h");
    assert_eq!(spaced.peek_kind(0), Some(TokenKind::HASH_SIGIL));

    let mut tight = Lexer::new("%h");
    assert_eq!(tight.peek_kind(0), Some(TokenKind::HASH_SIGIL));

    // And in operator position it is modulo, again regardless of spacing.
    let mut operator = Lexer::new("%h");
    operator.set_expect(Expect::Operator);
    assert_eq!(operator.peek_kind(0), Some(TokenKind::MODULO));
}

// ===== Scanner rules from ADR 0005 §5 =====

#[test]
fn radix_literals_do_not_swallow_the_range_operator() {
    let texts =
        |source: &str| -> Vec<String> { lex(source).into_iter().map(|(_, text)| text).collect() };
    assert_eq!(texts("0x7f..0xff"), vec!["0x7f", "..", "0xff"]);
    assert_eq!(texts("1..5"), vec!["1", "..", "5"]);
}

#[test]
fn repetition_operator_splits_from_its_count() {
    let mut operator = Lexer::new("x5");
    operator.set_expect(Expect::Operator);
    assert_eq!(operator.peek_kind(0), Some(T!["x"]));
    assert_eq!(operator.peek_text(0), Some("x"));

    // In term position the same text is one identifier.
    let mut term = Lexer::new("x5");
    assert_eq!(term.peek_kind(0), Some(TokenKind::IDENT));
    assert_eq!(term.peek_text(0), Some("x5"));
}

#[test]
fn file_test_operators_use_the_real_character_set() {
    let mut file_test = Lexer::new("-e $path");
    assert_eq!(file_test.peek_kind(0), Some(TokenKind::FILE_TEST_OP));

    // `-q` is not a file test; the old lexer accepted any single letter.
    let mut not_a_test = Lexer::new("-q $path");
    assert_ne!(not_a_test.peek_kind(0), Some(TokenKind::FILE_TEST_OP));

    // Nor is `-exists`, which is negation of a bareword.
    let mut word = Lexer::new("-exists");
    assert_ne!(word.peek_kind(0), Some(TokenKind::FILE_TEST_OP));
}

#[test]
fn quote_like_keywords_stay_barewords_before_fat_comma_or_brace() {
    assert_eq!(kinds("(s => 1)")[1], TokenKind::IDENT);
    assert_eq!(kinds("$h{q}")[3], TokenKind::IDENT);
    // But `s{a}{b}` is still a substitution.
    assert_eq!(
        kinds("s{a}{b}"),
        vec![
            T!["s"],
            TokenKind::DELIMITER,
            TokenKind::REGEX_PATTERN,
            TokenKind::DELIMITER,
            TokenKind::DELIMITER,
            TokenKind::INTERPOLATED_STRING,
            TokenKind::DELIMITER,
        ]
    );
}

#[test]
fn heredoc_body_follows_at_the_next_line_start() {
    let source = "my $x = <<EOF;\nline1\nline2\nEOF\nmy $y = 2;\n";
    let tokens = kinds(source);
    assert!(tokens.contains(&TokenKind::HEREDOC_START));
    assert!(tokens.contains(&TokenKind::HEREDOC_CONTENT));
    assert!(tokens.contains(&TokenKind::HEREDOC_END));
    assert_lossless(source);
}

/// A quoted terminator may be held off from the `<<`; a bare one may not.
///
/// perl forbids `<< EOF` (5.28 onwards) and reads `1 << 2` as a left shift, so
/// only the quoted spelling can be told from a shift by looking at it.
#[test]
fn a_space_before_a_quoted_heredoc_terminator_is_allowed() {
    let source = "my $x = << \"END\";\nbody\nEND\n";
    let tokens = kinds(source);
    assert!(tokens.contains(&TokenKind::HEREDOC_START));
    assert!(tokens.contains(&TokenKind::HEREDOC_END));
    assert_lossless(source);

    let bare = "my $x = << END;\n";
    assert!(!kinds(bare).contains(&TokenKind::HEREDOC_START));
    assert_lossless(bare);
}

#[test]
fn unterminated_heredoc_is_reported_not_guessed() {
    let source = "my $x = <<EOF;\nline1\n";
    assert!(kinds(source).contains(&TokenKind::UNTERMINATED_HEREDOC));
    assert_lossless(source);
}

#[test]
fn data_section_is_carried_verbatim() {
    let source = "my $x = 1;\n__DATA__\nnot; perl; at all\n";
    let data = lex(source)
        .into_iter()
        .find(|(kind, _)| *kind == TokenKind::DATA_CONTENT)
        .expect("data section token");
    assert_eq!(data.1, "not; perl; at all\n");
    assert_lossless(source);
}

// ===== Buffer and expect mechanics (ADR 0005 §2, §3) =====

#[test]
fn changing_expect_rescans_buffered_lookahead() {
    let mut lexer = Lexer::new("%h");
    assert_eq!(lexer.peek_kind(0), Some(TokenKind::HASH_SIGIL));
    lexer.set_expect(Expect::Operator);
    assert_eq!(
        lexer.peek_kind(0),
        Some(TokenKind::MODULO),
        "stale lookahead must not survive a change of expect"
    );
    lexer.set_expect(Expect::Term);
    assert_eq!(lexer.peek_kind(0), Some(TokenKind::HASH_SIGIL));
}

#[test]
fn rollback_restores_position_and_expect() {
    let mut lexer = Lexer::new("foo bar baz");
    let mark = lexer.mark();
    assert_eq!(lexer.bump().map(|token| token.kind), Some(TokenKind::IDENT));
    lexer.set_expect(Expect::Operator);
    lexer.rollback(mark);
    assert_eq!(lexer.expect(), Expect::Term);
    assert_eq!(lexer.peek_text(0), Some("foo"));
}

#[test]
fn heredoc_bookkeeping_survives_lookahead_invalidation() {
    // Peeking far enough to reach the body and then changing expect must
    // neither duplicate the body nor lose it.
    let source = "print <<EOF;\nbody\nEOF\n";
    let mut lexer = Lexer::new(source);
    for n in 0..6 {
        lexer.peek_kind(n);
    }
    lexer.set_expect(Expect::Operator);
    lexer.set_expect(Expect::Term);

    let bodies = lexer
        .scan_all()
        .iter()
        .filter(|token| token.kind == TokenKind::HEREDOC_CONTENT)
        .count();
    assert_eq!(bodies, 1);

    let rebuilt: String = lexer
        .scan_all()
        .iter()
        .map(|token| token.text(source))
        .collect();
    assert_eq!(rebuilt, source);
}

#[test]
fn rollback_drops_lookahead_scanned_under_another_expectation() {
    // The coherence guarantee of ADR 0005 §2 covers the whole buffer, not just
    // the token at the mark. `foo{sub}` reaches here through
    // `anon_hash_or_block`: the anonymous-hash attempt scans the `}` in operator
    // position, the attempt is rolled back, and the block re-parse arrives at
    // that same `}` expecting a term without ever changing `expect` — so nothing
    // invalidates it. Under a debug build the assertion in `bump` fired; under a
    // release build a token scanned as an operator was consumed as a term.
    let mut lexer = Lexer::new("{ sub }");
    let mark = lexer.mark();
    assert_eq!(lexer.expect(), Expect::Term);

    // The speculative attempt: walk to the `}` in operator position.
    lexer.bump();
    lexer.set_expect(Expect::Operator);
    while lexer.peek_kind(0).is_some_and(|kind| kind != T!["}"]) {
        lexer.bump();
    }
    assert_eq!(lexer.peek_kind(0), Some(T!["}"]));
    assert_eq!(
        lexer.peek(0).map(|token| token.expect_at_lex),
        Some(Expect::Operator)
    );

    lexer.rollback(mark);

    // Re-walk without ever leaving term position; the `}` must have been
    // re-scanned under it.
    lexer.bump();
    while lexer.peek_kind(0).is_some_and(|kind| kind != T!["}"]) {
        lexer.bump();
    }
    assert_eq!(
        lexer.peek(0).map(|token| token.expect_at_lex),
        Some(Expect::Term),
        "the rolled-back lookahead was left in the buffer"
    );
}

#[test]
fn a_comma_after_a_quote_like_keyword_is_its_delimiter() {
    // perl has no bareword exception for `,`: `(q, 1)` is a fatal "can't find
    // string terminator" and `m,b,` is a match. Reading the comma as a separator
    // turned `$v =~ m,/\z,,;` — which is what `HTTP::Config` contains — into a
    // bareword and an unterminated regex.
    assert_eq!(
        kinds("m,/\\z,"),
        vec![
            T!["m"],
            TokenKind::DELIMITER,
            TokenKind::REGEX_PATTERN,
            TokenKind::DELIMITER
        ]
    );
    assert_eq!(
        kinds("q,text,"),
        vec![
            T!["q"],
            TokenKind::DELIMITER,
            TokenKind::LITERAL_STRING,
            TokenKind::DELIMITER
        ]
    );
    assert_lossless("m,/\\z,,;");

    // The exceptions that do exist are unaffected.
    assert_eq!(kinds("s => 1")[0], TokenKind::IDENT);
    assert_eq!(kinds("{q}")[1], TokenKind::IDENT);
}

#[test]
fn a_second_heredoc_body_starts_on_the_next_line() {
    // `foo(<<A, <<B)` queues two bodies. The second begins after the first
    // terminator's line, and leaving that line terminator to the ordinary
    // scanner made it the first byte of B — so B held "\ntwo\n" and perl
    // printed a blank line. The token *stream* is identical either way, which
    // is why the invariants could not see it.
    let source = "foo(<<A, <<B);\none\nA\ntwo\nB\n";
    let bodies: Vec<String> = lex(source)
        .into_iter()
        .filter(|(kind, _)| *kind == TokenKind::HEREDOC_CONTENT)
        .map(|(_, text)| text)
        .collect();
    assert_eq!(bodies, vec!["one\n".to_string(), "two\n".to_string()]);
    assert_lossless(source);

    // Three, and an indented one, behave the same.
    let source = "f(<<~A, <<B, <<C);\n  one\n  A\ntwo\nB\nthree\nC\n";
    let bodies: Vec<String> = lex(source)
        .into_iter()
        .filter(|(kind, _)| *kind == TokenKind::HEREDOC_CONTENT)
        .map(|(_, text)| text)
        .collect();
    assert_eq!(
        bodies,
        vec![
            "  one\n".to_string(),
            "two\n".to_string(),
            "three\n".to_string()
        ]
    );
    assert_lossless(source);
}

#[test]
fn a_name_may_hold_characters_outside_ascii() {
    // Under `use utf8` perl accepts any word character in a name. Scanning as
    // ASCII did not reject `$café`, it split it, and `my $café = 1;` came out
    // as `my $caf é = 1;` — a syntax error where a working program had been.
    assert_eq!(
        kinds("my $café = 1;"),
        vec![
            T!["my"],
            TokenKind::SCALAR_SIGIL,
            TokenKind::IDENT,
            T!["="],
            TokenKind::NUMBER,
            T![";"]
        ]
    );
    assert_eq!(
        lex("$café")
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>(),
        vec!["$".to_string(), "café".to_string()]
    );
    assert_eq!(kinds("名前();")[0], TokenKind::IDENT);
    assert_eq!(kinds("my %メニュー;")[2], TokenKind::IDENT);
    assert_lossless("sub 名前 { my ($引数) = @_; }");
}

#[test]
fn a_leading_dot_is_a_number_where_a_term_is_expected() {
    let texts = |source: &str| {
        lex(source)
            .into_iter()
            .filter(|(kind, _)| !kind.is_trivia())
            .collect::<Vec<_>>()
    };

    assert_eq!(
        texts("$max * -.1"),
        vec![
            (TokenKind::SCALAR_SIGIL, "$".to_string()),
            (TokenKind::IDENT, "max".to_string()),
            (TokenKind::STAR, "*".to_string()),
            (T!["-"], "-".to_string()),
            (TokenKind::NUMBER, ".1".to_string()),
        ]
    );

    // Where an operator is expected the same character concatenates.
    assert_eq!(
        texts("$a .5"),
        vec![
            (TokenKind::SCALAR_SIGIL, "$".to_string()),
            (TokenKind::IDENT, "a".to_string()),
            (T!["."], ".".to_string()),
            (TokenKind::NUMBER, "5".to_string()),
        ]
    );

    assert_lossless("my $x = .5;");
    assert_lossless("$a .5");
}
