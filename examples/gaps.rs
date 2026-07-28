use std::{fs, path::Path};
fn walk(dir: &Path, acc: &mut Vec<std::path::PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, acc)
        } else if p.extension().is_some_and(|x| x == "pl") {
            acc.push(p)
        }
    }
}
fn main() {
    let mut files = vec![];
    walk(Path::new("src/formatter/fixtures"), &mut files);
    walk(Path::new("src/parser/fixtures/success"), &mut files);
    files.sort();
    for f in &files {
        let src = fs::read_to_string(f).unwrap();
        let p = camello::parse::parse(&src);
        if p.diagnostics.is_empty() {
            continue;
        }
        println!("{} ({} diags)", f.display(), p.diagnostics.len());
        for d in p.diagnostics.iter().take(2) {
            let line = src[..usize::from(d.range.start())].lines().count();
            println!(
                "   L{line}: {} | {}",
                d.message,
                src.lines().nth(line.saturating_sub(1)).unwrap_or("").trim()
            );
        }
    }
}
