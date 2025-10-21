use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    format_with_options, parse_perl, DelimiterTightness, DelimiterTightnessConfig, FormatterOptions,
};

fn format_and_assert(input: &str, options: &FormatterOptions) -> String {
    let (syntax, err) = parse_perl(input);
    assert!(err.is_empty(), "Parse errors for '{}': {:?}", input, err);
    format_with_options(&syntax, options.clone())
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

fn run_fixture_suite(
    fixture_dir: &Path,
    options: &FormatterOptions,
    snapshot_prefix: Option<&str>,
) {
    let mut files = Vec::new();
    collect_fixture_files(fixture_dir, &mut files);
    files.sort();

    assert!(
        !files.is_empty(),
        "No fixture files found in {:?}",
        fixture_dir
    );

    for path in files {
        let relative = path
            .strip_prefix(fixture_dir)
            .expect("Fixture path should be under fixture directory");
        let mut components = Vec::new();
        if let Some(prefix) = snapshot_prefix {
            components.push(prefix.to_owned());
        }
        components.extend(
            relative
                .iter()
                .map(|component| component.to_string_lossy().into_owned()),
        );
        let snapshot_name = components.join("__");

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("Failed to read {}: {}", path.display(), err));
        let formatted = format_and_assert(&source, options);

        insta::assert_snapshot!(snapshot_name, formatted);
    }
}

#[test]
fn formatter_fixture_snapshots() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/formatter/fixtures");
    let options = FormatterOptions::default();
    run_fixture_suite(&fixture_dir, &options, None);
}

#[test]
fn formatter_loose_delimiter_snapshots() {
    let fixture_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/formatter/fixtures_loose_delimiters");
    let options = FormatterOptions::default().with_delimiter_tightness(
        DelimiterTightnessConfig::default()
            .with_braces(DelimiterTightness::Loose)
            .with_brackets(DelimiterTightness::Loose),
    );
    run_fixture_suite(&fixture_dir, &options, Some("loose_delimiters"));
}
