//! The interpolation scanner (`docs/typecheck.md`, "Scopes").
//!
//! `"hi $who"` and `"$h->{k}[0]"` contain variable uses, and the lexer gives
//! the whole string as one token (the lexer contract's atomicity guarantee).
//! So the string is re-scanned here, in `sema`, and the CST is not changed:
//! this produces uses, not nodes.
//!
//! Getting it wrong means either a phantom "unused variable" or a missed
//! "undeclared variable", so the rules followed are perl's own (`perldoc
//! perlop`, "Gory details of parsing quoted constructs") rather than a regular
//! expression that looks close enough:
//!
//! - a backslash escapes the next character, whatever it is;
//! - `$` and `@` interpolate only when what follows could name a variable — an
//!   identifier, a `{`, or another sigil. `$` before `)` or at the end of a
//!   pattern is an anchor, and `foo@example.com` holds no array;
//! - `$h{k}` and `$a[0]` are elements of `%h` and `@a`, so the *use* carries
//!   the container's sigil, not the element's. `$h->{k}` is a use of `$h`;
//! - `${ ... }` and `@{ ... }` holding a bare name are that variable, and
//!   holding anything else are a block, whose contents are scanned in turn.
//!   `@{[ ... ]}` is the common case of the second;
//! - a method call is not interpolated: `"$obj->name"` uses `$obj` and then
//!   says `->name`.

use camello_syntax::ast::Sigil;

/// One variable use found inside a quoted construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub sigil: Sigil,
    pub name: String,
    /// Byte offset of the sigil, relative to the text that was scanned.
    pub offset: usize,
    /// Byte length of the whole reference, sigil and name.
    pub len: usize,
}

/// Every variable use in an interpolating construct.
#[must_use]
pub fn scan(text: &str) -> Vec<Use> {
    let mut uses = Vec::new();
    scan_into(text, 0, &mut uses);
    uses
}

/// The same, for a pattern written under `/x`.
///
/// Under `/x` a `#` outside a character class starts a comment that runs to
/// the end of the line, and nothing in it interpolates. `Text::ParseWords`
/// labels each branch of its pattern `# $quote`, `# $quoted`, and reading
/// those as uses reported six undeclared variables that are not there.
///
/// The comment is blanked rather than removed so that every offset still
/// points where it did.
#[must_use]
pub fn scan_extended(text: &str) -> Vec<Use> {
    let mut blanked = text.to_string();
    let bytes = unsafe {
        // Safe: every byte written is a space, and the bytes overwritten are
        // whole characters — the loop walks `char_indices`.
        blanked.as_bytes_mut()
    };
    let mut in_class = false;
    let mut escape = false;
    let mut comment = false;
    for (offset, ch) in text.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if comment {
            if ch == '\n' {
                comment = false;
            } else {
                for byte in &mut bytes[offset..offset + ch.len_utf8()] {
                    *byte = b' ';
                }
            }
            continue;
        }
        match ch {
            '\\' => escape = true,
            '[' => in_class = true,
            ']' => in_class = false,
            '#' if !in_class => {
                comment = true;
                bytes[offset] = b' ';
            }
            _ => {}
        }
    }
    scan(&blanked)
}

fn scan_into(text: &str, base: usize, uses: &mut Vec<Use>) {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'$' | b'@' => {
                let consumed = reference(text, index, base, false, uses);
                index += consumed.max(1);
            }
            _ => index += 1,
        }
    }
}

