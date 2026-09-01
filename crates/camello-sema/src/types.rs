//! The type lattice and the type-expression parser (`docs/typecheck.md`,
//! "Types").
//!
//! The type language is the intersection of Moose's string constraints and
//! Types::Standard, because that is what the annotations are written in, and it
//! is deliberately not extended: a type the annotations cannot express is a
//! type the checker cannot be told to expect.
//!
//! [`Type::Unknown`] is the rule the whole design rests on. It is a top like
//! `Any`, but it means "not analysed" rather than "anything", it propagates
//! through every operation, and nothing is ever reported against it. That is
//! what keeps the checker quiet on code it was told nothing about.

use std::fmt;

/// A shape a value may have.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Type {
    /// Top. Everything is `Any`.
    Any,
    /// Top too, but "not analysed": never reported against, and every
    /// operation on it yields it.
    Unknown,
    Defined,
    Value,
    Str,
    Num,
    Int,
    /// `0`, `1`, `''`, `undef`. Kept nominal: `Bool` is not `Int`, so an
    /// `isa => 'Bool'` slot rejects `2` — which is what Moose does.
    Bool,
    /// A `Str` naming a package known to the program, and — where the name is
    /// known — which one: `ClassName['Foo']` is a `Str` holding `'Foo'` or the
    /// name of one of its subclasses, which is what a class method's `$class`
    /// invocant is (`docs/types.md`, INFER-9a).
    ClassName(Option<String>),
    /// A `Str` naming a role.
    RoleName,
    Enum(Vec<String>),
    /// A reference of an unsaid kind.
    Ref,
    ScalarRef(Box<Type>),
    ArrayRef(Box<Type>),
    Tuple(Vec<Type>),
    HashRef(Box<Type>),
    Dict {
        slots: Vec<(String, Type)>,
        /// `Dict[..., slurpy HashRef[T]]` — what any other key holds. A `Dict`
        /// without one is *restricted*: reading a key it has no slot for is a
        /// diagnostic.
        slurpy: Option<Box<Type>>,
    },
    Map(Box<Type>, Box<Type>),
    CodeRef,
    RegexpRef,
    GlobRef,
    FileHandle,
    Object,
    InstanceOf(String),
    ConsumerOf(String),
    HasMethods(Vec<String>),
    Undef,
    /// Only inside a `Dict` or a parameter list: the slot may be absent.
    Optional(Box<Type>),
    Union(Vec<Type>),
}

impl Type {
    /// `Maybe[T]`, which is `T | Undef` and is written that way here.
    #[must_use]
    pub fn maybe(inner: Type) -> Type {
        if inner == Type::Unknown {
            return Type::Unknown;
        }
        Type::union(vec![inner, Type::Undef])
    }

    /// A union, flattened and deduplicated, and collapsed when it holds one
    /// member or an `Unknown`.
    #[must_use]
    pub fn union(members: Vec<Type>) -> Type {
        let mut flat: Vec<Type> = Vec::new();
        for member in members {
            match member {
                Type::Unknown => return Type::Unknown,
                Type::Union(inner) => {
                    for one in inner {
                        if !flat.contains(&one) {
                            flat.push(one);
                        }
                    }
                }
                // Two enums are one enum: `'foo' | 'bar'` names two values
                // a single `Str` may hold, and keeping them apart would show
                // it as `Enum[foo]|Enum[bar]` and compare it a member at a
                // time for no gain.
                Type::Enum(values) => match flat.iter_mut().find_map(|member| match member {
                    Type::Enum(into) => Some(into),
                    _ => None,
                }) {
                    Some(into) => {
                        for value in values {
                            if !into.contains(&value) {
                                into.push(value);
                            }
                        }
                    }
                    None => flat.push(Type::Enum(values)),
                },
                one => {
                    if !flat.contains(&one) {
                        flat.push(one);
                    }
                }
            }
        }
        match flat.len() {
            0 => Type::Unknown,
            1 => flat.pop().expect("one member"),
            _ => Type::Union(flat),
        }
    }

    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    /// Whether `undef` is one of the things this may be.
    #[must_use]
    pub fn is_maybe(&self) -> bool {
        match self {
            Type::Undef => true,
            Type::Union(members) => members.iter().any(Type::is_maybe),
            _ => false,
        }
    }

