//! Where a source comes from and how it goes back: the encodings it may be
//! in, reading it from a file, standard input or `-e`, and writing it back
//! over the file it came from.
//!
//! `format`, `check` and `dev` all read through here, so that a source is
//! decoded the same way whichever command asked for it.

use encoding_rs::Encoding;
use miette::{IntoDiagnostic, Result};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The encodings a source may be in, in the order they are tried.
///
/// One candidate is the whole of the usual case: the file is utf-8, or it is
/// not and that is something to hear about rather than to guess around. A tree
/// older than utf-8 is the other case — some of it euc-jp, some of it already
/// converted, and no flag that is right for the run because the encoding is a
/// property of each file. So the run names what a file may be, and the file
/// answers: the first candidate that reads its bytes without replacing any of
/// them is the one it is in, and the one it is written back in.
///
/// Order is what settles a file that more than one candidate can read. Every
/// candidate reads pure ASCII, and euc-jp bytes are rarely valid utf-8, so
/// utf-8 first is the order that means "already converted, or not yet".
pub(crate) struct Encodings(Vec<&'static Encoding>);

impl Encodings {
    /// The candidates named on the command line, or utf-8 when none were.
    pub(crate) fn parse(names: Option<&str>) -> Result<Self> {
        let Some(names) = names else {
            return Ok(Self(vec![encoding_rs::UTF_8]));
        };

        let mut candidates: Vec<&'static Encoding> = Vec::new();
        for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            let encoding = Encoding::for_label(name.as_bytes())
                .ok_or_else(|| miette::miette!("Unknown encoding: {name}"))?;
            // A candidate that already had its turn cannot decode anything the
            // earlier one did not.
            if !candidates.contains(&encoding) {
                candidates.push(encoding);
            }
        }
        if candidates.is_empty() {
            return Err(miette::miette!("--encoding names no encoding"));
        }
        Ok(Self(candidates))
    }

    /// Decode `bytes` with the first candidate that reads all of them.
    ///
    /// `None` is a source that none of them could read: rejected rather than
    /// decoded with replacement characters, which would be a rewrite of the
    /// file's contents wearing the name of a formatting run.
    pub(crate) fn decode(&self, bytes: &[u8]) -> Option<(String, &'static Encoding)> {
        self.0.iter().find_map(|encoding| {
            let (decoded, _, had_errors) = encoding.decode(bytes);
            (!had_errors).then(|| (decoded.into_owned(), *encoding))
        })
    }

    /// What to write text that never came from a file — `-e`, and its escaping
    /// sibling — in: the first thing the run said the sources are.
    pub(crate) fn first(&self) -> &'static Encoding {
        self.0[0]
    }

    /// `utf-8`, or `utf-8, euc-jp`: the candidates, for a message that has to
    /// say which ones were tried.
    pub(crate) fn names(&self) -> String {
        self.0
            .iter()
            .map(|encoding| encoding.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A source, what to call it in a message, and the encoding it turned out to be
/// in — which is the one it is written back in.
pub(crate) fn read_source(
    path: Option<&Path>,
    eval: Option<String>,
    eval_escape: Option<String>,
    encodings: &Encodings,
) -> Result<(String, String, &'static Encoding)> {
    if let Some(code) = eval.or_else(|| eval_escape.map(|code| interpret_escape_sequences(&code))) {
        return Ok((code, "<command-line>".to_string(), encodings.first()));
    }

    // What the source is called in the result, and in the refusal.
    let (bytes, name, described) = if let Some(path) = path {
        let bytes = fs::read(path).into_diagnostic()?;
        let name = path.display().to_string();
        let described = format!("'{name}'");
        (bytes, name, described)
    } else {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).into_diagnostic()?;
        (bytes, "<stdin>".to_string(), "stdin".to_string())
    };
    let Some((decoded, encoding)) = encodings.decode(&bytes) else {
        return Err(miette::miette!(
            "{described} is not decodable as {}; refusing lossy formatting",
            encodings.names()
        ));
    };
    Ok((decoded, name, encoding))
}

fn encode_to_vec(contents: &str, encoding: &'static Encoding) -> Result<Vec<u8>> {
    if std::ptr::eq(encoding, encoding_rs::UTF_8) {
        return Ok(contents.as_bytes().to_vec());
    }

    let (encoded, _, had_errors) = encoding.encode(contents);
    if had_errors {
        return Err(miette::miette!(
            "Unable to encode formatted output using {}",
            encoding.name()
        ));
    }

    Ok(encoded.into_owned())
}

pub(crate) fn write_with_encoding(
    path: &Path,
    contents: &str,
    encoding: &'static Encoding,
) -> Result<()> {
    let encoded = encode_to_vec(contents, encoding)?;
    // Preserve the usual meaning of explicitly formatting a symlink: update
    // its target rather than replacing the link itself with a regular file.
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::canonicalize(path).into_diagnostic()?
        }
        _ => path.to_path_buf(),
    };
    atomic_write(&target, &encoded)
}

