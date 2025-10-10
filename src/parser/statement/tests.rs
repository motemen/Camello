use crate::parser::test_utils::*;
use crate::{parse, PerlNode, SyntaxKind, T};
use std::fs;
use std::path::PathBuf;

fn statement_fixtures_root() -> PathBuf {
    fixtures_root().join("statements")
}

#[test]
fn statement_success_snapshots() {
    let success_dir = statement_fixtures_root().join("success");
    let mut files = Vec::new();
    collect_fixture_files(&success_dir, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "No statement success fixtures found in {:?}",
        success_dir
    );

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("Failed to read {}: {}", path.display(), err));
        let (green, errors) = parse(&source);
        assert!(
            errors.is_empty(),
            "Unexpected parse errors for {}: {:?}",
            path.display(),
            errors
        );

        let syntax = PerlNode::new_root(green);
        let relative = path
            .strip_prefix(&success_dir)
            .expect("Success fixture should live under success directory");
        let mut parts = relative
            .iter()
            .map(|component| component.to_string_lossy().into_owned())
            .collect::<Vec<String>>();
        if let Some(last) = parts.last_mut() {
            if let Some(stripped) = last.strip_suffix(".pl") {
                *last = stripped.to_string();
            }
        }
        let snapshot_name = format!("statements__success__{}", parts.join("__"));

        insta::assert_snapshot!(snapshot_name, render_success_tree(&syntax));
    }
}

#[test]
fn statement_error_snapshots() {
    let error_dir = statement_fixtures_root().join("errors");
    let mut files = Vec::new();
    collect_fixture_files(&error_dir, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "No statement error fixtures found in {:?}",
        error_dir
    );

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("Failed to read {}: {}", path.display(), err));
        let (_green, errors) = parse(&source);
        assert!(
            !errors.is_empty(),
            "Expected parse errors for {} but parser succeeded",
            path.display()
        );

        let relative = path
            .strip_prefix(&error_dir)
            .expect("Error fixture should live under statement error dir");
        let mut parts = relative
            .iter()
            .map(|component| component.to_string_lossy().into_owned())
            .collect::<Vec<String>>();
        if let Some(last) = parts.last_mut() {
            if let Some(stripped) = last.strip_suffix(".pl") {
                *last = stripped.to_string();
            }
        }
        let snapshot_name = format!("statements__errors__{}", parts.join("__"));

        insta::assert_snapshot!(snapshot_name, render_errors(&errors));
    }
}

#[test]
fn test_elsif_else_lookahead_functionality() {
    // Test that lookahead_for_elsif_or_else works with token-based lookahead
    // This method is used to peek ahead and see if elsif/else follows

    // Test with whitespace before keywords - this is the main use case
    let parser = crate::parser::Parser::new("  elsif");
    assert!(
        parser.lookahead_for_elsif_or_else(),
        "Should detect 'elsif' with leading whitespace"
    );

    let parser = crate::parser::Parser::new("\n\telse");
    assert!(
        parser.lookahead_for_elsif_or_else(),
        "Should detect 'else' with leading whitespace and newline"
    );

    // Test the realistic scenario - positioned after a closing brace, looking for elsif/else
    let parser = crate::parser::Parser::new("} elsif");
    assert_eq!(parser.current_kind(), Some(T!['}']));
    let mut parser = parser;
    parser.bump();
    assert!(
        parser.lookahead_for_elsif_or_else(),
        "Should detect 'elsif' after closing brace"
    );

    // Test cases where elsif/else should NOT be detected
    let should_not_detect_cases = ["foo", "my", "", "if", "while", "# comment\nfoo"];

    for input in should_not_detect_cases {
        let parser = crate::parser::Parser::new(input);
        assert!(
            !parser.lookahead_for_elsif_or_else(),
            "Should NOT detect elsif/else for input: '{}'",
            input
        );
    }

    // Test a simpler validation that if/elsif/else can be parsed correctly
    let full_if_input = "if (1) { } elsif (2) { }";
    let (green, errors) = crate::parse(full_if_input);
    assert!(
        errors.is_empty(),
        "Parse errors for '{}': {:?}",
        full_if_input,
        errors
    );
    let syntax = PerlNode::new_root(green);
    let has_if = syntax
        .descendants()
        .any(|node| node.kind() == SyntaxKind::IF_STMT);
    assert!(has_if, "Expected IF_STMT in parsed tree");
}