    /// The same type with `undef` taken out of it, which is what a `defined`
    /// check leaves behind.
    #[must_use]
    pub fn without_undef(&self) -> Type {
        match self {
            Type::Undef => Type::Unknown,
            Type::Union(members) => Type::union(
                members
                    .iter()
                    .filter(|member| **member != Type::Undef)
                    .cloned()
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// The type inside an `Optional[T]`, or the type itself.
    #[must_use]
    pub fn required(&self) -> &Type {
        match self {
            Type::Optional(inner) => inner,
            other => other,
        }
    }

    #[must_use]
    pub fn is_optional(&self) -> bool {
        matches!(self, Type::Optional(_))
    }

    /// The same type with every name a project's own type library declares
    /// replaced by what it declared (`docs/types.md`, ANNOT-8a).
    ///
    /// A bareword in a type position reads as `InstanceOf[name]`, because a
    /// name nothing declares is a class name; this is what turns the ones
    /// something *does* declare back into the shape they stand for. A lookup
    /// that answers with the name itself — `class_type 'DateTime'` — is a
    /// class after all and is left alone, which is also what stops a cycle.
    #[must_use]
    pub fn substituted(&self, lookup: &dyn Fn(&str) -> Option<Type>) -> Type {
        self.substituted_within(lookup, SUBSTITUTION_DEPTH)
    }

    fn substituted_within(&self, lookup: &dyn Fn(&str) -> Option<Type>, fuel: u32) -> Type {
        let Some(fuel) = fuel.checked_sub(1) else {
            // `type A => as B; type B => as A;` is not a type, and the depth
            // is what says so rather than a stack overflow.
            return Type::Unknown;
        };
        let inside = |ty: &Type| Box::new(ty.substituted_within(lookup, fuel));
        let each = |types: &[Type]| -> Vec<Type> {
            types
                .iter()
                .map(|ty| ty.substituted_within(lookup, fuel))
                .collect()
        };
        match self {
            Type::InstanceOf(name) => match lookup(name) {
                Some(found) if found != *self => found.substituted_within(lookup, fuel),
                _ => self.clone(),
            },
            Type::ScalarRef(inner) => Type::ScalarRef(inside(inner)),
            Type::ArrayRef(inner) => Type::ArrayRef(inside(inner)),
            Type::HashRef(inner) => Type::HashRef(inside(inner)),
            Type::Optional(inner) => Type::Optional(inside(inner)),
            Type::Tuple(members) => Type::Tuple(each(members)),
            Type::Union(members) => Type::union(each(members)),
            Type::Map(key, value) => Type::Map(inside(key), inside(value)),
            Type::Dict { slots, slurpy } => Type::Dict {
                slots: slots
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.substituted_within(lookup, fuel)))
                    .collect(),
                slurpy: slurpy.as_ref().map(|ty| inside(ty)),
            },
            other => other.clone(),
        }
    }
}

/// How far a named type may stand for another before the chain is called a
/// cycle. Type libraries nest a few deep; nothing legitimate goes further.
const SUBSTITUTION_DEPTH: u32 = 16;

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Any => f.write_str("Any"),
            Type::Unknown => f.write_str("Unknown"),
            Type::Defined => f.write_str("Defined"),
            Type::Value => f.write_str("Value"),
            Type::Str => f.write_str("Str"),
            Type::Num => f.write_str("Num"),
            Type::Int => f.write_str("Int"),
            Type::Bool => f.write_str("Bool"),
            Type::ClassName(None) => f.write_str("ClassName"),
            Type::ClassName(Some(class)) => write!(f, "ClassName['{class}']"),
            Type::RoleName => f.write_str("RoleName"),
            Type::Enum(values) => write!(f, "Enum[{}]", values.join(", ")),
            Type::Ref => f.write_str("Ref"),
            Type::ScalarRef(inner) => write!(f, "ScalarRef[{inner}]"),
            Type::ArrayRef(inner) => write!(f, "ArrayRef[{inner}]"),
            Type::Tuple(members) => write!(f, "Tuple[{}]", join(members)),
            Type::HashRef(inner) => write!(f, "HashRef[{inner}]"),
            Type::Dict { slots, slurpy } => {
                let mut parts: Vec<String> = slots
                    .iter()
                    .map(|(key, value)| format!("{key} => {value}"))
                    .collect();
                if let Some(rest) = slurpy {
                    parts.push(format!("slurpy {rest}"));
                }
                write!(f, "Dict[{}]", parts.join(", "))
            }
            Type::Map(key, value) => write!(f, "Map[{key}, {value}]"),
            Type::CodeRef => f.write_str("CodeRef"),
            Type::RegexpRef => f.write_str("RegexpRef"),
            Type::GlobRef => f.write_str("GlobRef"),
            Type::FileHandle => f.write_str("FileHandle"),
            Type::Object => f.write_str("Object"),
            Type::InstanceOf(name) => write!(f, "InstanceOf['{name}']"),
            Type::ConsumerOf(name) => write!(f, "ConsumerOf['{name}']"),
            Type::HasMethods(names) => write!(f, "HasMethods[{}]", names.join(", ")),
            Type::Undef => f.write_str("Undef"),
            Type::Optional(inner) => write!(f, "Optional[{inner}]"),
            Type::Union(members) => {
                let text: Vec<String> = members.iter().map(ToString::to_string).collect();
                f.write_str(&text.join("|"))
            }
        }
    }
}

