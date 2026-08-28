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

use crate::annotate::{Dialect, ListShape, Returns};
use crate::decl::{self, FileDecls, Params, SubDecl};
use crate::types::Type;

/// One file, as the graph holds it.
pub struct FileEntry {
    pub path: PathBuf,
    pub decls: FileDecls,
    /// Whether a diagnostic may be reported against it.
    pub in_roots: bool,
}

/// One method a class answers to, as [`Program::methods_of`] lists it.
#[derive(Debug, Clone)]
pub struct Method<'a> {
    pub name: String,
    /// The class that declares it, which is not the class it was asked of
    /// when it was inherited.
    pub class: String,
    /// Where that class sits in the linearisation: `0` is the class itself,
    /// so sorting by it puts a class's own methods before what it inherits.
    pub depth: usize,
    pub kind: MethodKind<'a>,
}

impl Method<'_> {
    /// The signature to show beside the name, or `None` where there is
    /// nothing to say.
    #[must_use]
    pub fn signature(&self) -> Option<String> {
        match &self.kind {
            MethodKind::Sub(symbol) => Some(crate::decl::signature_of(symbol)),
            MethodKind::Attribute(attribute) => Some(format!(
                "{} ({})",
                attribute.returns(&self.name),
                attribute.ty
            )),
            MethodKind::Constructor => Some("new(%args)".to_string()),
            MethodKind::Universal => None,
        }
    }
}

