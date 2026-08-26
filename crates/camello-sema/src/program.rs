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

use std::collections::{HashMap, HashSet};
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

/// What a method name resolved to on a class.
#[derive(Debug, Clone)]
pub enum MethodLookup<'a> {
    Sub(&'a SubDecl),
    Attribute(&'a crate::annotate::AttributeDecl),
    /// The framework generates `new`.
    Constructor,
    /// `UNIVERSAL` gives it to every class.
    Universal,
    /// Nothing declares it, and every ancestor is known — so it really is not
    /// there.
    Missing,
    /// An ancestor is a package the run never saw, or the class answers
    /// through `AUTOLOAD` or an opaque delegation. Never reported against.
    Unknown,
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

    /// What the run knows about a package, from every file that opens it.
    ///
    /// A package's declarations are the union: it is common for one file to
    /// hold several packages and rare, but legal, for one package to span
    /// files (`docs/typecheck.md`, "Files, packages, symbols").
    #[must_use]
    pub fn facts(&self, package: &str) -> Vec<&crate::decl::PackageFacts> {
        self.packages
            .get(package)
            .into_iter()
            .flatten()
            .filter_map(|index| self.files[*index].decls.facts_for(package))
            .collect()
    }

    /// Every attribute a package declares, its ancestors' included.
    #[must_use]
    pub fn attributes(&self, package: &str) -> Vec<&crate::annotate::AttributeDecl> {
        self.linearise(package)
            .iter()
            .flat_map(|class| self.facts(class))
            .flat_map(|facts| facts.attributes.iter())
            .collect()
    }

    /// Whether the class, or anything it inherits from, is a package the run
    /// never saw.
    ///
    /// This is the question that decides whether "no such method" may be said
    /// at all: a class with an unknown ancestor might have any method, and
    /// reporting one missing there would be the worst kind of false positive.
    #[must_use]
    pub fn has_unknown_ancestor(&self, package: &str) -> bool {
        if !self.knows_package(package) {
            return true;
        }
        for class in self.linearise(package) {
            if !self.knows_package(&class) {
                return true;
            }
            // `AUTOLOAD` answers anything.
            if self.sub(&class, "AUTOLOAD").is_some() {
                return true;
            }
            let mut declares_anything = false;
            for facts in self.facts(&class) {
                if facts.dynamic {
                    return true;
                }
                // A `handles` naming a regexp or a role delegates a set nobody
                // here can enumerate.
                if facts
                    .attributes
                    .iter()
                    .any(|attribute| attribute.opaque_delegation)
                {
                    return true;
                }
                if facts.roles.iter().any(|role| !self.knows_package(role)) {
                    return true;
                }
                declares_anything |= !facts.attributes.is_empty() || facts.constructor;
            }
            // A package the run has seen the *name* of and nothing else — an
            // XS module whose `.pm` only loads a shared library — cannot be
            // said to be missing anything.
            declares_anything |= self.subs.iter().any(|symbol| symbol.package == class);
            if !declares_anything {
                return true;
            }
        }
        false
    }

    /// The class and everything above it, depth first, each named once.
    ///
    /// Depth first over `isa` with roles folded in is C3 for every hierarchy
    /// the checker can actually see; where the two would differ, the question
    /// is which of two ancestors defines a method, and both define it.
    #[must_use]
    pub fn linearise(&self, package: &str) -> Vec<String> {
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![package.to_string()];
        while let Some(class) = stack.pop() {
            if !seen.insert(class.clone()) {
                continue;
            }
            let parents: Vec<String> = self
                .facts(&class)
                .iter()
                .flat_map(|facts| facts.isa.iter().chain(facts.roles.iter()).cloned())
                .collect();
            order.push(class);
            // Reversed, so the first parent written is the first visited.
            for parent in parents.into_iter().rev() {
                stack.push(parent);
            }
        }
        order
    }

    /// Whether `descendant` is `ancestor`, or has it above it.
    #[must_use]
    pub fn isa(&self, descendant: &str, ancestor: &str) -> bool {
        self.linearise(descendant)
            .iter()
            .any(|class| class == ancestor)
    }

    /// The methods `UNIVERSAL` gives every class, and the two `Exporter`
    /// gives every module. None of them is declared anywhere, and all of them
    /// are there.
    const UNIVERSAL: &'static [&'static str] = &[
        "isa", "can", "DOES", "VERSION", "import", "unimport", "DESTROY",
    ];

    /// What `$obj->name` resolves to on a class.
    ///
    /// `from` is the package the call is *written* in, which is what `SUPER::`
    /// is relative to: `$self->SUPER::init(...)` looks in the parents of the
    /// package holding the line, not in the parents of whatever `$self` turned
    /// out to be.
    #[must_use]
    pub fn resolve_method_from(&self, package: &str, name: &str, from: &str) -> MethodLookup<'_> {
        if let Some(bare) = name.strip_prefix("SUPER::") {
            let parents: Vec<String> = self
                .facts(from)
                .iter()
                .flat_map(|facts| facts.isa.iter().cloned())
                .collect();
            if parents.is_empty() {
                return MethodLookup::Unknown;
            }
            for parent in parents {
                match self.resolve_method(&parent, bare) {
                    MethodLookup::Missing => {}
                    found => return found,
                }
            }
            return MethodLookup::Missing;
        }
        // `$self->Other::method(...)` says where to look.
        if let Some((class, bare)) = name.rsplit_once("::") {
            return self.resolve_method(class, bare);
        }
        self.resolve_method(package, name)
    }

    /// What `$obj->name` resolves to on a class.
    #[must_use]
    pub fn resolve_method(&self, package: &str, name: &str) -> MethodLookup<'_> {
        if Self::UNIVERSAL.contains(&name) {
            return MethodLookup::Universal;
        }
        for class in self.linearise(package) {
            if let Some(symbol) = self.sub(&class, name) {
                return MethodLookup::Sub(symbol);
            }
            for facts in self.facts(&class) {
                for attribute in &facts.attributes {
                    if attribute.name == name || attribute.methods.iter().any(|one| one == name) {
                        return MethodLookup::Attribute(attribute);
                    }
                }
                if name == "new" && facts.constructor {
                    return MethodLookup::Constructor;
                }
            }
        }
        if self.has_unknown_ancestor(package) {
            MethodLookup::Unknown
        } else {
            MethodLookup::Missing
        }
    }

    /// A named type a project's own `Type::Library` declares.
    #[must_use]
    pub fn named_type(&self, name: &str) -> Option<&crate::types::Type> {
        self.files
            .iter()
            .flat_map(|entry| entry.decls.facts.iter())
            .flat_map(|facts| facts.types.iter())
            .find(|named| named.name == name)
            .map(|named| &named.ty)
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