fn join(types: &[Type]) -> String {
    types
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

// ===== The type-expression parser =====

/// Why an annotation could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

/// What a name that is not a known constructor should be read as.
///
/// The Moose reading: an unrecognised *bareword* in a type position is a class
/// name (`docs/typecheck.md`, "Open questions"). It makes a typo in a type
/// name into an `InstanceOf` of a class nothing declares — resolvable to
/// nothing, hence `Unknown`, hence silent — which is the price of not
/// reporting every class from an unresolved dependency. A quoted string is the
/// other reading, and [`Parser::primary`] has it.
fn constructor(name: &str, arguments: Vec<Arg>) -> Result<Type, ParseError> {
    let arity = arguments.len();
    let plain = || -> Result<Vec<Type>, ParseError> {
        arguments
            .iter()
            .map(|argument| match argument {
                Arg::Plain(ty) => Ok(ty.clone()),
                Arg::Named(key, _) => Err(ParseError {
                    message: format!("`{name}` does not take a named parameter `{key}`"),
                }),
                Arg::Slurpy(_) => Err(ParseError {
                    message: format!("`{name}` does not take a `slurpy`"),
                }),
            })
            .collect()
    };

    // `Dict` is the one constructor whose arguments are named, so it is built
    // before the arm that refuses a named argument to everything else.
    if name == "Dict" && arity > 0 {
        return Ok(dict(arguments));
    }

    // A structured constructor written without parameters constrains nothing
    // beyond the kind of reference it is: bare `Dict` accepts any hash, bare
    // `Tuple` any array (`docs/types.md`, TYPE-4b). Reading bare `Dict` as the
    // *empty* structure instead would make every key an `unknown-key`.
    if arity == 0 {
        match name {
            "Dict" | "Map" => return Ok(Type::HashRef(Box::new(Type::Unknown))),
            "Tuple" => return Ok(Type::ArrayRef(Box::new(Type::Unknown))),
            _ => {}
        }
    }

    let ty = match name {
        // No parameters.
        "Any" | "Item" => Type::Any,
        "Defined" => Type::Defined,
        "Value" => Type::Value,
        "Bool" => Type::Bool,
        "RoleName" => Type::RoleName,
        "Undef" => Type::Undef,
        "CodeRef" | "CodeLike" => Type::CodeRef,
        "RegexpRef" => Type::RegexpRef,
        "GlobRef" => Type::GlobRef,
        "FileHandle" => Type::FileHandle,
        "Object" => Type::Object,

        // `Types::Common::String` and `Types::Common::Numeric` read as their
        // base type: the refinement is a run-time predicate, and the
        // structural part is what the checker can use.
        "Str" | "SimpleStr" | "NonEmptyStr" | "NonEmptySimpleStr" | "LowerCaseStr"
        | "UpperCaseStr" | "LowerCaseSimpleStr" | "UpperCaseSimpleStr" | "Password"
        | "StrongPassword" | "StrMatch" => Type::Str,
        "Num" | "LaxNum" | "StrictNum" | "PositiveNum" | "PositiveOrZeroNum" | "NegativeNum"
        | "NegativeOrZeroNum" | "NumRange" => Type::Num,
        "Int" | "PositiveInt" | "PositiveOrZeroInt" | "NegativeInt" | "NegativeOrZeroInt"
        | "SingleDigit" | "IntRange" => Type::Int,

        // `Ref['ARRAY']` says which kind, and the lattice has no place to put
        // that, so every `Ref` is the same `Ref`.
        "Ref" => Type::Ref,
        "ScalarRef" => Type::ScalarRef(Box::new(one(plain()?))),
        "ArrayRef" | "ArrayLike" => Type::ArrayRef(Box::new(one(plain()?))),
        "HashRef" | "HashLike" => Type::HashRef(Box::new(one(plain()?))),
        "Tuple" => Type::Tuple(plain()?),
        "Map" => {
            let members = plain()?;
            if members.len() != 2 {
                return Err(ParseError {
                    message: "`Map` takes a key type and a value type".to_string(),
                });
            }
            Type::Map(Box::new(members[0].clone()), Box::new(members[1].clone()))
        }
        "Maybe" => Type::maybe(one(plain()?)),
        "Optional" => Type::Optional(Box::new(one(plain()?))),
        "Enum" => Type::Enum(names(&arguments)),
        // Bare, it is any class's name; parameterised, that class or one
        // below it — the `type[Foo]` of a language that has one.
        "ClassName" => Type::ClassName(names(&arguments).into_iter().next()),
        "InstanceOf" => match names(&arguments).into_iter().next() {
            Some(class) => Type::InstanceOf(class),
            None => Type::Object,
        },
        "ConsumerOf" => match names(&arguments).into_iter().next() {
            Some(role) => Type::ConsumerOf(role),
            None => Type::Object,
        },
        "HasMethods" | "Overload" => Type::HasMethods(names(&arguments)),
        // `Slurpy[HashRef[Str]]` in a Dict position; elsewhere it is what it
        // wraps.
        "Slurpy" => one(plain()?),

        // Str-as-class. `Foo::Bar` is a class name; a bareword with arguments
        // is a constructor nobody here knows, and that is `Unknown` rather
        // than an error, because a project's own library may define it.
        _ if arity == 0 => Type::InstanceOf(name.to_string()),
        _ => Type::Unknown,
    };
    Ok(ty)
}

/// The single parameter of a one-parameter constructor, `Any` when absent.
fn one(mut types: Vec<Type>) -> Type {
    if types.is_empty() {
        Type::Any
    } else {
        types.remove(0)
    }
}

/// The names an `Enum` / `InstanceOf` / `HasMethods` was given.
///
/// A name is a name whichever way it was written: `InstanceOf['Foo']` quotes
/// it the way Type::Tiny does and `InstanceOf[Foo]` does not, so the quoted
/// string that arrives here as the value it holds is read back as the name.
fn names(arguments: &[Arg]) -> Vec<String> {
    arguments
        .iter()
        .flat_map(|argument| match argument {
            Arg::Plain(Type::InstanceOf(name)) => vec![name.clone()],
            Arg::Plain(Type::Enum(values)) => values.clone(),
            Arg::Plain(other) => vec![other.to_string()],
            _ => Vec::new(),
        })
        .collect()
}

/// One argument inside `[...]`.
#[derive(Debug, Clone)]
enum Arg {
    Plain(Type),
    Named(String, Type),
    Slurpy(Type),
}

/// Read a type expression.
///
/// One grammar for both syntaxes. The design document has the bareword form
/// walked as a CST subtree and the string form parsed from text; they are the
/// same grammar written the same way — `ArrayRef[HashRef[Str]]` is one string
/// either way — and a declaration keeps the source text rather than the
/// subtree (see `decl::Annotation`), so there is one parser.
///
/// # Errors
///
/// When the text is not a type expression at all.
pub fn parse(text: &str) -> Result<Type, ParseError> {
    let mut parser = Parser {
        tokens: lex(text)?,
        index: 0,
    };
    let ty = parser.union()?;
    if parser.peek().is_some() {
        return Err(ParseError {
            message: format!("unexpected `{}`", parser.rest()),
        });
    }
    Ok(ty)
}

/// Whether text is shaped like a type expression at all.
///
/// The point is to tell an annotation that was *meant* as a type and is wrong
/// from prose that happens to sit where one could go. `File::Temp` writes
///
/// ```text
/// # Returns:    modified template
/// ```
///
/// which is a sentence, not a broken annotation, and reporting it would be the
/// checker claiming a comment for a syntax the file predates. The test is two
/// bare names side by side outside any bracket: no type expression has that,
/// and English has little else.
#[must_use]
pub fn is_type_shaped(text: &str) -> bool {
    let Ok(tokens) = lex(text) else {
        return false;
    };
    if tokens.is_empty() {
        return false;
    }
    if !matches!(
        tokens[0],
        Token::Name(_) | Token::Text(_) | Token::OpenParen
    ) {
        return false;
    }
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Open | Token::OpenParen => depth += 1,
            Token::Close | Token::CloseParen => depth = depth.saturating_sub(1),
            Token::Name(_) | Token::Text(_)
                if depth == 0
                    && index > 0
                    && matches!(tokens[index - 1], Token::Name(_) | Token::Text(_)) =>
            {
                return false;
            }
            _ => {}
        }
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Name(String),
    Text(String),
    Open,
    Close,
    Comma,
    FatComma,
    Pipe,
    OpenParen,
    CloseParen,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Name(name) => f.write_str(name),
            Token::Text(text) => write!(f, "'{text}'"),
            Token::Open => f.write_str("["),
            Token::Close => f.write_str("]"),
            Token::Comma => f.write_str(","),
            Token::FatComma => f.write_str("=>"),
            Token::Pipe => f.write_str("|"),
            Token::OpenParen => f.write_str("("),
            Token::CloseParen => f.write_str(")"),
        }
    }
}

