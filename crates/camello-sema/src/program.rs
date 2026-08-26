//! The program graph (`docs/typecheck.md`, "The program model").
//!
//! A **package** is a name and the set of files that have a `package` statement
//! for it — many-to-many, because one file commonly holds several packages and
//! one package may, legally if rarely, span files. A package's declarations are
//! the union.
//!
//! Files are added in two kinds. A file **in the roots** is analysed in full
//! and may be reported against; a **dependency** contributes its declarations
//! and nothing else, and no diagnostic is ever reported against it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use camello_syntax::lang::SyntaxNode;

use crate::decl::{self, FileDecls, SubDecl};

/// One file, as the graph holds it.
pub struct FileEntry {
    pub path: PathBuf,
    pub decls: FileDecls,
    /// Whether a diagnostic may be reported against it.
    pub in_roots: bool,
}

/// Every declaration the run can see.
#[derive(Default)]
pub struct Program {
    files: Vec<FileEntry>,
    by_path: HashMap<PathBuf, usize>,
    subs: Vec<SubDecl>,
    /// `(package, name)` to the first declaration of it. First rather than
    /// last: a redefinition later in a file is usually a conditional
    /// alternative, and either answer is a guess.
    by_name: HashMap<(String, String), usize>,
    /// Which files declare a package, for method resolution.
    packages: HashMap<String, Vec<usize>>,
}

impl Program {
    #[must_use]
    pub fn new() -> Self {
        Program::default()
    }

    /// Run the declaration pass over a file and fold it into the graph.
    pub fn add_file(&mut self, path: &Path, root: &SyntaxNode, in_roots: bool) -> usize {
        self.add(path, decl::declare(root), in_roots)
    }

    /// Fold declarations already read — by another thread, or off the cache —
    /// into the graph.
    pub fn add(&mut self, path: &Path, mut decls: FileDecls, in_roots: bool) -> usize {
        if let Some(index) = self.by_path.get(path) {
            return *index;
        }
        let index = self.files.len();
        for symbol in &mut decls.subs {
            symbol.file = index;
        }
        for (_, name) in &decls.packages {
            self.packages.entry(name.clone()).or_default().push(index);
        }
        for symbol in &decls.subs {
            let key = (symbol.package.clone(), symbol.name.clone());
            if !self.by_name.contains_key(&key) {
                self.by_name.insert(key, self.subs.len());
                self.subs.push(symbol.clone());
            }
        }
        self.by_path.insert(path.to_path_buf(), index);
        self.files.push(FileEntry {
            path: path.to_path_buf(),
            decls,
            in_roots,
        });
        index
    }

    #[must_use]
    pub fn file(&self, index: usize) -> Option<&FileEntry> {
        self.files.get(index)
    }

    pub fn files(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.iter()
    }

    #[must_use]
    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.by_path.get(path).copied()
    }

    #[must_use]
    pub fn sub(&self, package: &str, name: &str) -> Option<&SubDecl> {
        self.by_name
            .get(&(package.to_string(), name.to_string()))
            .map(|index| &self.subs[*index])
    }

    /// Whether anything in the run declares this package.
    #[must_use]
    pub fn knows_package(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// What a bareword call in this file at this offset names.
    ///
    /// perl looks in the current package and then at what was imported, and so
    /// does this. A name neither answers is `Unknown` — never a diagnostic.
    #[must_use]
    pub fn resolve_call(&self, file: usize, offset: u32, name: &str) -> Option<&SubDecl> {
        // A qualified name says where to look.
        if let Some((package, bare)) = name.rsplit_once("::") {
            return self.sub(package, bare);
        }
        let entry = self.files.get(file)?;
        if let Some(symbol) = self.sub(entry.decls.package_at(offset), name) {
            return Some(symbol);
        }
        let from = entry.decls.imports.get(name)?;
        self.sub(from, name)
    }
}
