use crate::{parser::ParseError, PerlNode};
use miette::{GraphicalReportHandler, GraphicalTheme};
use std::fs;
use std::path::{Path, PathBuf};

pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/parser/fixtures")
}

pub fn render_success_tree(node: &PerlNode) -> String {
    format!("{node:#?}")
}

pub fn render_errors(errors: &[ParseError]) -> String {
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor());
    errors
        .iter()
        .map(|err| {
            let mut rendered = String::new();
            handler
                .render_report(&mut rendered, err)
                .expect("failed to render report");
            rendered.trim_end().to_owned()
        })
        .collect::<Vec<String>>()
        .join("\n")
}

pub fn collect_fixture_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    for entry in fs::read_dir(dir).expect("Failed to read fixture directory") {
        let entry = entry.expect("Failed to read fixture entry");
        let path = entry.path();
        if path.is_dir() {
            collect_fixture_files(&path, acc);
        } else if path.extension().map(|ext| ext == "pl").unwrap_or(false) {
            acc.push(path);
        }
    }
}