/// Replace `path` only after the complete output has reached a sibling file.
///
/// Keeping the temporary file in the same directory makes the final rename
/// atomic on the target filesystem. Existing permissions are retained.
fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    /// How many names to try for the temporary file before giving up. Each
    /// carries the process id and a fresh sequence number, so a name already
    /// taken is a leftover from a run that died, and a hundred of them is a
    /// directory something else is wrong with.
    const ATTEMPTS: usize = 100;
    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output");
    let permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());

    let (temporary, mut file) = (0..ATTEMPTS)
        .find_map(|_| {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(
                ".{name}.camello.{}.{}.tmp",
                std::process::id(),
                sequence
            ));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => Some(Ok((temporary, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a temporary output file",
            ))
        })
        .into_diagnostic()?;

    let result = (|| -> std::io::Result<()> {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.into_diagnostic()
}

/// `-E`: the text with `\n`, `\t`, `\r`, `\\` and the quotes unescaped; any
/// other backslash is kept as it is.
fn interpret_escape_sequences(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some('n') => {
                    chars.next(); // consume 'n'
                    result.push('\n');
                }
                Some('t') => {
                    chars.next(); // consume 't'
                    result.push('\t');
                }
                Some('r') => {
                    chars.next(); // consume 'r'
                    result.push('\r');
                }
                Some('\\') => {
                    chars.next(); // consume second '\'
                    result.push('\\');
                }
                Some('"') => {
                    chars.next(); // consume '"'
                    result.push('"');
                }
                Some('\'') => {
                    chars.next(); // consume '\''
                    result.push('\'');
                }
                _ => {
                    // Unknown escape sequence, keep as is
                    result.push(ch);
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_interpret_escape_sequences() {
        assert_eq!(interpret_escape_sequences("hello\\nworld"), "hello\nworld");
        assert_eq!(interpret_escape_sequences("tab\\there"), "tab\there");
        assert_eq!(interpret_escape_sequences("quote\\\"test"), "quote\"test");
        assert_eq!(
            interpret_escape_sequences("backslash\\\\test"),
            "backslash\\test"
        );
        assert_eq!(interpret_escape_sequences("normal text"), "normal text");
        assert_eq!(
            interpret_escape_sequences("print 1\\nif yes();"),
            "print 1\nif yes();"
        );
    }

    #[test]
    fn candidates_are_named_once_each_and_must_be_known() {
        let parsed = |names: &str| Encodings::parse(Some(names));

        assert_eq!(
            parsed("utf-8, euc-jp").expect("known").names(),
            "UTF-8, EUC-JP"
        );
        // The same encoding twice cannot decode anything its first turn did not.
        assert_eq!(parsed("utf-8,utf8").expect("known").names(), "UTF-8");
        assert!(parsed("invalid-encoding-name").is_err());
        assert!(parsed(",").is_err());
        // None of them named is utf-8, which is what it was before candidates.
        assert_eq!(Encodings::parse(None).expect("a default").names(), "UTF-8");
    }

    #[test]
    fn decoding_errors_are_rejected_instead_of_replaced() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        let path = dir.path().join("invalid.pl");
        fs::write(&path, [0xff, 0xfe, b'\n'])?;

        let result = read_source(Some(&path), None, None, &Encodings::parse(None)?);

        assert!(result.is_err());
        assert_eq!(fs::read(&path)?, [0xff, 0xfe, b'\n']);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_through_a_symlink_preserves_the_link() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::os::unix::fs::symlink;

        let dir = tempdir()?;
        let target = dir.path().join("target.pl");
        let link = dir.path().join("link.pl");
        fs::write(&target, "old\n")?;
        symlink(&target, &link)?;

        write_with_encoding(&link, "new\n", encoding_rs::UTF_8)?;

        assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert_eq!(fs::read_to_string(&target)?, "new\n");
        Ok(())
    }
}