/// Read one `$...` or `@...` reference at `index`, returning what it consumed.
///
/// Zero means "this sigil begins nothing", and the caller steps over it.
fn reference(
    text: &str,
    index: usize,
    base: usize,
    dereferenced: bool,
    uses: &mut Vec<Use>,
) -> usize {
    let bytes = text.as_bytes();
    let sigil_byte = bytes[index];
    let mut cursor = index + 1;

    // `$#array` is the last index of `@array` — an array use.
    let mut sigil = if sigil_byte == b'$' {
        Sigil::Scalar
    } else {
        Sigil::Array
    };
    if sigil_byte == b'$' && bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
        sigil = Sigil::Array;
    }

    match bytes.get(cursor) {
        // `$$ref` / `@$ref`: the inner reference is the use; the outer sigil
        // only says what it is dereferenced as.
        Some(b'$') => {
            // `$$argv[0]` is an element of `@{$argv}`, so the subscript
            // belongs to this dereference and `$argv` stays a scalar.
            let consumed = reference(text, cursor, base, true, uses);
            if consumed == 0 {
                return 0;
            }
            let after = index + 1 + consumed;
            subscripts(text, after, base, uses) - index
        }
        // `${name}` is `$name`; `${ anything else }` is a block.
        Some(b'{') => {
            let Some(close) = matching_brace(bytes, cursor) else {
                return 0;
            };
            let inner = &text[cursor + 1..close];
            if let Some(name) = plain_name(inner.trim()) {
                let end = subscripts(text, close + 1, base, uses);
                let sigil = if dereferenced {
                    sigil
                } else {
                    container_sigil(sigil, text, close + 1)
                };
                uses.push(Use {
                    sigil,
                    name,
                    offset: base + index,
                    len: close + 1 - index,
                });
                end - index
            } else {
                scan_into(inner, base + cursor + 1, uses);
                close + 1 - index
            }
        }
        Some(byte) if is_name_start(*byte) => {
            let start = cursor;
            cursor = name_end(bytes, cursor);
            let name = text[start..cursor].to_string();
            let sigil = if dereferenced {
                sigil
            } else {
                container_sigil(sigil, text, cursor)
            };
            let end = if dereferenced {
                cursor
            } else {
                subscripts(text, cursor, base, uses)
            };
            uses.push(Use {
                sigil,
                name,
                offset: base + index,
                len: cursor - index,
            });
            end - index
        }
        // A punctuation variable (`$@`, `$!`, `$1`) or nothing at all. Neither
        // is a name a `my` could have declared, so neither is a use worth
        // recording — and `$` before `)` is an anchor, not a variable.
        _ => 0,
    }
}

/// What the variable is an element *of*, given what follows its name.
///
/// `$h{k}` reads `%h` and `$a[0]` reads `@a`; only an arrow keeps the scalar.
fn container_sigil(sigil: Sigil, text: &str, after_name: usize) -> Sigil {
    if sigil == Sigil::Code || sigil == Sigil::Typeglob {
        return sigil;
    }
    match text.as_bytes().get(after_name) {
        Some(b'{') => Sigil::Hash,
        Some(b'[') => Sigil::Array,
        _ => sigil,
    }
}

/// Walk the `{...}` / `[...]` / `->{...}` chain after a name, scanning inside
/// each for the uses a key expression holds, and return where it ends.
fn subscripts(text: &str, mut index: usize, base: usize, uses: &mut Vec<Use>) -> usize {
    let bytes = text.as_bytes();
    loop {
        let mut cursor = index;
        // An arrow only continues the chain when a subscript follows it: a
        // method call is not interpolated.
        if bytes.get(cursor) == Some(&b'-') && bytes.get(cursor + 1) == Some(&b'>') {
            cursor += 2;
        }
        match bytes.get(cursor) {
            Some(b'{') => {
                let Some(close) = matching_brace(bytes, cursor) else {
                    return index;
                };
                scan_into(&text[cursor + 1..close], base + cursor + 1, uses);
                index = close + 1;
            }
            Some(b'[') => {
                let Some(close) = matching_bracket(bytes, cursor) else {
                    return index;
                };
                scan_into(&text[cursor + 1..close], base + cursor + 1, uses);
                index = close + 1;
            }
            _ => return index,
        }
    }
}

fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    matching(bytes, open, b'{', b'}')
}

fn matching_bracket(bytes: &[u8], open: usize) -> Option<usize> {
    matching(bytes, open, b'[', b']')
}

