//! Walking a tree of Perl files, and running a pass over it on every core.
//!
//! Both consumers of the declaration pass do this: `camello check` walks the
//! paths it was pointed at, and `camello lsp` walks the workspace in the
//! background at startup (`docs/lsp.md`, "The index"). It lives here rather
//! than in the command line because the language server cannot reach the
//! command line — the binary depends on the server crate and not the other way
//! round — and two copies of a worker pool is two places for a walk to start
//! following symlinks.

use std::path::{Path, PathBuf};

/// How many workers a run of `items` gets.
#[must_use]
pub fn worker_count(jobs: Option<usize>, items: usize) -> usize {
    jobs.filter(|&jobs| jobs > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
        })
        .min(items)
        .max(1)
}

/// Run `job` over every item, on `jobs` threads, and answer in input order.
///
/// Scoped threads and an atomic cursor: the items are borrowed rather than
/// moved, and the results land in per-item slots so that the order a reader
/// sees is the order they were asked in, whatever order they finished in.
pub fn in_parallel<T, R>(items: &[T], jobs: Option<usize>, job: impl Fn(&T) -> R + Sync) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let workers = worker_count(jobs, items.len());

    if workers == 1 {
        return items.iter().map(job).collect();
    }

    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let job = &job;
    let slots = &slots;
    let next = &next;

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else { return };
                let result = job(item);
                *slots[index]
                    .lock()
                    .expect("no worker panics while holding this") = Some(result);
            });
        }
    });

    slots
        .iter()
        .map(|slot| {
            slot.lock()
                .expect("the workers are finished")
                .take()
                .expect("every slot was filled")
        })
        .collect()
}

/// The Perl files under a path, or the path itself when it names a file.
///
/// Recursive, sorted, and it does not follow a symlink found below a
/// requested root: besides escaping the root, a link to an ancestor would
/// recurse forever. A link the caller named itself is still followed, because
/// naming it is asking for it.
pub fn collect_files(
    path: &Path,
    extensions: &[&str],
    into: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    if path.is_file() {
        into.push(path.to_path_buf());
        return Ok(());
    }
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no such file or directory: {}", path.display()),
        ));
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for entry in entries {
        let metadata = std::fs::symlink_metadata(&entry)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files(&entry, extensions, into)?;
        } else if entry
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            into.push(entry);
        }
    }
    Ok(())
}
