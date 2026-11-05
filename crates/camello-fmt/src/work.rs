//! Counts of the formatter's units of work, for the tests that guard its
//! complexity.
//!
//! A timing depends on the machine and on what else it is doing; a count of
//! steps does not, so a test can hold the formatter to a shape — one build per
//! format, one flatness answer per block — without being flaky. Outside tests
//! [`tick`] compiles to nothing.

/// A unit of work worth counting.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Work {
    /// A Doc built from a tree.
    Build,
    /// A Doc rendered into lines.
    Render,
    /// A block's "can this be flat" answered rather than looked up.
    FlatBlock,
    /// A line's column read by the alignment pass.
    AlignColumn,
}

#[cfg(test)]
thread_local! {
    static COUNTS: std::cell::RefCell<[usize; 4]> = const { std::cell::RefCell::new([0; 4]) };
}

#[inline(always)]
pub(crate) fn tick(work: Work) {
    #[cfg(test)]
    COUNTS.with(|counts| counts.borrow_mut()[work as usize] += 1);
    #[cfg(not(test))]
    let _ = work;
}

/// How many of each unit `run` did on this thread.
#[cfg(test)]
pub(crate) fn count(run: impl FnOnce()) -> Counts {
    COUNTS.with(|counts| *counts.borrow_mut() = [0; 4]);
    run();
    Counts(COUNTS.with(|counts| *counts.borrow()))
}

#[cfg(test)]
pub(crate) struct Counts([usize; 4]);

#[cfg(test)]
impl Counts {
    pub(crate) fn of(&self, work: Work) -> usize {
        self.0[work as usize]
    }
}