fn matching(bytes: &[u8], open: usize, opener: u8, closer: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            byte if byte == opener => depth += 1,
            byte if byte == closer => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// A bare `name` or `Foo::name`, and nothing else.
fn plain_name(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !is_name_start(bytes[0]) {
        return None;
    }
    (name_end(bytes, 0) == bytes.len()).then(|| text.to_string())
}

/// Where a name starting at `index` ends.
///
/// A single colon is not part of a name: `"$filename: not found"` interpolates
/// `$filename` and then says `: not found`, and reading the colon as part of
/// the name reported `$filename:` undeclared 21 times over @INC. Only `::`
/// continues one, and only when a name follows it.
fn name_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        if is_name_continue(bytes[index]) {
            index += 1;
        } else if bytes[index] == b':'
            && bytes.get(index + 1) == Some(&b':')
            && bytes
                .get(index + 2)
                .is_some_and(|byte| is_name_start(*byte))
        {
            index += 2;
        } else {
            break;
        }
    }
    index
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        scan(text)
            .into_iter()
            .map(|item| format!("{}{}", item.sigil.as_str(), item.name))
            .collect()
    }

    #[test]
    fn a_plain_scalar_is_a_use() {
        assert_eq!(names("hi $who"), vec!["$who"]);
    }

    #[test]
    fn an_escaped_sigil_is_not() {
        assert_eq!(names("hi \\$who"), Vec::<String>::new());
        assert_eq!(names("a\\@b $c"), vec!["$c"]);
    }

    #[test]
    fn an_element_names_its_container() {
        // `$h{k}` reads `%h`, not `$h`; getting this backwards is a phantom
        // undeclared-variable on every hash in the corpus.
        assert_eq!(names("$h{k}"), vec!["%h"]);
        assert_eq!(names("$a[0]"), vec!["@a"]);
        assert_eq!(names("$x->{k}"), vec!["$x"]);
        assert_eq!(names("$x->[0]"), vec!["$x"]);
    }

    #[test]
    fn a_key_expression_is_scanned_too() {
        assert_eq!(names("$h{$key}"), vec!["$key", "%h"]);
    }

    #[test]
    fn a_chain_is_walked_to_its_end() {
        assert_eq!(names("$x->{a}[0]{$b}"), vec!["$b", "$x"]);
    }

    #[test]
    fn a_braced_name_is_that_variable() {
        assert_eq!(names("${who}"), vec!["$who"]);
        assert_eq!(names("@{list}"), vec!["@list"]);
    }

    #[test]
    fn a_braced_block_is_scanned_as_code() {
        assert_eq!(names("@{[ join ',', @parts ]}"), vec!["@parts"]);
        assert_eq!(names("${ $ref }"), vec!["$ref"]);
    }

    #[test]
    fn a_dereference_names_the_reference() {
        assert_eq!(names("$$ref"), vec!["$ref"]);
        assert_eq!(names("@$ref"), vec!["$ref"]);
    }

    #[test]
    fn a_subscript_after_a_dereference_belongs_to_it() {
        // `$$argv[0]` is an element of `@{$argv}`; `$argv` itself is a scalar.
        assert_eq!(names("$$argv[0]"), vec!["$argv"]);
        assert_eq!(names("$$row{name}"), vec!["$row"]);
        assert_eq!(names("$$argv[$i]"), vec!["$argv", "$i"]);
    }

    #[test]
    fn an_extended_pattern_ignores_its_comments() {
        let pattern = "(\")   # $quote\n  $real";
        assert_eq!(
            scan_extended(pattern)
                .into_iter()
                .map(|item| item.name)
                .collect::<Vec<_>>(),
            vec!["real"]
        );
        // The offset still points where it did: the comment was blanked, not
        // removed.
        assert_eq!(
            scan_extended(pattern)[0].offset,
            pattern.find("$real").expect("it is there")
        );
        // A `#` inside a character class is a character, not a comment.
        assert_eq!(
            scan_extended("[#$a]")
                .into_iter()
                .map(|item| item.name)
                .collect::<Vec<_>>(),
            vec!["a"]
        );
    }

    #[test]
    fn a_last_index_names_the_array() {
        assert_eq!(names("$#items"), vec!["@items"]);
    }

    #[test]
    fn a_method_call_is_not_interpolated() {
        // perl interpolates `$obj` and then prints `->name` literally.
        assert_eq!(names("$obj->name"), vec!["$obj"]);
    }

    #[test]
    fn a_punctuation_variable_is_not_a_lexical() {
        assert_eq!(names("$@ $! $1 $_"), vec!["$_"]);
    }

    #[test]
    fn an_address_holds_no_array() {
        // `@example` here would be an array in perl too, so the honest answer
        // is that it is a use; what must not happen is `@` before a character
        // that cannot start a name being read as one.
        assert_eq!(names("foo@.com"), Vec::<String>::new());
        assert_eq!(names("100% @ once"), Vec::<String>::new());
    }

    #[test]
    fn an_anchor_is_not_a_variable() {
        assert_eq!(names("^foo$"), Vec::<String>::new());
        assert_eq!(names("(foo$|bar)"), Vec::<String>::new());
    }

    #[test]
    fn a_qualified_name_keeps_its_package() {
        assert_eq!(names("$Foo::bar"), vec!["$Foo::bar"]);
    }

    #[test]
    fn a_single_colon_ends_the_name() {
        assert_eq!(names("$filename: not found"), vec!["$filename"]);
        assert_eq!(names("${file}: bad"), vec!["$file"]);
    }

    #[test]
    fn the_offset_points_at_the_sigil() {
        let found = scan("hi $who there");
        assert_eq!(found[0].offset, 3);
        assert_eq!(found[0].len, 4);
    }
}
