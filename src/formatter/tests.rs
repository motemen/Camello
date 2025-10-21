use std::fs;
use std::path::{Path, PathBuf};

use crate::{format, format_with_options, parse_perl, FormatterOptions};

fn format_and_assert(input: &str) -> String {
    let (syntax, err) = parse_perl(input);
    assert!(err.is_empty(), "Parse errors for '{}': {:?}", input, err);
    format(&syntax)
}

fn collect_fixture_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("Failed to read fixtures directory") {
        let entry = entry.expect("Failed to read fixture entry");
        let path = entry.path();
        if path.is_dir() {
            collect_fixture_files(&path, acc);
        } else if path.extension().map(|ext| ext == "pl").unwrap_or(false) {
            acc.push(path);
        }
    }
}

#[test]
fn formatter_fixture_snapshots() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/formatter/fixtures");
    let mut files = Vec::new();
    collect_fixture_files(&fixture_dir, &mut files);
    files.sort();

    assert!(
        !files.is_empty(),
        "No fixture files found in {:?}",
        fixture_dir
    );

    for path in files {
        let relative = path
            .strip_prefix(&fixture_dir)
            .expect("Fixture path should be under fixture directory");
        let snapshot_name = relative
            .iter()
            .map(|component| component.to_string_lossy().into_owned())
            .collect::<Vec<String>>()
            .join("__");

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("Failed to read {}: {}", path.display(), err));
        let formatted = format_and_assert(&source);

        insta::assert_snapshot!(snapshot_name, formatted);
    }
}

#[test]
fn compound_assignment_alignment_can_be_disabled() {
    let source = include_str!("fixtures/compound_assignment_alignment.pl");
    let (syntax, err) = parse_perl(source);
    assert!(err.is_empty(), "Parse errors for fixture: {:?}", err);

    let options = FormatterOptions::default().with_align_compound_assignments(false);
    let formatted = format_with_options(&syntax, options);

    insta::assert_snapshot!("compound_assignment_alignment_disabled", formatted);
}