fn lex(text: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' => index += 1,
            b'[' => {
                tokens.push(Token::Open);
                index += 1;
            }
            b']' => {
                tokens.push(Token::Close);
                index += 1;
            }
            b'(' => {
                tokens.push(Token::OpenParen);
                index += 1;
            }
            b')' => {
                tokens.push(Token::CloseParen);
                index += 1;
            }
            b',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            b'|' => {
                tokens.push(Token::Pipe);
                index += 1;
            }
            b'=' if bytes.get(index + 1) == Some(&b'>') => {
                tokens.push(Token::FatComma);
                index += 2;
            }
            b'\'' | b'"' => {
                let quote = byte;
                let start = index + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != quote {
                    end += if bytes[end] == b'\\' { 2 } else { 1 };
                }
                if end >= bytes.len() {
                    return Err(ParseError {
                        message: "unterminated string in a type expression".to_string(),
                    });
                }
                tokens.push(Token::Text(text[start..end.min(bytes.len())].to_string()));
                index = end + 1;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                // `Foo::Bar` is one name.
                while bytes.get(index) == Some(&b':') && bytes.get(index + 1) == Some(&b':') {
                    index += 2;
                    while index < bytes.len()
                        && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                    {
                        index += 1;
                    }
                }
                tokens.push(Token::Name(text[start..index].to_string()));
            }
            byte if byte.is_ascii_digit() => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_digit()
                        || bytes[index] == b'.'
                        || bytes[index] == b'-')
                {
                    index += 1;
                }
                tokens.push(Token::Text(text[start..index].to_string()));
            }
            _ => {
                return Err(ParseError {
                    message: format!(
                        "`{}` is not part of a type expression",
                        text[index..].chars().next().unwrap_or('?')
                    ),
                })
            }
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned();
        self.index += 1;
        token
    }

    fn eat(&mut self, token: &Token) -> bool {
        if self.peek() == Some(token) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn rest(&self) -> String {
        self.tokens[self.index..]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn union(&mut self) -> Result<Type, ParseError> {
        let mut members = vec![self.primary()?];
        while self.eat(&Token::Pipe) {
            members.push(self.primary()?);
        }
        Ok(Type::union(members))
    }

    fn primary(&mut self) -> Result<Type, ParseError> {
        match self.bump() {
            Some(Token::OpenParen) => {
                let inner = self.union()?;
                if !self.eat(&Token::CloseParen) {
                    return Err(ParseError {
                        message: "a `(` in a type expression is not closed".to_string(),
                    });
                }
                Ok(inner)
            }
            // Quotes are a *value*: `Returns: 'foo' | 'bar'` is the two
            // strings and not two classes (`docs/types.md`, TYPE-3a). A
            // bareword is what names a type here — `Returns: DateTime` is
            // still an instance of one — so the two readings have a spelling
            // each, and the quoted one is the one Moose has no use for: an
            // `isa => 'Foo'` reaches this parser with its quotes already off.
            // Inside `InstanceOf[...]` and its neighbours the quotes are how
            // Type::Tiny writes a name, which is what `names` reads back.
            Some(Token::Text(text)) => Ok(Type::Enum(vec![text])),
            Some(Token::Name(name)) => {
                let arguments = if self.eat(&Token::Open) {
                    let arguments = self.arguments()?;
                    if !self.eat(&Token::Close) {
                        return Err(ParseError {
                            message: format!("`{name}[` is not closed"),
                        });
                    }
                    arguments
                } else {
                    Vec::new()
                };
                constructor(&name, arguments)
            }
            Some(other) => Err(ParseError {
                message: format!("a type expression cannot begin with `{other}`"),
            }),
            None => Err(ParseError {
                message: "an empty type expression".to_string(),
            }),
        }
    }

    fn arguments(&mut self) -> Result<Vec<Arg>, ParseError> {
        let mut acc = Vec::new();
        if self.peek() == Some(&Token::Close) {
            return Ok(acc);
        }
        loop {
            acc.push(self.argument()?);
            if !self.eat(&Token::Comma) {
                break;
            }
            // A trailing comma is allowed, the way perl allows one.
            if self.peek() == Some(&Token::Close) {
                break;
            }
        }
        Ok(acc)
    }

    fn argument(&mut self) -> Result<Arg, ParseError> {
        // `slurpy HashRef[Str]` — a bareword followed by a type, with no comma.
        if self.peek() == Some(&Token::Name("slurpy".to_string())) {
            self.index += 1;
            return Ok(Arg::Slurpy(self.union()?));
        }
        // `name => Str` inside a `Dict`.
        let key = match (self.peek().cloned(), self.tokens.get(self.index + 1)) {
            (Some(Token::Name(name)), Some(Token::FatComma)) => Some(name),
            (Some(Token::Text(text)), Some(Token::FatComma)) => Some(text),
            _ => None,
        };
        if let Some(key) = key {
            self.index += 2;
            return Ok(Arg::Named(key, self.union()?));
        }
        Ok(Arg::Plain(self.union()?))
    }
}

/// `Dict[...]` needs its named arguments, which [`constructor`] refuses for
/// every other name; it is built here instead.
fn dict(arguments: Vec<Arg>) -> Type {
    let mut slots = Vec::new();
    let mut slurpy = None;
    for argument in arguments {
        match argument {
            Arg::Named(key, value) => slots.push((key, value)),
            Arg::Slurpy(rest) => slurpy = Some(Box::new(rest)),
            Arg::Plain(_) => {}
        }
    }
    Type::Dict { slots, slurpy }
}

#[cfg(test)]
mod tests;