/// Where a listed method came from.
#[derive(Debug, Clone)]
pub enum MethodKind<'a> {
    Sub(&'a SubDecl),
    Attribute(&'a crate::annotate::AttributeDecl),
    /// The framework generates `new`.
    Constructor,
    /// `UNIVERSAL` gives it to every class.
    Universal,
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
    /// Every sub a package declares, in declaration order. The closed set is
    /// what completion needs and what asking `by_name` cannot give
    /// (`docs/lsp.md`, "The method surface"); scanning `subs` for it would be
    /// a walk of the whole workspace per keystroke.
    by_package: HashMap<String, Vec<usize>>,
    /// Which files declare a package, for method resolution.
    packages: HashMap<String, Vec<usize>>,
    /// What this project's own modules stand in for (`camello.toml`,
    /// `read-as`).
    dialect: Dialect,
}

impl Program {
    #[must_use]
    pub fn new() -> Self {
        Program::default()
    }

    /// Read the graph under a project's own dialect.
    pub fn set_dialect(&mut self, dialect: Dialect) {
        self.dialect = dialect;
    }

    #[must_use]
    pub fn dialect(&self) -> &Dialect {
        &self.dialect
    }

    /// Run the declaration pass over a file and fold it into the graph.
    pub fn add_file(&mut self, path: &Path, root: &SyntaxNode, in_roots: bool) -> usize {
        let decls = decl::declare_in(root, &self.dialect);
        self.add(path, decls, in_roots)
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
                self.by_package
                    .entry(symbol.package.clone())
                    .or_default()
                    .push(self.subs.len());
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

    /// Install a file's declarations over the ones the graph already holds,
    /// and rebuild what was indexed from them.
    ///
    /// An editor needs this and a batch run does not: `camello check` builds
    /// the graph once and asks it once, while an edit that changes a
    /// *declaration* changes what every other file can see
    /// (`docs/lsp.md`, "Incremental reanalysis", step 5). The rebuild is over
    /// the name indexes only — no file is reread and no body is walked —
    /// which is what makes relinking cheap relative to the body passes it
    /// invalidates.
    ///
    /// A path the graph does not hold is added instead, so the caller need
    /// not ask first.
    pub fn replace(&mut self, path: &Path, mut decls: FileDecls, in_roots: bool) -> usize {
        let Some(index) = self.by_path.get(path).copied() else {
            return self.add(path, decls, in_roots);
        };
        for symbol in &mut decls.subs {
            symbol.file = index;
        }
        self.files[index].decls = decls;
        self.files[index].in_roots = in_roots;
        self.reindex();
        index
    }

    /// Install a return the inference read off a body
    /// (`docs/return-inference.md`, "Tier 2").
    ///
    /// `index` is into the file's own `decls.subs`. Both copies are written:
    /// the file's declarations, which are the truth a [`Program::replace`]
    /// rebuilds from, and the flattened list the name indexes answer from.
    pub fn set_returns(&mut self, file: usize, index: usize, returns: Returns) {
        let Some(entry) = self.files.get_mut(file) else {
            return;
        };
        let Some(symbol) = entry.decls.subs.get_mut(index) else {
            return;
        };
        symbol.returns = returns.clone();
        let key = (symbol.package.clone(), symbol.name.clone());
        // The flattened copy is only this sub's where the first-wins rule
        // picked this file; where it picked another, the name means the other
        // file's sub and this is not it.
        if let Some(flat) = self.by_name.get(&key).copied() {
            if self.subs[flat].file == file {
                self.subs[flat].returns = returns;
            }
        }
    }

    /// Take one file's declarations back out of the graph.
    ///
    /// For the throwaway single-file graph tier 1 of return inference builds:
    /// the declarations go in so that the walk can resolve the file's own
    /// calls against them, and the same declarations come back out with the
    /// returns filled in.
    pub fn take_decls(&mut self, file: usize) -> FileDecls {
        self.files
            .get_mut(file)
            .map(|entry| std::mem::take(&mut entry.decls))
            .unwrap_or_default()
    }

    /// Every sub in a file that nothing is yet known about the return of.
    ///
    /// The subs a round of the fixpoint walks: an annotated sub is never
    /// inferred, and an inferred type is final once it is known, so what is
    /// left over is exactly this.
    #[must_use]
    pub fn unresolved_returns(&self, file: usize) -> Vec<usize> {
        self.files
            .get(file)
            .map(|entry| {
                entry
                    .decls
                    .subs
                    .iter()
                    .enumerate()
                    // A `new` whose body says the value is one of its own
                    // class is answered at the call site, where the receiver
                    // is known (INFER-2g): reading `InstanceOf[the package it
                    // was written in]` off the `bless` instead would tell
                    // every subclass it was the parent.
                    .filter(|(_, symbol)| {
                        symbol.returns.is_unresolved() && !symbol.constructs_own_class
                    })
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Rebuild the name indexes from the files, which are the truth.
    fn reindex(&mut self) {
        self.subs.clear();
        self.by_name.clear();
        self.by_package.clear();
        self.packages.clear();
        for (index, entry) in self.files.iter().enumerate() {
            for (_, name) in &entry.decls.packages {
                self.packages.entry(name.clone()).or_default().push(index);
            }
            for symbol in &entry.decls.subs {
                let key = (symbol.package.clone(), symbol.name.clone());
                if !self.by_name.contains_key(&key) {
                    self.by_name.insert(key, self.subs.len());
                    self.by_package
                        .entry(symbol.package.clone())
                        .or_default()
                        .push(self.subs.len());
                    self.subs.push(symbol.clone());
                }
            }
        }
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

    /// The sub `package::name` as *this file* declares it, or as the run does.
    ///
    /// [`Program::sub`] answers from one global name index, and its first-wins
    /// rule picks whichever file the walk reached first. That is the only
    /// answer available about a name a file merely *calls*, and it is the
    /// wrong one about a name the file in hand declares itself: a checkout
    /// left inside the tree — a vendored copy, an old release directory —
    /// holds the same `package Foo` with the same subs, sorts before `lib/`,
    /// and so wins the index. The declarations being read then belong to a
    /// file nobody is looking at, and the annotations in front of the reader
    /// count for nothing.
    ///
    /// So a question asked *from* a file is answered by that file first. Where
    /// it declares nothing of the name this falls back to the global index, so
    /// it differs from [`Program::sub`] only where a duplicate exists — which
    /// is where the global answer was a guess between two files, and this one
    /// is not a guess.
    #[must_use]
    pub fn sub_in(&self, file: usize, package: &str, name: &str) -> Option<&SubDecl> {
        self.files
            .get(file)
            .and_then(|entry| {
                entry
                    .decls
                    .subs
                    .iter()
                    // First within the file too, the way `by_name` reads a
                    // redefinition: either answer is a guess, and the two
                    // lookups agreeing is worth more than the choice.
                    .find(|symbol| symbol.package == package && symbol.name == name)
            })
            .or_else(|| self.sub(package, name))
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
        // XS registers its methods into the distribution's namespace, and a
        // distribution's namespace is a name prefix: `Net::DBus` calls
        // `XSLoader::load` and the methods land on
        // `Net::DBus::Binding::Iterator`, whose own file has no idea. So a
        // dynamic package makes everything below its name dynamic too.
        let mut prefix = package;
        while let Some((outer, _)) = prefix.rsplit_once("::") {
            if self.facts(outer).iter().any(|facts| facts.dynamic) {
                return true;
            }
            prefix = outer;
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
                // A package that exports its own subs is a mixin: what its
                // methods are called on is whichever package imported them,
                // not this one (METHOD-6b).
                if facts.exports_unknown || !facts.exports.is_empty() {
                    return true;
                }
                // A module whose `@EXPORT` this could not read puts names
                // here that nothing can enumerate (METHOD-6a), and a code
                // generator that assigns to globs writes its methods into the
                // package that *called* it (METHOD-5g).
                if facts.uses.iter().any(|module| {
                    self.facts(module)
                        .iter()
                        .any(|exporter| exporter.exports_unknown)
                }) {
                    return true;
                }
                if facts
                    .file_scope_calls
                    .iter()
                    .any(|(target, method)| self.is_generator(target, method))
                {
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

    /// Whether a method call is a call to a code generator: a sub declared in
    /// a file that assigns to globs (`docs/types.md`, METHOD-5g).
    ///
    /// `TAP::Object::mk_methods` and `Class::Accessor`'s family all write
    /// `*{"${class}::$name"} = sub {...}`, so the methods land in whichever
    /// package made the call and that package's own file never names one.
    /// Resolved through `linearise` rather than `resolve_method`, which would
    /// ask this question back.
    fn is_generator(&self, target: &str, method: &str) -> bool {
        for class in self.linearise(target) {
            if self.sub(&class, method).is_some() {
                return self.facts(&class).iter().any(|facts| facts.dynamic);
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
                    if attribute.answers_to(name) {
                        return MethodLookup::Attribute(attribute);
                    }
                }
                if name == "new" && facts.constructor {
                    return MethodLookup::Constructor;
                }
            }
        }
        // A name a module exports is a sub of this package: `use Exporter
        // 'import'` installs it here, so `$self->name` finds it
        // (`docs/types.md`, METHOD-6). Asked after the class's own
        // declarations, because that is the order perl would find them in and
        // it keeps the common lookup as short as it was.
        for class in self.linearise(package) {
            for facts in self.facts(&class) {
                for module in &facts.uses {
                    if !self
                        .facts(module)
                        .iter()
                        .any(|exporter| exporter.exports.iter().any(|export| export == name))
                    {
                        continue;
                    }
                    if let Some(symbol) = self.sub(module, name) {
                        return MethodLookup::Sub(symbol);
                    }
                }
            }
        }
        if self.has_unknown_ancestor(package) {
            MethodLookup::Unknown
        } else {
            MethodLookup::Missing
        }
    }

    /// Everything callable on a class, in MRO order (`docs/lsp.md`, "The
    /// method surface").
    ///
    /// [`Program::resolve_method_from`] answers "is this one there"; this
    /// answers "what is there", which is a different question and the one
    /// completion asks. Data, not diagnostics: nothing here decides whether a
    /// name *should* have been found, so an unknown ancestor is not this
    /// function's business — it means the list is a floor rather than the
    /// whole set, and the caller who cares is told by
    /// [`Program::has_unknown_ancestor`].
    ///
    /// A name is listed once, by the first class in the linearisation that
    /// declares it, which is the one perl would reach.
    #[must_use]
    pub fn methods_of(&self, package: &str) -> Vec<Method<'_>> {
        let mut methods: Vec<Method<'_>> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let order = self.linearise(package);
        for (depth, class) in order.iter().enumerate() {
            for index in self.by_package.get(class).into_iter().flatten() {
                let symbol = &self.subs[*index];
                if seen.insert(symbol.name.clone()) {
                    methods.push(Method {
                        name: symbol.name.clone(),
                        class: class.clone(),
                        depth,
                        kind: MethodKind::Sub(symbol),
                    });
                }
            }
            for facts in self.facts(class) {
                for attribute in &facts.attributes {
                    // The accessor answers to its own name, and to every name
                    // the framework generated beside it — the same set
                    // [`crate::annotate::AttributeDecl::answers_to`] tests one
                    // name against.
                    let names = std::iter::once(&attribute.name)
                        .chain(attribute.methods.iter().map(|method| &method.name));
                    for name in names {
                        if seen.insert(name.clone()) {
                            methods.push(Method {
                                name: name.clone(),
                                class: class.clone(),
                                depth,
                                kind: MethodKind::Attribute(attribute),
                            });
                        }
                    }
                }
                if facts.constructor && seen.insert("new".to_string()) {
                    methods.push(Method {
                        name: "new".to_string(),
                        class: class.clone(),
                        depth,
                        kind: MethodKind::Constructor,
                    });
                }
            }
        }
        // Last, and at a depth past every real class: `UNIVERSAL` is there,
        // and it is never what the user meant to be shown first.
        for name in Self::UNIVERSAL {
            if seen.insert((*name).to_string()) {
                methods.push(Method {
                    name: (*name).to_string(),
                    class: "UNIVERSAL".to_string(),
                    depth: order.len(),
                    kind: MethodKind::Universal,
                });
            }
        }
        methods
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

    /// Substitute what the run's type libraries declare into every annotation
    /// that named one (`docs/types.md`, ANNOT-8a).
    ///
    /// Once, after the last file is in: a library is as likely to be read
    /// after the file that uses it as before, so this cannot be folded into
    /// [`Program::add`]. Names are global rather than per-package, because a
    /// type library exists to be imported and the importing file writes the
    /// bare name.
    pub fn link_named_types(&mut self) {
        let declared: HashMap<String, Type> = {
            let mut collected: HashMap<String, Type> = HashMap::new();
            for named in self
                .files
                .iter()
                .flat_map(|entry| entry.decls.facts.iter())
                .flat_map(|facts| facts.types.iter())
            {
                // First wins, the way `sub` redefinition does.
                collected
                    .entry(named.name.clone())
                    .or_insert_with(|| named.ty.clone());
            }
            collected
        };
        if declared.is_empty() {
            return;
        }
        let lookup = |name: &str| declared.get(name).cloned();

        for entry in &mut self.files {
            for symbol in &mut entry.decls.subs {
                link_sub(symbol, &lookup);
            }
            for facts in &mut entry.decls.facts {
                for attribute in &mut facts.attributes {
                    attribute.ty = attribute.ty.substituted(&lookup);
                }
                for named in &mut facts.types {
                    named.ty = named.ty.substituted(&lookup);
                }
            }
            for annotated in &mut entry.decls.annotations {
                annotated.ty = annotated.ty.substituted(&lookup);
            }
        }
        // The flattened copy the name lookup answers from is a copy, so it is
        // linked too rather than rebuilt.
        for symbol in &mut self.subs {
            link_sub(symbol, &lookup);
        }
    }

    /// What a bareword call in this file at this offset names.
    ///
    /// perl looks in the current package and then at what was imported, and so
    /// does this. A name neither answers is `Unknown` — never a diagnostic.
    ///
    /// A builtin's name is looked up in the imports and nowhere else. `sub
    /// delete { ... }` beside `delete $h->{k}` does not make the second call
    /// the first: perl reaches `delete` before it reaches the package, and
    /// nothing a package writes for itself changes that. Importing the name is
    /// the one mechanism perlsub gives for overriding a builtin, so an import
    /// still answers — for the builtins that allow it, which perl decides and
    /// this does not.
    #[must_use]
    pub fn resolve_call(&self, file: usize, offset: u32, name: &str) -> Option<&SubDecl> {
        // A qualified name says where to look — and is never the builtin.
        if let Some((package, bare)) = name.rsplit_once("::") {
            return self.sub_in(file, package, bare);
        }
        let entry = self.files.get(file)?;
        if !camello_syntax::is_builtin(name) {
            if let Some(symbol) = self.sub_in(file, entry.decls.package_at(offset), name) {
                return Some(symbol);
            }
        }
        // An import names another package, and this file declares nothing of
        // it: the global index is the whole of the answer.
        let from = entry.decls.imports.get(name)?;
        self.sub(from, name)
    }
}

/// One sub's parameters and `Returns:`, with the named types substituted in.
fn link_sub(symbol: &mut SubDecl, lookup: &dyn Fn(&str) -> Option<Type>) {
    match &mut symbol.params {
        Params::Unknown => {}
        Params::Positional { params, .. } | Params::Named { params, .. } => {
            for param in params {
                param.ty = param.ty.substituted(lookup);
            }
        }
    }
    symbol.returns.scalar = symbol.returns.scalar.substituted(lookup);
    if let ListShape::Fixed(types) = &mut symbol.returns.list {
        for ty in types {
            *ty = ty.substituted(lookup);
        }
    }
}
